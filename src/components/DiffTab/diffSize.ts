/**
 * Large-diff guard for DiffTab. @git-diff-view has no virtualization, so above
 * this many lines we render an opt-in placeholder instead of thousands of DOM
 * rows. Pure helpers so the threshold logic is unit-testable without the view.
 */

/** Line count above which a diff is considered too large to render eagerly. */
export const LARGE_DIFF_LINES = 3000;

/** Number of lines in a unified-diff string (0 for empty/falsy input). */
export function diffLineCount(diff: string): number {
	return diff ? diff.split("\n").length : 0;
}

/** Whether a diff exceeds the eager-render threshold. */
export function isDiffTooLarge(diff: string): boolean {
	return diffLineCount(diff) > LARGE_DIFF_LINES;
}
