import type { SessionStatus } from "../components/StatusBadge";
import type { SessionInfo } from "../useSessions";

/**
 * Derives the display status from session state.
 * Priority order: rate_limited > error > question > busy > idle
 */
export function deriveStatus(session: SessionInfo): SessionStatus {
  const s = session.state;
  if (!s) return "idle";
  if (s.rate_limited) return "rate-limited";
  if (s.last_error) return "error";
  if (s.awaiting_input) return "question";
  if (s.is_busy) return "busy";
  return "idle";
}
