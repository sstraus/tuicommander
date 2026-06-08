/**
 * Compute the next absolute display offset for a coalesced scroll step.
 *
 * Offset semantics match the backend grid: 0 = bottom, `historySize` = top.
 * A wheel/touch gesture of `lines` (positive = scroll up into history) moves
 * the offset by `+lines`. The old per-step path sent `terminal_scroll` with
 * `delta = -lines`, so the equivalent absolute target is `base - lines`.
 * Result is clamped to [0, historySize].
 */
export function nextScrollOffset(base: number, lines: number, historySize: number): number {
	return Math.max(0, Math.min(historySize, base - lines));
}
