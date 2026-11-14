/**
 * Built-in ticker: how many agent sessions are working and how many are idle.
 * Counts come from the per-session agent state TUIC already tracks.
 */

import { createEffect, createRoot } from "solid-js";
import { activityDashboardStore } from "../stores/activityDashboard";
import { statusBarTicker } from "../stores/statusBarTicker";
import { type AgentLifecycleState, terminalsStore } from "../stores/terminals";

const FEATURE_ID = "agent-activity";
const TICKER_ID = "counts";
/** Normal tier: rotates or sits beside the others, never pins. */
const TICKER_PRIORITY = 20;

export interface AgentCounts {
	working: number;
	idle: number;
}

/** Agent states that count as working; "awaiting_input" waits for the user and is neither. */
function classify(state: AgentLifecycleState): keyof AgentCounts | null {
	if (state === "working" || state === "starting") return "working";
	if (state === "idle" || state === "completed") return "idle";
	return null;
}

export function countAgentSessions(
	sessions: ReadonlyArray<{ agentType: unknown; agentState: AgentLifecycleState }>,
): AgentCounts {
	const counts: AgentCounts = { working: 0, idle: 0 };
	for (const session of sessions) {
		if (!session.agentType) continue;
		const bucket = classify(session.agentState);
		if (bucket) counts[bucket]++;
	}
	return counts;
}

export function formatAgentCounts({ working, idle }: AgentCounts): string {
	return `${working} working · ${idle} idle`;
}

let dispose: (() => void) | null = null;

export function initAgentActivity(): void {
	if (dispose) return;
	dispose = createRoot((disposeRoot) => {
		createEffect(() => {
			const sessions = terminalsStore.getIds().flatMap((id) => {
				const t = terminalsStore.get(id);
				return t ? [t] : [];
			});
			const counts = countAgentSessions(sessions);
			if (counts.working + counts.idle === 0) {
				statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
				return;
			}
			statusBarTicker.addMessage({
				id: TICKER_ID,
				pluginId: FEATURE_ID,
				label: "Agents",
				text: formatAgentCounts(counts),
				priority: TICKER_PRIORITY,
				ttlMs: 0,
				onClick: activityDashboardStore.open,
			});
		});
		return disposeRoot;
	});
}

export function destroyAgentActivity(): void {
	dispose?.();
	dispose = null;
	statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
}
