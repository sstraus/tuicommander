import { terminalVisualState } from "../../utils/terminalVisualState";
import type { SessionStatus } from "../components/StatusBadge";
import type { SessionInfo } from "../useSessions";

/**
 * Derives the display status from session state.
 * Priority order: rate_limited > error > question > shell_state
 */
export function deriveStatus(session: SessionInfo): SessionStatus {
	const s = session.state;
	if (!s) return "idle";
	if (s.rate_limited) return "rate-limited";
	const visual = terminalVisualState({
		error: !!s.last_error,
		question: s.awaiting_input || s.agent_state === "awaiting_input",
		stalled: s.stalled === true,
		busy: s.shell_state === "busy" || s.agent_state === "working" || (s.active_sub_tasks ?? 0) > 0,
		unseen: session.unseen ?? s.agent_state === "completed",
		idle: s.shell_state === "idle" || s.agent_state === "idle",
	});
	if (visual === "busy" && (s.active_sub_tasks ?? 0) > 0 && s.shell_state !== "busy" && s.agent_state !== "working")
		return "sub-tasks";
	return visual === "default" ? "idle" : visual;
}
