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

export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  if (isTauri()) return tauriListen<T>(event, handler);
  // Browser mode: events not supported, return no-op unsubscribe
  return Promise.resolve(() => {});
}
