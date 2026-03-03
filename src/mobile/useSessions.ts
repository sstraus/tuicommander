import { createSignal, onCleanup } from "solid-js";
import { rpc } from "../transport";
import { listen } from "../invoke";
import { appLogger } from "../stores/appLogger";

/** Server-side accumulated state for a session (matches Rust SessionState) */
export interface SessionState {
  awaiting_input: boolean;
  question_text?: string;
  rate_limited: boolean;
  retry_after_ms?: number;
  usage_limit_pct?: number;
  is_busy: boolean;
  last_activity_ms: number;
  agent_type?: string;
  last_error?: string;
  agent_intent?: string;
  current_task?: string;
  last_prompt?: string;
  progress?: number;
  suggested_actions?: string[];
}

/** Session info returned by GET /sessions (matches Rust SessionInfo) */
export interface SessionInfo {
  session_id: string;
  cwd: string | null;
  worktree_path: string | null;
  worktree_branch: string | null;
  state?: SessionState;
}

const POLL_INTERVAL_MS = 3_000;

/**
 * Thin hook that polls GET /sessions every 3s and subscribes to SSE for
 * real-time session create/close events between polls.
 *
 * Returns reactive signals for the session list, loading state, and error.
 */
export function useSessions() {
  const [sessions, setSessions] = createSignal<SessionInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [refreshing, setRefreshing] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let refreshToken = 0;

  async function fetchSessions() {
    try {
      const result = await rpc<SessionInfo[]>("list_active_sessions");
      setSessions(result);
      setError(null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      appLogger.warn("network", `Failed to fetch sessions: ${msg}`);
    } finally {
      setLoading(false);
      // NOTE: do NOT set refreshing here — refresh() manages its own lifecycle
      // via the refreshToken guard to avoid race conditions with concurrent fetches.
    }
  }

  // Initial fetch
  fetchSessions();

  // Poll every 3s
  const timer = setInterval(fetchSessions, POLL_INTERVAL_MS);
  onCleanup(() => clearInterval(timer));

  // SSE subscription for real-time create/close events between polls
  // This triggers an immediate refetch so the UI updates faster than the poll interval
  const unlistenCreated = listen<{ session_id: string; cwd: string | null }>(
    "session-created",
    () => fetchSessions(),
  );
  const unlistenClosed = listen<{ session_id: string }>(
    "session-closed",
    () => fetchSessions(),
  );

  onCleanup(() => {
    unlistenCreated.then((fn) => fn());
    unlistenClosed.then((fn) => fn());
  });

  /** Force an immediate refresh (sets refreshing=true while in-flight) */
  function refresh() {
    const token = ++refreshToken;
    setRefreshing(true);
    fetchSessions().catch(() => {}).finally(() => {
      if (refreshToken === token) setRefreshing(false);
    });
  }

  /** Count of sessions with pending questions */
  function questionCount(): number {
    return sessions().filter((s) => s.state?.awaiting_input).length;
  }

  return { sessions, loading, refreshing, error, refresh, questionCount };
}
