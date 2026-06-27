import { appLogger } from "../stores/appLogger";
import { isTauri, rpc } from "../transport";
import type { OrchestratorStats, PtyConfig } from "../types";
import { clearShellFamilyCache, getShellFamily, sendCommand as sendCommandUtil } from "../utils/sendCommand";
import { browserCreatedSessions } from "./useAppInit";

/** Pre-generate a session id for browser-mode creates and register it locally
 *  BEFORE the create RPC. The backend's `session-created` event is delivered to
 *  the browser over SSE, which can arrive before the RPC's HTTP response — so
 *  registering the id up front closes that race window and the echo is dropped
 *  by the `session-created` listener instead of spawning a duplicate "PTY:" tab.
 *  Desktop (Tauri) has no such echo race, so the backend mints the id there. */
function preRegisterBrowserSessionId(): string | undefined {
	if (isTauri()) return undefined;
	const id = crypto.randomUUID();
	browserCreatedSessions.add(id);
	return id;
}

/** Worktree configuration */
interface WorktreeConfig {
	task_name: string;
	base_repo: string;
	branch: string | null;
	create_branch: boolean;
}

/** PTY session metrics from the Rust backend */
export interface SessionMetrics {
	total_spawned: number;
	failed_spawns: number;
	active_sessions: number;
	bytes_emitted: number;
	pauses_triggered: number;
}

/** Worktree creation result */
interface WorktreeResult {
	session_id: string;
	worktree_path: string;
	branch: string | null;
}

/** Active session info returned by list_active_sessions */
export interface ActiveSessionInfo {
	session_id: string;
	cwd: string | null;
	worktree_path: string | null;
	worktree_branch: string | null;
	display_name?: string | null;
}

/** PTY hook for managing terminal sessions */
export function usePty() {
	/** Check if we can spawn a new session */
	async function canSpawn(): Promise<boolean> {
		try {
			return await rpc<boolean>("can_spawn_session");
		} catch (err) {
			appLogger.error("terminal", "Failed to check session limit", err);
			return false;
		}
	}

	/** Create a new PTY session */
	async function createSession(config: PtyConfig): Promise<string> {
		const requestedId = preRegisterBrowserSessionId();
		const sessionId = await rpc<string>("create_pty", {
			config: requestedId ? { ...config, session_id: requestedId } : config,
		});
		browserCreatedSessions.add(sessionId);
		return sessionId;
	}

	/** Create a PTY session with a git worktree */
	async function createSessionWithWorktree(
		ptyConfig: PtyConfig,
		worktreeConfig: WorktreeConfig,
	): Promise<WorktreeResult> {
		const requestedId = preRegisterBrowserSessionId();
		const result = await rpc<WorktreeResult>("create_pty_with_worktree", {
			pty_config: requestedId ? { ...ptyConfig, session_id: requestedId } : ptyConfig,
			worktree_config: worktreeConfig,
		});
		browserCreatedSessions.add(result.session_id);
		return result;
	}

	/** Write raw data to a PTY session */
	async function write(sessionId: string, data: string): Promise<void> {
		await rpc("write_pty", { sessionId, data });
	}

	/** Send a command to a PTY session with agent-aware Enter handling. */
	async function sendCommand(sessionId: string, text: string, agentType?: string | null): Promise<void> {
		const shellFamily = await getShellFamily(sessionId);
		await sendCommandUtil((data) => write(sessionId, data), text, agentType, shellFamily);
	}

	/** Resize a PTY session */
	async function resize(sessionId: string, rows: number, cols: number): Promise<void> {
		await rpc("resize_pty", { sessionId, rows, cols });
	}

	/** Pause PTY reader thread (flow control) */
	async function pause(sessionId: string): Promise<void> {
		await rpc("pause_pty", { sessionId });
	}

	/** Resume PTY reader thread (flow control) */
	async function resume(sessionId: string): Promise<void> {
		await rpc("resume_pty", { sessionId });
	}

	/** Query current kitty keyboard protocol flags for a session (0 = not active) */
	async function getKittyFlags(sessionId: string): Promise<number> {
		return await rpc<number>("get_kitty_flags", { sessionId });
	}

	/** Close a PTY session */
	async function close(sessionId: string, cleanupWorktree: boolean = false): Promise<void> {
		clearShellFamilyCache(sessionId);
		await rpc("close_pty", { sessionId, cleanupWorktree });
	}

	/** Get orchestrator stats */
	async function getStats(): Promise<OrchestratorStats> {
		return await rpc<OrchestratorStats>("get_orchestrator_stats");
	}

	/** List active worktrees */
	async function listWorktrees(): Promise<unknown[]> {
		return await rpc<unknown[]>("list_worktrees");
	}

	/** Get worktrees directory path (repo-aware when repoPath provided) */
	async function getWorktreesDir(repoPath?: string): Promise<string> {
		return await rpc<string>("get_worktrees_dir", { repoPath: repoPath ?? null });
	}

	/** Get PTY session metrics for observability */
	async function getMetrics(): Promise<SessionMetrics> {
		return await rpc<SessionMetrics>("get_session_metrics");
	}

	/** List all active PTY sessions for reconnection after frontend reload */
	async function listActiveSessions(): Promise<ActiveSessionInfo[]> {
		return await rpc<ActiveSessionInfo[]>("list_active_sessions");
	}

	return {
		canSpawn,
		createSession,
		createSessionWithWorktree,
		write,
		sendCommand,
		resize,
		pause,
		resume,
		getKittyFlags,
		close,
		getStats,
		getMetrics,
		listWorktrees,
		getWorktreesDir,
		listActiveSessions,
	};
}
