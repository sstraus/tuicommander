/** Room kept for the rotating slot when tickers do not all fit: its minimum text width plus the cycle badge. */
export const ROTATING_SLOT_RESERVE_PX = 110;

/**
 * How many tickers (in pool order) are shown side by side. The rest rotate in one
 * last slot. Returns widths.length when everything fits, so no slot or badge is needed.
 * An unmeasured container (available <= 0) pins nothing: every ticker rotates.
 */
export function countSideBySide(widths: readonly number[], available: number, gap: number): number {
	if (available <= 0 || widths.length === 0) return 0;

	let used = 0;
	for (let i = 0; i < widths.length; i++) used += widths[i] + (i > 0 ? gap : 0);
	if (used <= available) return widths.length;

	let pinned = 0;
	used = ROTATING_SLOT_RESERVE_PX;
	for (const width of widths) {
		used += width + gap;
		if (used > available) break;
		pinned++;
	}
	// The loop can run through every ticker only when they all fit, which was handled above.
	return Math.min(pinned, widths.length - 1);
}
