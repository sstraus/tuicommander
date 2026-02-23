import { activityStore } from "../stores/activityStore";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { prNotificationsStore } from "../stores/prNotifications";
import { repoSettingsStore } from "../stores/repoSettings";
import { notificationsStore } from "../stores/notifications";
import { mdTabsStore } from "../stores/mdTabs";
import { uiStore } from "../stores/ui";
import { pluginStore } from "../stores/pluginStore";
import { markdownProviderRegistry } from "./markdownProviderRegistry";
import { invoke, listen } from "../invoke";
import { LineBuffer } from "../utils/lineBuffer";
import { stripAnsi } from "../utils/stripAnsi";
import {
  INVOKE_WHITELIST,
  PluginCapabilityError,
} from "./types";
import type {
  Disposable,
  FsChangeEvent,
  HttpFetchOptions,
  HttpResponse,
  MarkdownProvider,
  OpenPanelOptions,
  OutputWatcher,
  PanelHandle,
  PluginCapability,
  PluginHost,
  TuiPlugin,
  RepoSnapshot,
  RepoListEntry,
  PrNotificationSnapshot,
  RepoSettingsSnapshot,
} from "./types";

/**
 * Central plugin lifecycle manager.
 *
 * Responsibilities:
 * - Calls plugin.onload(host) on register, plugin.onunload() on unregister
 * - Auto-disposes all plugin registrations (sections, watchers, providers) on unregister
 * - Dispatches raw PTY lines to registered OutputWatchers
 * - Dispatches structured Tauri events to registered typed handlers
 * - Provides tiered API surface to plugins (Tier 1-4)
 */
function createPluginRegistry() {
  // Active plugin → its aggregated Disposable (wraps all sub-registrations)
  const plugins = new Map<string, { plugin: TuiPlugin; disposable: Disposable }>();

  // Global watcher list — all watchers from all plugins, tagged with pluginId
  const outputWatchers: Array<{ pluginId: string; watcher: OutputWatcher }> = [];

  // Per-session LineBuffers for processRawOutput
  const lineBuffers = new Map<string, LineBuffer>();

  // Structured event handlers: type → list of { pluginId, handler }
  const structuredHandlers = new Map<
    string,
    Array<{ pluginId: string; handler: (payload: unknown, sessionId: string) => void }>
  >();

  // -------------------------------------------------------------------------
  // Capability checking
  // -------------------------------------------------------------------------

  function requireCapability(
    pluginId: string,
    capabilities: ReadonlySet<string> | null,
    required: PluginCapability,
  ): void {
    // null = built-in plugin, no restrictions
    if (capabilities === null) return;
    if (!capabilities.has(required)) {
      throw new PluginCapabilityError(pluginId, required);
    }
  }

  // -------------------------------------------------------------------------
  // Build the PluginHost surface for a given plugin
  // -------------------------------------------------------------------------

  /**
   * Build a PluginHost for a plugin.
   * @param pluginId - The plugin's unique ID
   * @param disposables - Mutable array to track disposables for auto-cleanup
   * @param capabilities - Set of declared capabilities, or null for built-in plugins (unrestricted)
   */
  function buildHost(
    pluginId: string,
    disposables: Disposable[],
    capabilities: ReadonlySet<string> | null = null,
    allowedUrls: readonly string[] = [],
  ): PluginHost {
    function track(d: Disposable): Disposable {
      disposables.push(d);
      return d;
    }

    const logger = pluginStore.getLogger(pluginId);

    return {
      // -- Tier 0: Logging --

      log(level, message, data) {
        logger.log(level, message, data);
      },

      // -- Tier 1: Activity Center + watchers + providers --

      registerSection(section) {
        return track(activityStore.registerSection(section));
      },

      registerOutputWatcher(watcher: OutputWatcher): Disposable {
        const entry = { pluginId, watcher };
        outputWatchers.push(entry);
        return track({
          dispose() {
            const idx = outputWatchers.indexOf(entry);
            if (idx >= 0) outputWatchers.splice(idx, 1);
          },
        });
      },

      registerStructuredEventHandler(type, handler) {
        const list = structuredHandlers.get(type) ?? [];
        structuredHandlers.set(type, list);
        const entry = { pluginId, handler };
        list.push(entry);
        return track({
          dispose() {
            const list = structuredHandlers.get(type);
            if (!list) return;
            const idx = list.indexOf(entry);
            if (idx >= 0) list.splice(idx, 1);
          },
        });
      },

      registerMarkdownProvider(scheme: string, provider: MarkdownProvider): Disposable {
        return track(markdownProviderRegistry.register(scheme, provider));
      },

      addItem(item) {
        activityStore.addItem(item);
      },

      removeItem(id) {
        activityStore.removeItem(id);
      },

      updateItem(id, updates) {
        activityStore.updateItem(id, updates);
      },

      // -- Tier 2: Read-only app state --

      getActiveRepo(): RepoSnapshot | null {
        const repo = repositoriesStore.getActive();
        if (!repo) return null;
        const branch = repo.activeBranch ? repo.branches[repo.activeBranch] : null;
        return {
          path: repo.path,
          displayName: repo.displayName,
          activeBranch: repo.activeBranch,
          worktreePath: branch?.worktreePath ?? null,
        };
      },

      getRepos(): RepoListEntry[] {
        return repositoriesStore.getPaths().map((path) => {
          const repo = repositoriesStore.get(path);
          return { path, displayName: repo?.displayName ?? path };
        });
      },

      getActiveTerminalSessionId(): string | null {
        const terminal = terminalsStore.getActive();
        return terminal?.sessionId ?? null;
      },

      getRepoPathForSession(sessionId: string): string | null {
        const termId = terminalsStore.getIds().find(
          (id: string) => terminalsStore.get(id)?.sessionId === sessionId,
        );
        if (!termId) return null;
        for (const repoPath of repositoriesStore.getPaths()) {
          const repo = repositoriesStore.get(repoPath);
          if (!repo) continue;
          for (const branch of Object.values(repo.branches)) {
            if (branch.terminals.includes(termId)) return repoPath;
          }
        }
        return null;
      },

      getPrNotifications(): PrNotificationSnapshot[] {
        return prNotificationsStore.getActive().map((n) => ({
          id: n.id,
          repoPath: n.repoPath,
          branch: n.branch,
          prNumber: n.prNumber,
          title: n.title,
          type: n.type,
        }));
      },

      getSettings(repoPath: string): RepoSettingsSnapshot | null {
        const effective = repoSettingsStore.getEffective(repoPath);
        if (!effective) return null;
        return {
          path: effective.path,
          displayName: effective.displayName,
          baseBranch: effective.baseBranch,
          color: effective.color,
        };
      },

      // -- Tier 3: Write actions (capability-gated) --

      async writePty(sessionId: string, data: string): Promise<void> {
        requireCapability(pluginId, capabilities, "pty:write");
        await invoke("write_pty", { id: sessionId, data });
      },

      openMarkdownPanel(title: string, contentUri: string): void {
        requireCapability(pluginId, capabilities, "ui:markdown");
        mdTabsStore.addVirtual(title, contentUri);
        if (!uiStore.state.markdownPanelVisible) {
          uiStore.toggleMarkdownPanel();
        }
      },

      async playNotificationSound(): Promise<void> {
        requireCapability(pluginId, capabilities, "ui:sound");
        await notificationsStore.testSound("question");
      },

      // -- Tier 3b: Filesystem operations --

      async readFile(absolutePath: string): Promise<string> {
        requireCapability(pluginId, capabilities, "fs:read");
        return invoke<string>("plugin_read_file", { path: absolutePath, pluginId });
      },

      async listDirectory(path: string, pattern?: string): Promise<string[]> {
        requireCapability(pluginId, capabilities, "fs:list");
        return invoke<string[]>("plugin_list_directory", { path, pattern: pattern ?? null, pluginId });
      },

      async watchPath(
        path: string,
        callback: (events: FsChangeEvent[]) => void,
        options?: { recursive?: boolean; debounceMs?: number },
      ): Promise<Disposable> {
        requireCapability(pluginId, capabilities, "fs:watch");
        const watchId = await invoke<string>("plugin_watch_path", {
          path,
          pluginId,
          recursive: options?.recursive ?? false,
          debounceMs: options?.debounceMs ?? 300,
        });
        const eventName = `plugin-fs-change-${pluginId}`;
        const unlisten = await listen<FsChangeEvent[]>(eventName, (event) => {
          try {
            callback(event.payload);
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            pluginStore.getLogger(pluginId).error(`watchPath callback threw: ${msg}`, err);
          }
        });
        const disposable: Disposable = {
          dispose() {
            unlisten();
            invoke("plugin_unwatch", { watchId, pluginId }).catch(() => {
              // Watcher may already be cleaned up on plugin unload
            });
          },
        };
        return track(disposable);
      },

      // -- Tier 3c: Panel UI --

      openPanel(options: OpenPanelOptions): PanelHandle {
        requireCapability(pluginId, capabilities, "ui:panel");
        const tabId = mdTabsStore.addPluginPanel(pluginId, options.title, options.html);
        if (!uiStore.state.markdownPanelVisible) {
          uiStore.toggleMarkdownPanel();
        }
        return {
          tabId,
          update(html: string) {
            mdTabsStore.updatePluginPanel(tabId, html);
          },
          close() {
            mdTabsStore.remove(tabId);
          },
        };
      },

      // -- Tier 3d: Credential access --

      async readCredential(serviceName: string): Promise<string | null> {
        requireCapability(pluginId, capabilities, "credentials:read");

        // First-use consent check for external plugins
        if (capabilities !== null) {
          const consentKey = `credential-consent-${serviceName}`;
          const existing = await invoke<string | null>("read_plugin_data", {
            pluginId,
            path: consentKey,
          });
          if (!existing) {
            // Show consent dialog
            const { confirm } = await import("@tauri-apps/plugin-dialog");
            const allowed = await confirm(
              `Plugin "${pluginId}" wants to read your credentials for "${serviceName}". Allow?`,
              { title: "Credential Access", kind: "warning" },
            );
            if (!allowed) {
              throw new Error(`User denied credential access for "${serviceName}"`);
            }
            await invoke("write_plugin_data", {
              pluginId,
              path: consentKey,
              content: "allowed",
            });
          }
        }

        return invoke<string | null>("plugin_read_credential", {
          serviceName,
          pluginId,
        });
      },

      // -- Tier 3d: HTTP requests --

      async httpFetch(url: string, options?: HttpFetchOptions): Promise<HttpResponse> {
        requireCapability(pluginId, capabilities, "net:http");
        return invoke<HttpResponse>("plugin_http_fetch", {
          url,
          method: options?.method ?? null,
          headers: options?.headers ?? null,
          body: options?.body ?? null,
          allowedUrls: [...allowedUrls],
          pluginId,
        });
      },

      // -- Tier 4: Scoped Tauri invoke --

      async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
        if (!INVOKE_WHITELIST.includes(cmd)) {
          throw new Error(`Plugin "${pluginId}": command "${cmd}" is not in the invoke whitelist`);
        }
        // Check capability for scoped invoke commands
        const capKey = `invoke:${cmd}` as PluginCapability;
        if (capabilities !== null && cmd !== "read_plugin_data" && cmd !== "write_plugin_data" && cmd !== "delete_plugin_data") {
          requireCapability(pluginId, capabilities, capKey);
        }
        return invoke<T>(cmd, args);
      },
    };
  }

  // -------------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------------

  /**
   * Register a plugin.
   * @param plugin - The plugin to register
   * @param capabilities - Optional set of declared capabilities for external plugins.
   *   Pass null or omit for built-in plugins (unrestricted access).
   */
  function register(plugin: TuiPlugin, capabilities?: string[], allowedUrls?: string[]): void {
    // Replace existing registration for same id
    if (plugins.has(plugin.id)) {
      unregister(plugin.id);
    }

    const disposables: Disposable[] = [];
    const capSet = capabilities ? new Set(capabilities) : null;
    const host = buildHost(plugin.id, disposables, capSet, allowedUrls ?? []);

    const pluginLogger = pluginStore.getLogger(plugin.id);

    try {
      plugin.onload(host);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(`[pluginRegistry] plugin "${plugin.id}" threw during onload:`, err);
      pluginLogger.error(`onload failed: ${msg}`, err);
      pluginStore.updatePlugin(plugin.id, { loaded: false, error: msg });
      for (const d of disposables) {
        try { d.dispose(); } catch { /* cleanup best-effort */ }
      }
      return;
    }

    pluginStore.updatePlugin(plugin.id, { loaded: true, error: null });

    plugins.set(plugin.id, {
      plugin,
      disposable: {
        dispose() {
          for (const d of disposables) {
            try { d.dispose(); } catch { /* ignore */ }
          }
        },
      },
    });
  }

  function unregister(id: string): void {
    const entry = plugins.get(id);
    if (!entry) return;
    plugins.delete(id);
    try {
      entry.plugin.onunload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(`[pluginRegistry] plugin "${id}" threw during onunload:`, err);
      pluginStore.getLogger(id).error(`onunload failed: ${msg}`, err);
    }
    entry.disposable.dispose();
    pluginStore.updatePlugin(id, { loaded: false });
  }

  // -------------------------------------------------------------------------
  // Dispatch
  // -------------------------------------------------------------------------

  /**
   * Dispatch a clean (ANSI-stripped) PTY line to all registered OutputWatchers.
   * Regex matching runs synchronously (cheap), but onMatch callbacks are
   * deferred via queueMicrotask so slow handlers don't block terminal.write().
   */
  function dispatchLine(cleanLine: string, sessionId: string): void {
    for (const { pluginId, watcher } of outputWatchers) {
      const { pattern, onMatch } = watcher;
      // Reset global regex state before each test to avoid position carry-over
      if (pattern.global) pattern.lastIndex = 0;
      const match = pattern.exec(cleanLine);
      if (match) {
        queueMicrotask(() => {
          try {
            onMatch(match, sessionId);
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            console.error("[pluginRegistry] watcher threw:", err);
            pluginStore.getLogger(pluginId).error(`OutputWatcher threw: ${msg}`, err);
          }
        });
      }
    }
  }

  /**
   * Accept a raw PTY data chunk, reassemble complete lines, strip ANSI, and
   * dispatch each clean line to all registered OutputWatchers.
   *
   * Called inside handlePtyData() BEFORE terminal.write() so that plugins
   * observe every byte in the same order xterm does.
   */
  function processRawOutput(data: string, sessionId: string): void {
    if (outputWatchers.length === 0) return; // fast path: no watchers
    let buf = lineBuffers.get(sessionId);
    if (!buf) {
      buf = new LineBuffer();
      lineBuffers.set(sessionId, buf);
    }
    const lines = buf.push(data);
    for (const line of lines) {
      const clean = stripAnsi(line);
      dispatchLine(clean, sessionId);
    }
  }

  /**
   * Dispatch a structured Tauri event to all registered handlers for the type.
   * Handlers are deferred via queueMicrotask to avoid blocking the event loop.
   */
  function dispatchStructuredEvent(type: string, payload: unknown, sessionId: string): void {
    const handlers = structuredHandlers.get(type);
    if (!handlers) return;
    for (const { pluginId, handler } of handlers) {
      queueMicrotask(() => {
        try {
          handler(payload, sessionId);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          console.error(`[pluginRegistry] structured handler (plugin "${pluginId}", type "${type}") threw:`, err);
          pluginStore.getLogger(pluginId).error(`Structured handler "${type}" threw: ${msg}`, err);
        }
      });
    }
  }

  /** Clean up the LineBuffer for a closed PTY session. */
  function removeSession(sessionId: string): void {
    lineBuffers.delete(sessionId);
  }

  /** Remove all plugins and registrations (for testing). */
  function clear(): void {
    for (const id of [...plugins.keys()]) {
      unregister(id);
    }
    lineBuffers.clear();
  }

  return { register, unregister, processRawOutput, dispatchLine, dispatchStructuredEvent, removeSession, clear };
}

export const pluginRegistry = createPluginRegistry();
