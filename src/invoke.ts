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

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { isTauri, owningConnectionFor, rpc } from "./transport";
import { emitSseLocalEvent, subscribeSse } from "./transport/sse";
import { backendMismatchMessage } from "./utils/backendMismatch";

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

// Browser mode: lazily resolved HTTP transport. transport.ts is already a static
// import (isTauri above), so rpc is wired statically — no dynamic import needed.
let _httpInvoke: InvokeFn | undefined;

function getHttpInvoke(): Promise<InvokeFn> {
	if (_httpInvoke) return Promise.resolve(_httpInvoke);
	_httpInvoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => rpc<T>(cmd, args ?? {});
	return Promise.resolve(_httpInvoke);
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
	"get_diff_stats",
	"get_changed_files",
	"get_file_diff",
	"get_git_branches",
	"get_merged_branches",
	"get_recent_commits",
	"get_remote_url",
	"get_github_status",
	"get_shell_state",
	"get_last_prompt",
	"load_config",
	"load_agents_config",
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

function invokeLocal<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	const result = args !== undefined ? tauriInvoke<T>(cmd, args) : tauriInvoke<T>(cmd);
	if (!import.meta.env.DEV) return result;
	return result.catch((failure: unknown) => {
		const message = failure instanceof Error ? failure.message : failure;
		if (message === `Command ${cmd} not found`) {
			throw new Error(backendMismatchMessage(cmd), { cause: failure });
		}
		throw failure;
	});
}

export function invoke<T>(cmd: string, args?: Record<string, unknown>, connectionId?: string | null): Promise<T> {
	// A recorded event owns its backend even if repository ownership changes later.
	// null pins the local backend; undefined keeps ordinary argument-based routing.
	if (connectionId !== undefined) {
		if (connectionId === null && isTauri()) return invokeLocal<T>(cmd, args);
		return rpc<T>(cmd, args ?? {}, connectionId);
	}
	if (isTauri()) {
		// A repository held by another machine is not reachable over this process's
		// IPC. The desktop path skips rpc() entirely, so it has to ask the same
		// resolver rpc() asks — otherwise every call from a desktop satellite runs
		// against the local backend and quietly answers about the wrong disk.
		const owner = owningConnectionFor(cmd, args ?? {});
		if (owner) return rpc<T>(cmd, args ?? {}, owner);
		if (DEDUP_COMMANDS.has(cmd)) {
			const key = args !== undefined ? `${cmd}:${JSON.stringify(args)}` : cmd;
			const existing = _inflight.get(key) as Promise<T> | undefined;
			if (existing) return existing;
			const promise = invokeLocal<T>(cmd, args).finally(() => _inflight.delete(key));
			_inflight.set(key, promise as Promise<unknown>);
			return promise;
		}
		return invokeLocal<T>(cmd, args);
	}
	if (_httpInvoke) return _httpInvoke<T>(cmd, args);
	return getHttpInvoke().then((fn) => fn<T>(cmd, args));
}

/** Dispatch HTTP response events to the same browser subscribers as SSE. */
export function emitLocalEvent(event: string, payload: unknown): void {
	emitSseLocalEvent(event, payload);
}

/**
 * Wrap a Tauri UnlistenFn (from `@tauri-apps/api/event` `listen`/`once`, or any
 * Window/Webview method built on it — `.listen()`, `.onCloseRequested()`, etc.)
 * so repeat calls are a safe no-op.
 *
 * Tauri's UnlistenFn is `async () => _unlisten(...)`. A second call — a duplicate
 * dispose, or one racing a webview reload that already cleared the backend
 * registry — throws *inside* that async fn and surfaces as a rejected promise,
 * not a sync throw, hence the wrap + `.catch()`. The outer try/catch covers the
 * (defensive) sync-throw path. The listener is already gone either way; swallow.
 * Every caller of a Tauri listen-returning API must dispose through this guard
 * instead of calling the raw UnlistenFn directly.
 */
export function guardTauriUnlisten(unlisten: () => unknown): () => void {
	let disposed = false;
	return () => {
		if (disposed) return;
		disposed = true;
		try {
			void Promise.resolve(unlisten()).catch(() => {});
		} catch {
			// listener already gone
		}
	};
}

export function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
	if (isTauri()) {
		return tauriListen<T>(event, handler).then((unlisten) => guardTauriUnlisten(unlisten));
	}

	return Promise.resolve(
		subscribeSse({
			[event]: (payload) => handler({ payload: payload as T }),
		}),
	);
}
