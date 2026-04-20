import { appLogger } from "../stores/appLogger";
import { activityStore } from "../stores/activityStore";
import { statusBarTicker } from "../stores/statusBarTicker";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { prNotificationsStore } from "../stores/prNotifications";
import { repoSettingsStore } from "../stores/repoSettings";
import { notificationsStore } from "../stores/notifications";
import { mdTabsStore } from "../stores/mdTabs";
import { contextMenuActionsStore } from "../stores/contextMenuActionsStore";
import { sidebarPluginStore } from "../stores/sidebarPluginStore";

import { pluginStore } from "../stores/pluginStore";
import { keybindingsStore } from "../stores/keybindings";
import { markdownProviderRegistry } from "./markdownProviderRegistry";
import { fileIconRegistry } from "./fileIconRegistry";
import { dashboardRegistry } from "./dashboardRegistry";
import { invoke, listen } from "../invoke";
import { LineBuffer } from "../utils/lineBuffer";
import { stripAnsi } from "../utils/stripAnsi";
import { sanitizeSvgIcon } from "../utils/sanitizeSvg";
import { sendCommand, getShellFamily } from "../utils/sendCommand";
import {
  INVOKE_WHITELIST,
  NOTIFICATION_SOUNDS,
  PluginCapabilityError,
} from "./types";
import type {
  Disposable,
  FileIconProvider,
  FsChangeEvent,
  HttpFetchOptions,
  HttpResponse,
  MarkdownProvider,
  NotificationSound,
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
  StateChangeEvent,
  TerminalAction,
  TerminalStateSnapshot,
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
  const plugins = new Map<string, { plugin: TuiPlugin; disposable: Disposable; agentTypes: readonly string[] }>();

  // Plugin command handlers: action name (`plugin:<id>:<cmd>`) → run callback.
  // Populated via host.registerCommand(), consulted by the global keydown
  // dispatcher when a combo resolves to a plugin-namespaced action.
  const pluginCommandHandlers = new Map<string, () => void | Promise<void>>();

  // Panel message bridge: tabId → onMessage callback from plugin
  const panelMessageHandlers = new Map<string, (data: unknown) => void>();
  // Panel message bridge: tabId → send function (set by PluginPanel component)
  const panelSendChannels = new Map<string, (data: unknown) => void>();

  // Global watcher list — all watchers from all plugins, tagged with pluginId
  const outputWatchers: Array<{ pluginId: string; watcher: OutputWatcher }> = [];

  // Per-session LineBuffers for processRawOutput
  const lineBuffers = new Map<string, LineBuffer>();

  // Structured event handlers: type → list of { pluginId, handler }
  const structuredHandlers = new Map<
    string,
    Array<{ pluginId: string; handler: (payload: unknown, sessionId: string) => void }>
  >();

  // State change listeners for terminal/branch changes
  const stateChangeListeners: Array<{ pluginId: string; callback: (event: StateChangeEvent) => void }> = [];

  // -------------------------------------------------------------------------
  // Agent-type filtering
  // -------------------------------------------------------------------------

  // Fast lookup for paused plugins — avoids reactive store access in hot paths
  const pausedPlugins = new Set<string>();

  /** Returns true if a plugin is temporarily paused. */
  function isPluginPaused(pluginId: string): boolean {
    return pausedPlugins.has(pluginId);
  }

  /** Update the paused set (called from pluginStore.setPaused). */
  function setPluginPaused(pluginId: string, paused: boolean): void {
    if (paused) pausedPlugins.add(pluginId);
    else pausedPlugins.delete(pluginId);
  }

  /** Returns true if a plugin should receive events from a given session. */
  function pluginMatchesSession(pluginId: string, sessionId: string): boolean {
    const entry = plugins.get(pluginId);
    if (!entry || entry.agentTypes.length === 0) return true; // universal plugin
    const agentType = terminalsStore.getAgentTypeForSession(sessionId);
    return agentType !== null && entry.agentTypes.includes(agentType);
  }

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
      // Wrap in idempotent guard: plugins may manually dispose() and then the
      // registry disposes again on unload — second call must be a no-op.
      let disposed = false;
      const safe: Disposable = {
        dispose() {
          if (disposed) return;
          disposed = true;
          d.dispose();
        },
      };
      disposables.push(safe);
      return safe;
    }

    const logger = pluginStore.getLogger(pluginId);

    return {
      // -- Tier 0: Logging --

      log(level, message, data) {
        logger.log(level, message, data);
        appLogger.push(level, "plugin", `[${pluginId}] ${message}`, data);
      },

      // -- Tier 1: Activity Center + watchers + providers --

      registerSection(section) {
        return track(activityStore.registerSection({ ...section, pluginId }));
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

      registerFileIconProvider(provider: FileIconProvider): Disposable {
        requireCapability(pluginId, capabilities, "ui:file-icons");
        return track(fileIconRegistry.register(provider));
      },

      addItem(item) {
        if (item.icon) item.icon = sanitizeSvgIcon(item.icon);
        activityStore.addItem(item);
      },

      removeItem(id) {
        activityStore.removeItem(id);
      },

      updateItem(id, updates) {
        if (updates.icon) updates.icon = sanitizeSvgIcon(updates.icon);
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
        // Plugins should not see parked repos — they're considered dormant
        // from the SDK's point of view. (#1358-caf5)
        return repositoriesStore.getActivePaths().map((path) => {
          const repo = repositoriesStore.get(path);
          return { path, displayName: repo?.displayName ?? path };
        });
      },

      getActiveTerminalSessionId(): string | null {
        const terminal = terminalsStore.getActive();
        return terminal?.sessionId ?? null;
      },

      getRepoPathForSession(sessionId: string): string | null {
        const termId = terminalsStore.getTerminalForSession(sessionId);
        if (!termId) return null;
        return repositoriesStore.getRepoPathForTerminal(termId);
      },

      getSessionCwd(sessionId: string): string | null {
        const termId = terminalsStore.getTerminalForSession(sessionId);
        if (!termId) return null;
        const terminal = terminalsStore.get(termId);
        return terminal?.cwd ?? null;
      },

      async getClaudeProjectDir(repoPath: string): Promise<string | null> {
        requireCapability(pluginId, capabilities, "fs:read");
        return invoke<string | null>("claude_project_dir", { cwd: repoPath });
      },

      getActiveRepoPath(): string | null {
        return repositoriesStore.state.activeRepoPath;
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

      getTerminalState(): TerminalStateSnapshot | null {
        const terminal = terminalsStore.getActive();
        if (!terminal) return null;
        const repoPath = this.getRepoPathForSession(terminal.sessionId ?? "");
        return {
          sessionId: terminal.sessionId,
          shellState: terminal.shellState,
          agentType: terminal.agentType,
          agentActive: terminal.agentType !== null,
          awaitingInput: terminal.awaitingInput,
          repoPath,
        };
      },

      onStateChange(callback: (event: StateChangeEvent) => void): Disposable {
        const entry = { pluginId, callback };
        stateChangeListeners.push(entry);
        return track({
          dispose() {
            const idx = stateChangeListeners.indexOf(entry);
            if (idx >= 0) stateChangeListeners.splice(idx, 1);
          },
        });
      },

      // -- Tier 2b: Git read (capability-gated) --

      async getGitBranches(repoPath: string): Promise<Array<{ name: string; isCurrent: boolean }>> {
        requireCapability(pluginId, capabilities, "git:read");
        const raw = await invoke<Array<{ name: string; is_current: boolean }>>(
          "get_git_branches",
          { path: repoPath },
        );
        return (raw ?? []).map((b) => ({ name: b.name, isCurrent: b.is_current }));
      },

      async getRecentCommits(repoPath: string, count?: number): Promise<Array<{ hash: string; message: string; author: string; date: string }>> {
        requireCapability(pluginId, capabilities, "git:read");
        const raw = await invoke<Array<{ hash: string; message: string; author: string; date: string }>>(
          "get_recent_commits",
          { path: repoPath, count: count ?? 20 },
        );
        return raw ?? [];
      },

      async getGitDiff(repoPath: string, scope?: "staged" | "unstaged"): Promise<string> {
        requireCapability(pluginId, capabilities, "git:read");
        const raw = await invoke<string>("get_git_diff", { path: repoPath, scope: scope ?? null });
        return raw ?? "";
      },

      // -- Tier 3: Write actions (capability-gated) --

      registerTerminalAction(action: TerminalAction): Disposable {
        requireCapability(pluginId, capabilities, "ui:context-menu");
        // Wrap handler in a stale-plugin guard: after unregister, invocations are no-ops
        const guardedAction: TerminalAction = {
          ...action,
          action: (ctx) => {
            if (!plugins.has(pluginId)) return;
            action.action(ctx);
          },
        };
        return track(contextMenuActionsStore.registerAction(pluginId, guardedAction));
      },

      registerContextMenuAction(action: import("../stores/contextMenuActionsStore").ContextMenuAction): Disposable {
        requireCapability(pluginId, capabilities, "ui:context-menu");
        const guarded: import("../stores/contextMenuActionsStore").ContextMenuAction = {
          ...action,
          action: (ctx) => {
            if (!plugins.has(pluginId)) return;
            action.action(ctx);
          },
        };
        return track(contextMenuActionsStore.registerContextAction(pluginId, guarded));
      },

      registerSidebarPanel(options: import("../stores/sidebarPluginStore").SidebarPanelOptions) {
        requireCapability(pluginId, capabilities, "ui:sidebar");
        if (options.icon) options.icon = sanitizeSvgIcon(options.icon);
        const handle = sidebarPluginStore.registerPanel(pluginId, options);
        // Track dispose for auto-cleanup, but return the full handle
        track({ dispose: () => handle.dispose() });
        return handle;
      },

      async writePty(sessionId: string, data: string): Promise<void> {
        if (isPluginPaused(pluginId)) return;
        requireCapability(pluginId, capabilities, "pty:write");
        await invoke("write_pty", { sessionId, data });
      },

      async sendAgentInput(sessionId: string, text: string): Promise<void> {
        if (isPluginPaused(pluginId)) return;
        requireCapability(pluginId, capabilities, "pty:write");
        const agentType = terminalsStore.getAgentTypeForSession(sessionId);
        if (!agentType) {
          appLogger.warn("plugin", `[${pluginId}] sendAgentInput blocked — no active agent in session ${sessionId.slice(0, 8)}`);
          return;
        }
        const shellFamily = await getShellFamily(sessionId);
        await sendCommand(
          (data) => invoke("write_pty", { sessionId, data }),
          text,
          agentType,
          shellFamily,
        );
      },

      async readSessionOutput(sessionId: string, maxLines?: number): Promise<string> {
        requireCapability(pluginId, capabilities, "pty:read");
        return invoke<string>("plugin_read_session_output", {
          sessionId,
          maxLines: maxLines ?? null,
          pluginId,
        });
      },

      registerDashboard(options): Disposable {
        return track(
          dashboardRegistry.register({
            pluginId,
            label: options.label ?? "Dashboard",
            icon: options.icon,
            open: options.open,
          }),
        );
      },

      registerCommand(options): Disposable {
        const actionName = `plugin:${pluginId}:${options.id}`;
        keybindingsStore.registerDynamicAction({
          action: actionName,
          label: options.title,
          pluginId,
          defaultKey: options.defaultShortcut,
        });
        pluginCommandHandlers.set(actionName, options.run);
        return track({
          dispose() {
            keybindingsStore.unregisterDynamicAction(actionName);
            pluginCommandHandlers.delete(actionName);
          },
        });
      },

      openMarkdownPanel(title: string, contentUri: string): void {
        requireCapability(pluginId, capabilities, "ui:markdown");
        mdTabsStore.addVirtual(title, contentUri);
      },

      openMarkdownFile(absolutePath: string): void {
        requireCapability(pluginId, capabilities, "ui:markdown");
        mdTabsStore.add("", absolutePath);
      },

      async playNotificationSound(sound?: NotificationSound): Promise<void> {
        requireCapability(pluginId, capabilities, "ui:sound");
        const resolved: NotificationSound = NOTIFICATION_SOUNDS.includes(sound as NotificationSound)
          ? (sound as NotificationSound)
          : "info";
        if (sound !== undefined && resolved === "info") {
          appLogger.warn("plugin", `[${pluginId}] playNotificationSound: unknown sound "${sound}", defaulting to "info"`);
        }
        await notificationsStore.play(resolved);
      },

      // -- Tier 3b: Filesystem operations --

      async readFile(absolutePath: string): Promise<string> {
        requireCapability(pluginId, capabilities, "fs:read");
        return invoke<string>("plugin_read_file", { path: absolutePath, pluginId });
      },

      async readFileTail(absolutePath: string, maxBytes: number): Promise<string> {
        requireCapability(pluginId, capabilities, "fs:read");
        return invoke<string>("plugin_read_file_tail", { path: absolutePath, maxBytes, pluginId });
      },

      async listDirectory(
        path: string,
        pattern?: string,
        options?: { sortBy?: "name" | "mtime" },
      ): Promise<string[]> {
        requireCapability(pluginId, capabilities, "fs:list");
        return invoke<string[]>("plugin_list_directory", {
          path,
          pattern: pattern ?? null,
          sortBy: options?.sortBy ?? null,
          pluginId,
        });
      },

      async writeFile(absolutePath: string, content: string): Promise<void> {
        requireCapability(pluginId, capabilities, "fs:write");
        await invoke("plugin_write_file", { path: absolutePath, content, pluginId });
      },

      async renamePath(from: string, to: string): Promise<void> {
        requireCapability(pluginId, capabilities, "fs:rename");
        await invoke("plugin_rename_path", { from, to, pluginId });
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

      // -- Tier 3c: Status bar ticker --

      setTicker(options) {
        requireCapability(pluginId, capabilities, "ui:ticker");
        statusBarTicker.addMessage({
          id: options.id,
          pluginId,
          text: options.text,
          label: options.label,
          icon: options.icon ? sanitizeSvgIcon(options.icon) : undefined,
          priority: options.priority ?? 0,
          ttlMs: options.ttlMs ?? 60_000,
          onClick: options.onClick,
        });
      },

      clearTicker(id: string) {
        requireCapability(pluginId, capabilities, "ui:ticker");
        statusBarTicker.removeMessage(id, pluginId);
      },

      // -- Tier 3d: Panel UI --

      openPanel(options: OpenPanelOptions): PanelHandle {
        requireCapability(pluginId, capabilities, "ui:panel");
        const tabId = mdTabsStore.addPluginPanel(pluginId, options.title, options.html);
        // Register message handler for this panel
        if (options.onMessage) {
          panelMessageHandlers.set(tabId, options.onMessage);
        }
        return {
          tabId,
          update(html: string) {
            mdTabsStore.updatePluginPanel(tabId, html);
          },
          close() {
            panelMessageHandlers.delete(tabId);
            panelSendChannels.delete(tabId);
            mdTabsStore.remove(tabId);
          },
          send(data: unknown) {
            const sender = panelSendChannels.get(tabId);
            if (sender) sender(data);
          },
        };
      },

      // -- Tier 3e: Credential access --

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

      // -- Tier 3f: HTTP requests --

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

      // -- Tier 3g: CLI execution --

      async execCli(binary: string, args: string[], cwd?: string): Promise<string> {
        requireCapability(pluginId, capabilities, "exec:cli");
        return invoke<string>("plugin_exec_cli", {
          binary,
          args,
          cwd: cwd ?? null,
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
   * @param agentTypes - Optional list of agent types this plugin targets.
   *   Empty or omit for universal plugins.
   */
  async function register(plugin: TuiPlugin, capabilities?: string[], allowedUrls?: string[], agentTypes?: string[]): Promise<void> {
    // Replace existing registration for same id
    if (plugins.has(plugin.id)) {
      unregister(plugin.id);
    }

    const disposables: Disposable[] = [];
    const capSet = capabilities ? new Set(capabilities) : null;
    const host = buildHost(plugin.id, disposables, capSet, allowedUrls ?? []);

    const pluginLogger = pluginStore.getLogger(plugin.id);

    // Register capabilities on the Rust side before calling onload,
    // so Rust-gated commands (exec:cli, fs:read, etc.) work immediately.
    if (capabilities) {
      try {
        await invoke("register_loaded_plugin", {
          pluginId: plugin.id,
          capabilities: [...capabilities],
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        appLogger.error("plugin", `Plugin "${plugin.id}" Rust registration failed: ${msg}`, err);
        pluginLogger.error(`Rust registration failed: ${msg}`, err);
        pluginStore.updatePlugin(plugin.id, { loaded: false, error: msg });
        return;
      }
    }

    try {
      plugin.onload(host);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("plugin", `Plugin "${plugin.id}" onload failed: ${msg}`, err);
      pluginLogger.error(`onload failed: ${msg}`, err);
      pluginStore.updatePlugin(plugin.id, { loaded: false, error: msg });
      if (capabilities) {
        invoke("unregister_loaded_plugin", { pluginId: plugin.id }).catch(() => {});
      }
      for (const d of disposables) {
        try { d.dispose(); } catch { /* cleanup best-effort */ }
      }
      return;
    }

    plugins.set(plugin.id, {
      plugin,
      disposable: {
        dispose() {
          for (const d of disposables) {
            try { d.dispose(); } catch { /* ignore */ }
          }
        },
      },
      agentTypes: agentTypes ?? [],
    });

    pluginStore.updatePlugin(plugin.id, { loaded: true, error: null });
  }

  function unregister(id: string): void {
    const entry = plugins.get(id);
    if (!entry) return;
    plugins.delete(id);
    try {
      entry.plugin.onunload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("plugin", `Plugin "${id}" onunload failed: ${msg}`, err);
      pluginStore.getLogger(id).error(`onunload failed: ${msg}`, err);
    }
    entry.disposable.dispose();
    statusBarTicker.removeAllForPlugin(id);
    invoke("unregister_loaded_plugin", { pluginId: id }).catch(() => {});
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
      if (isPluginPaused(pluginId)) continue;
      if (!pluginMatchesSession(pluginId, sessionId)) continue;
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
            appLogger.error("plugin", `Plugin "${pluginId}" output watcher threw: ${msg}`, err);
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
      // Strip ANSI escapes, then backticks — Claude Code renders tokens as
      // markdown inline code (`path`), leaving literal backticks in clean text.
      const clean = stripAnsi(line).split("`").join("");
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
      if (isPluginPaused(pluginId)) continue;
      if (!pluginMatchesSession(pluginId, sessionId)) continue;
      queueMicrotask(() => {
        try {
          handler(payload, sessionId);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          appLogger.error("plugin", `Plugin "${pluginId}" structured handler "${type}" threw: ${msg}`, err);
          pluginStore.getLogger(pluginId).error(`Structured handler "${type}" threw: ${msg}`, err);
        }
      });
    }
  }

  /** Clean up the LineBuffer for a closed PTY session and notify plugins. */
  function removeSession(sessionId: string): void {
    lineBuffers.delete(sessionId);
    dispatchStructuredEvent("session-closed", {}, sessionId);
  }

  // Reentrancy guard for notifyStateChange: if a listener callback triggers
  // another state change synchronously (e.g. writes to a reactive store that
  // a caller effect tracks), naive dispatch would recurse and freeze the main
  // thread. We detect re-entry, defer nested events to a microtask, and log
  // the offending plugin so the root cause is visible.
  let dispatching = false;
  const pendingEvents: StateChangeEvent[] = [];

  function dispatchNow(event: StateChangeEvent): void {
    for (const { pluginId, callback } of stateChangeListeners) {
      if (isPluginPaused(pluginId)) continue;
      if (event.sessionId && !pluginMatchesSession(pluginId, event.sessionId)) continue;
      try {
        callback(event);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        appLogger.error("plugin", `State change listener threw: ${msg}`, err);
      }
    }
  }

  /** Notify all state change listeners of a terminal/branch state change */
  function notifyStateChange(event: StateChangeEvent): void {
    if (dispatching) {
      appLogger.warn(
        "plugin",
        `notifyStateChange re-entered while dispatching (type=${event.type}); deferring to microtask`,
      );
      pendingEvents.push(event);
      return;
    }
    dispatching = true;
    try {
      dispatchNow(event);
    } finally {
      dispatching = false;
    }
    if (pendingEvents.length > 0) {
      const drain = pendingEvents.splice(0, pendingEvents.length);
      queueMicrotask(() => {
        for (const e of drain) notifyStateChange(e);
      });
    }
  }

  /** Remove all plugins and registrations (for testing). */
  function clear(): void {
    for (const id of [...plugins.keys()]) {
      unregister(id);
    }
    lineBuffers.clear();
    stateChangeListeners.length = 0;
    panelMessageHandlers.clear();
    panelSendChannels.clear();
  }

  // -------------------------------------------------------------------------
  // Panel message bridge (used by PluginPanel component)
  // -------------------------------------------------------------------------

  /** Route a message from an iframe to the registered onMessage handler */
  function handlePanelMessage(tabId: string, data: unknown): void {
    const handler = panelMessageHandlers.get(tabId);
    if (handler) {
      try {
        handler(data);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        appLogger.error("plugin", `Panel message handler for tab "${tabId}" threw: ${msg}`, err);
      }
    }
  }

  /** Register a send channel for a panel (called by PluginPanel component on mount) */
  function registerPanelSendChannel(tabId: string, sender: (data: unknown) => void): void {
    panelSendChannels.set(tabId, sender);
  }

  /** Unregister a send channel (called by PluginPanel component on cleanup) */
  function unregisterPanelSendChannel(tabId: string): void {
    panelSendChannels.delete(tabId);
  }

  /**
   * Invoke a plugin command by its namespaced action name.
   * Returns true if a handler was found and called.
   * Used by the global keybinding dispatcher in App.tsx.
   */
  function invokePluginCommand(actionName: string): boolean {
    const handler = pluginCommandHandlers.get(actionName);
    if (!handler) return false;
    try {
      const result = handler();
      if (result instanceof Promise) {
        result.catch((err) => {
          appLogger.error("plugin", `Plugin command "${actionName}" failed`, err);
        });
      }
    } catch (err) {
      appLogger.error("plugin", `Plugin command "${actionName}" threw`, err);
    }
    return true;
  }

  return {
    register, unregister, processRawOutput, dispatchLine, dispatchStructuredEvent,
    notifyStateChange, removeSession, clear, setPluginPaused,
    handlePanelMessage, registerPanelSendChannel, unregisterPanelSendChannel,
    invokePluginCommand,
  };
}

export const pluginRegistry = createPluginRegistry();
