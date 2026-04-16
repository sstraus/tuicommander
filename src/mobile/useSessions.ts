import { createSignal, onCleanup } from "solid-js";
import { rpc } from "../transport";
import { listen } from "../invoke";
import { appLogger } from "../stores/appLogger";

/** Server-side accumulated state for a session (matches Rust SessionState) */
export interface SessionState {
  awaiting_input: boolean;
  question_text?: string;
  question_confident?: boolean;
  rate_limited: boolean;
  retry_after_ms?: number;
  usage_limit_pct?: number;
  shell_state?: string;
  last_activity_ms: number;
  agent_type?: string;
  last_error?: string;
  agent_intent?: string;
  current_task?: string;
  active_sub_tasks?: number;
  last_prompt?: string;
  progress?: number;
  suggested_actions?: string[];
  slash_menu_items?: SlashMenuItem[];
  choice_prompt?: ChoicePrompt;
}

/** A single slash command menu item (matches Rust output_parser::SlashMenuItem) */
export interface SlashMenuItem {
  command: string;
  description: string;
  highlighted: boolean;
}

/** A numbered choice dialog (edit-confirm, bash-confirm, etc.).
 *  Matches Rust output_parser::ChoicePromptPayload. */
export interface ChoicePrompt {
  title: string;
  options: ChoiceOption[];
  dismiss_key?: string;
  amend_key?: string;
}

/** Single option in a ChoicePrompt. Matches Rust output_parser::ChoiceOption. */
export interface ChoiceOption {
  key: string;
  label: string;
  highlighted: boolean;
  destructive: boolean;
  hint?: string;
}

/** Session info returned by GET /sessions (matches Rust SessionInfo) */
export interface SessionInfo {
  session_id: string;
  cwd: string | null;
  worktree_path: string | null;
  worktree_branch: string | null;
  display_name?: string | null;
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

  // SSE subscription for shell-state changes — updates session in-place
  // without a full refetch, eliminating the 3s polling delay for idle/busy transitions.
  const unlistenPtyParsed = listen<{ session_id: string; parsed: { type: string; state?: string } }>(
    "pty-parsed",
    (event) => {
      const { session_id, parsed } = event.payload;
      if (parsed.type !== "shell-state" || !parsed.state) return;
      setSessions((prev) =>
        prev.map((s) =>
          s.session_id === session_id && s.state
            ? { ...s, state: { ...s.state, shell_state: parsed.state } }
            : s,
        ),
      );
    },
  );

  onCleanup(() => {
    unlistenCreated.then((fn) => fn()).catch(() => {});
    unlistenClosed.then((fn) => fn()).catch(() => {});
    unlistenPtyParsed.then((fn) => fn()).catch(() => {});
  });

  /** Force an immediate refresh (sets refreshing=true while in-flight) */
  function refresh() {
    const token = ++refreshToken;
    setRefreshing(true);
    fetchSessions().finally(() => {
      if (refreshToken === token) setRefreshing(false);
    });
  }

  /** Count of sessions with pending questions */
  function questionCount(): number {
    return sessions().filter((s) => s.state?.awaiting_input).length;
  }

  return { sessions, loading, refreshing, error, refresh, questionCount };
}
