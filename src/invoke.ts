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

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
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
      appLogger.warn("network", "SSE connection error — will auto-reconnect"),
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
