import { AGENT_TYPES, type AgentType } from "../../agents";
import type { SessionInfo } from "../useSessions";

/**
 * An agent type this build still knows about.
 *
 * A build that drops an agent leaves its name behind in persisted state, and the
 * `Record<AgentType, …>` lookups are indexed without an existence check — so an
 * unrecognised name is treated as no agent at all rather than trusted.
 */
export function isKnownAgentType(value: string | null | undefined): value is AgentType {
	return value != null && (AGENT_TYPES as readonly string[]).includes(value);
}

/**
 * A plain PTY: a shell the user opened, not an AI agent.
 *
 * The absence of a recognised `agent_type` is the whole test. A session whose
 * agent has not identified itself yet reads as a PTY until it does, which is
 * correct — that is exactly what it is on screen at that moment.
 */
export function isPtySession(session: SessionInfo): boolean {
	return !isKnownAgentType(session.state?.agent_type);
}

/**
 * Waiting sessions first, then idle agents, then plain PTYs. Preserve insertion
 * order inside each group.
 *
 * The list is a triage surface: sessions awaiting input need attention before
 * idle agents or shells. Shells can outnumber agents on a busy machine, so
 * leaving them interleaved pushes agent sessions off the first screen.
 *
 * Returns a new array — never sorts in place. `useSessions.reconcileSessions`
 * hands back the *previous* array reference when an idle poll changed nothing,
 * and sorting that in place would mutate the state Solid is tracking.
 */
export function ptysLast(sessions: SessionInfo[]): SessionInfo[] {
	// Stable per spec, so equal keys keep the backend's ordering.
	const priority = (session: SessionInfo) => (session.state?.awaiting_input ? 0 : isPtySession(session) ? 2 : 1);
	return sessions.toSorted((a, b) => priority(a) - priority(b));
}
