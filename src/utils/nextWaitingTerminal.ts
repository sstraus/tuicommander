/**
 * Pick the next terminal to jump to from an ordered list of waiting terminal IDs.
 *
 * Cycles by tab order: if the currently active terminal is itself in the list,
 * advance to the one after it (wrapping at the end); otherwise start at the
 * first. Returns null when nothing is waiting.
 */
export function nextWaitingTerminal(waitingIds: string[], activeId: string | null): string | null {
	if (waitingIds.length === 0) return null;
	const idx = activeId ? waitingIds.indexOf(activeId) : -1;
	return waitingIds[(idx + 1) % waitingIds.length];
}
