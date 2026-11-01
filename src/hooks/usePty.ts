import { appLogger } from "../stores/appLogger";
import { isTauri, owningConnectionFor, rpc } from "../transport";
import type { OrchestratorStats, PtyConfig } from "../types";
import { clearShellFamilyCache, getShellFamily, sendCommand as sendCommandUtil } from "../utils/sendCommand";
import { browserCreatedSessions } from "./useAppInit";

/** Pre-generate a session id for creates that travel over HTTP and register it
 *  locally BEFORE the create RPC. The backend's `session-created` event is
 *  delivered over SSE, which can arrive before the RPC's HTTP response — so
 *  registering the id up front closes that race window and the echo is dropped
 *  by the `session-created` listener instead of spawning a duplicate "PTY:" tab.
 *
 *  The desktop over Tauri IPC has no such echo race, but a desktop create for a
 *  REMOTE repo is routed to that machine's daemon and does, which is why the
 *  predicate is "not local", not "not Tauri" — the same one `rpc()` uses to
 *  decide whether `write_pty` needs its per-session queue. */
function preRegisterBrowserSessionId(command: string, args: Record<string, unknown>): string | undefined {
	if (isTauri() && !owningConnectionFor(command, args)) return undefined;
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

/** What the backend's idle gate did with an enqueued command. */
export interface EnqueuedCommand {
	/** The agent was idle: the text was typed and submitted right away. */
	typed: boolean;
	/** Commands still waiting, this one included when `typed` is false. */
	queued: number;
}

/** One command still parked in a session's idle gate. */
export interface QueuedCommand {
	/** Stable across drains — deleting by list position would race the FIFO. */
	id: number;
	text: string;
	/**
	 * Who parked it. `user_command` is the operator's own Compose entry;
	 * `notice` is a server wake pointing at the agent's inbox; `initial_prompt`
	 * is a spawned child's task still waiting for its TUI to be ready. All three
	 * block the composer identically, so all three are listed and deletable —
	 * hiding the server ones is what made a stuck queue undiagnosable.
	 */
	kind: "user_command" | "notice" | "initial_prompt";
}

/** Active session info returned by list_active_sessions */
export interface ActiveSessionInfo {
	session_id: string;
	cwd: string | null;
	worktree_path: string | null;
	worktree_branch: string | null;
	display_name?: string | null;
	display_name_is_custom: boolean;
	/** The name was given by the agent spawn that created the session, and no
	 *  user rename has replaced it. The agent's own OSC title must not override it. */
	display_name_from_spawn?: boolean;
	is_remote: boolean;
	/** Session (or `$TUIC_SESSION`) of the agent that spawned this one. Only the
	 *  resolved id is sent — never an unregistered caller's placeholder. */
	parent_session?: string | null;
	pty_description?: string | null;
	/** Terminal alias (e.g. `tu-3`). The only record of it after a WebView reload:
	 *  `term-alias-assigned` fires once, at spawn. */
	alias?: string | null;
	state?: {
		shell_state?: "busy" | "idle";
		agent_state?: "starting" | "working" | "awaiting_input" | "idle" | "completed";
		agent_type?: string | null;
		agent_intent?: string | null;
		last_prompt?: string | null;
		background_work?: boolean;
		queued_commands?: number;
		last_activity_ms?: number;
	} | null;
	/**
	 * Which remote machine runs this session; absent for a local one. The list
	 * holds both since #791-055e, and this is the only field that tells them
	 * apart — everything else about a mirrored row is shaped like a local one on
	 * purpose.
	 */
	connection_id?: string | null;
	/** Live agent identity; may differ from the PTY's session_id. */
	tuic_session?: string | null;
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
		const requestedId = preRegisterBrowserSessionId("create_pty", { config });
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
		const requestedId = preRegisterBrowserSessionId("create_pty_with_worktree", {
			pty_config: ptyConfig,
			worktree_config: worktreeConfig,
		});
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

	/** Send or insert text through the central agent-aware command path. */
	async function sendCommand(sessionId: string, text: string, agentType?: string | null, submit = true): Promise<void> {
		const shellFamily = await getShellFamily(sessionId);
		await sendCommandUtil((data) => write(sessionId, data), text, agentType, shellFamily, submit, sessionId);
	}

	/** Hand a command to the backend's idle gate instead of typing it now: it is
	 *  submitted immediately when the agent is idle, otherwise on the agent's next
	 *  busy→idle transition, so a running turn is never steered.
	 *  Rejects for non-agent sessions — a plain shell has no composer to wait for. */
	async function enqueueCommand(sessionId: string, text: string): Promise<EnqueuedCommand> {
		return await rpc<EnqueuedCommand>("enqueue_agent_command", {
			sessionId,
			text,
			idempotencyKey: crypto.randomUUID(),
		});
	}

	/** Drop every command still queued for a session. Returns how many were dropped. */
	async function clearQueuedCommands(sessionId: string): Promise<number> {
		return await rpc<number>("clear_queued_agent_commands", { sessionId });
	}

	/** The commands still queued for a session, in delivery order. */
	async function listQueuedCommands(sessionId: string): Promise<QueuedCommand[]> {
		return await rpc<QueuedCommand[]>("list_queued_agent_commands", { sessionId });
	}

	/** Drop one queued command. False when it was already typed. */
	async function removeQueuedCommand(sessionId: string, commandId: number): Promise<boolean> {
		return await rpc<boolean>("remove_queued_agent_command", { sessionId, commandId });
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
		enqueueCommand,
		clearQueuedCommands,
		listQueuedCommands,
		removeQueuedCommand,
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
