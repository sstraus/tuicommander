import { rpc } from "../transport";
import { appLogger } from "../stores/appLogger";
import type { PtyConfig, OrchestratorStats } from "../types";

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
    return await rpc<string>("create_pty", { config });
  }

  /** Create a PTY session with a git worktree */
  async function createSessionWithWorktree(
    ptyConfig: PtyConfig,
    worktreeConfig: WorktreeConfig
  ): Promise<WorktreeResult> {
    return await rpc<WorktreeResult>("create_pty_with_worktree", {
      pty_config: ptyConfig,
      worktree_config: worktreeConfig,
    });
  }

  /** Write data to a PTY session */
  async function write(sessionId: string, data: string): Promise<void> {
    await rpc("write_pty", { sessionId, data });
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
