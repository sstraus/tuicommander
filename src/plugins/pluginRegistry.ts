import { activityStore } from "../stores/activityStore";
import { markdownProviderRegistry } from "./markdownProviderRegistry";
import { LineBuffer } from "../utils/lineBuffer";
import { stripAnsi } from "../utils/stripAnsi";
import type {
  Disposable,
  MarkdownProvider,
  OutputWatcher,
  PluginHost,
  TuiPlugin,
} from "./types";

/**
 * Central plugin lifecycle manager.
 *
 * Responsibilities:
 * - Calls plugin.onload(host) on register, plugin.onunload() on unregister
 * - Auto-disposes all plugin registrations (sections, watchers, providers) on unregister
 * - Dispatches raw PTY lines to registered OutputWatchers
 * - Dispatches structured Tauri events to registered typed handlers
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
  // Build the PluginHost surface for a given plugin
  // -------------------------------------------------------------------------

  function buildHost(pluginId: string, disposables: Disposable[]): PluginHost {
    function track(d: Disposable): Disposable {
      disposables.push(d);
      return d;
    }

    return {
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
        if (!structuredHandlers.has(type)) structuredHandlers.set(type, []);
        const entry = { pluginId, handler };
        structuredHandlers.get(type)!.push(entry);
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
    };
  }

  // -------------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------------

  function register(plugin: TuiPlugin): void {
    // Replace existing registration for same id
    if (plugins.has(plugin.id)) {
      unregister(plugin.id);
    }

    const disposables: Disposable[] = [];
    const host = buildHost(plugin.id, disposables);

    try {
      plugin.onload(host);
    } catch (err) {
      console.error(`[pluginRegistry] plugin "${plugin.id}" threw during onload:`, err);
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
    });
  }

  function unregister(id: string): void {
    const entry = plugins.get(id);
    if (!entry) return;
    plugins.delete(id);
    entry.plugin.onunload();
    entry.disposable.dispose();
  }

  // -------------------------------------------------------------------------
  // Dispatch
  // -------------------------------------------------------------------------

  /**
   * Dispatch a clean (ANSI-stripped) PTY line to all registered OutputWatchers.
   * Called synchronously in the PTY hot path — watchers MUST be fast.
   */
  function dispatchLine(cleanLine: string, sessionId: string): void {
    for (const { watcher } of outputWatchers) {
      const { pattern, onMatch } = watcher;
      // Reset global regex state before each test to avoid position carry-over
      if (pattern.global) pattern.lastIndex = 0;
      const match = pattern.exec(cleanLine);
      if (match) {
        try {
          onMatch(match, sessionId);
        } catch (err) {
          console.error("[pluginRegistry] watcher threw:", err);
        }
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
   */
  function dispatchStructuredEvent(type: string, payload: unknown, sessionId: string): void {
    const handlers = structuredHandlers.get(type);
    if (!handlers) return;
    for (const { pluginId, handler } of handlers) {
      try {
        handler(payload, sessionId);
      } catch (err) {
        console.error(`[pluginRegistry] structured handler (plugin "${pluginId}", type "${type}") threw:`, err);
      }
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
