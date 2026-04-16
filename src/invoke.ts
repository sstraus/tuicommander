/**
 * Smart invoke/listen wrapper — zero overhead in Tauri mode, HTTP fallback in browser mode.
 *
 * Tauri mode: directly re-exports from @tauri-apps/api (statically resolved, zero overhead).
 * Browser mode: delegates to rpc() from transport.ts via lazy dynamic import.
 *
 * Since @tauri-apps/api is a dependency of the project, the static import is always
 * available at build time. In Tauri webview, the APIs work normally. In browser mode,
 * we intercept calls before they reach the Tauri APIs.
 */
import { isTauri } from "./transport";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

// Browser mode: lazily resolved HTTP transport
let _httpInvoke: InvokeFn | undefined;

function getHttpInvoke(): Promise<InvokeFn> {
  if (_httpInvoke) return Promise.resolve(_httpInvoke);
  return import("./transport").then(({ rpc }) => {
    _httpInvoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
      rpc<T>(cmd, args ?? {});
    return _httpInvoke;
  });
}

// ---------------------------------------------------------------------------
// In-flight dedup — coalesces concurrent identical read-only calls into a
// single Tauri IPC round-trip. Without this, 20 components reacting to
// "repo-changed" spawn 20 parallel git processes for the same repo.
// Browser mode already dedupes in transport.ts via isIdempotentRpc.
// ---------------------------------------------------------------------------

const _inflight = new Map<string, Promise<unknown>>();

/** Exposed for tests only — do not use in production code. */
export const _inflight_TEST_ONLY = _inflight;

/** Read-only Tauri commands safe to deduplicate. Mutations (stage, discard,
 *  commit, push) are never deduped — even identical args may have side effects. */
const DEDUP_COMMANDS = new Set([
  "get_repo_summary",
  "get_repo_structure",
  "get_repo_diff_stats",
  "get_repo_info",
  "get_git_diff",
  "get_github_status",
  "get_repo_pr_statuses",
  "check_github_circuit",
  "get_shell_state",
  "get_last_prompt",
  "load_config",
  "load_agents_config",
  "load_llm_api_config",
  "load_keybindings",
  "load_notification_config",
  "load_notes",
  "load_activity",
  "load_pane_layout",
  "load_repo_local_config",
  "load_mcp_upstreams",
  "get_mcp_upstream_status",
  "get_dictation_config",
  "get_dictation_status",
  "list_audio_devices",
  "get_model_info",
  "fetch_plugin_registry",
  "list_user_plugins",
  "check_has_custom_settings",
  "resolve_terminal_path",
  "stat_path",
  "search_files",
  "list_directory",
  "get_claude_usage_api",
]);

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    if (DEDUP_COMMANDS.has(cmd)) {
      const key = args !== undefined ? `${cmd}:${JSON.stringify(args)}` : cmd;
      const existing = _inflight.get(key) as Promise<T> | undefined;
      if (existing) return existing;
      const promise = (args !== undefined ? tauriInvoke<T>(cmd, args) : tauriInvoke<T>(cmd))
        .finally(() => _inflight.delete(key));
      _inflight.set(key, promise as Promise<unknown>);
      return promise;
    }
    return args !== undefined ? tauriInvoke<T>(cmd, args) : tauriInvoke<T>(cmd);
  }
  if (_httpInvoke) return _httpInvoke<T>(cmd, args);
  return getHttpInvoke().then((fn) => fn<T>(cmd, args));
}

// ---------------------------------------------------------------------------
// Browser-mode SSE listener — shared EventSource for all listen() calls
// ---------------------------------------------------------------------------

let _sseSource: EventSource | null = null;
/** Listeners registered before or after SSE connects */
const _sseListeners = new Map<string, Set<(payload: unknown) => void>>();

/** Get or create the shared SSE connection for browser mode */
function ensureSse(): EventSource {
  if (_sseSource && _sseSource.readyState !== EventSource.CLOSED) return _sseSource;

  const origin = typeof window !== "undefined" ? window.location.origin : "";
  _sseSource = new EventSource(`${origin}/events`);

  _sseSource.onerror = () => {
    // EventSource auto-reconnects; just log
    import("./stores/appLogger").then(({ appLogger }) =>
      appLogger.debug("network", "SSE connection error — will auto-reconnect"),
    );
  };

  // Re-attach listeners for all registered event types
  for (const eventType of _sseListeners.keys()) {
    attachSseEventType(eventType);
  }

  return _sseSource;
}

/** Attach a native SSE addEventListener for a given event type */
function attachSseEventType(eventType: string) {
  if (!_sseSource) return;
  _sseSource.addEventListener(eventType, ((sseEvent: MessageEvent) => {
    const listeners = _sseListeners.get(eventType);
    if (!listeners) return;
    try {
      const payload = JSON.parse(sseEvent.data);
      for (const handler of listeners) handler(payload);
    } catch {
      // Ignore parse errors
    }
  }) as EventListener);
}

export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  if (isTauri()) return tauriListen<T>(event, handler);

  // Browser mode: SSE via shared EventSource
  const wrappedHandler = (payload: unknown) => handler({ payload: payload as T });

  if (!_sseListeners.has(event)) {
    _sseListeners.set(event, new Set());
    // If SSE is already connected, attach this new event type
    if (_sseSource && _sseSource.readyState !== EventSource.CLOSED) {
      attachSseEventType(event);
    }
  }
  _sseListeners.get(event)!.add(wrappedHandler);

  // Ensure SSE connection exists
  ensureSse();

  return Promise.resolve(() => {
    const listeners = _sseListeners.get(event);
    if (listeners) {
      listeners.delete(wrappedHandler);
      if (listeners.size === 0) _sseListeners.delete(event);
    }
  });
}
