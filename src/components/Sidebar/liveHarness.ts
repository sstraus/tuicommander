import { terminalsStore } from "../../stores/terminals";

/**
 * A session qualifies for the H view while a harness (agent) is the foreground
 * process of a terminal that has not exited. Working, idle and awaiting-input
 * all qualify; a plain shell (no `agentType`) and an ended harness do not.
 * Reads the same terminal state that drives the working/idle/awaiting badges.
 */
export function isLiveHarness(terminalId: string): boolean {
	const term = terminalsStore.get(terminalId);
	return !!term?.agentType && term.shellState !== "exited";
}

export const hasLiveHarness = (terminalIds: readonly string[]): boolean => terminalIds.some(isLiveHarness);
