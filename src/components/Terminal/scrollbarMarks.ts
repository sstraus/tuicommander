/**
 * The scrollbar overlay for one terminal, as a pure string.
 *
 * Extracted from `CanvasTerminal`'s `paintScrollbarMarks` closure so the one
 * decision embedded in it is testable: **`showBlocks` gates the history markers
 * only.** Block boundaries and user-prompt ticks are a display preference, so
 * `showScrollbarMarks` may hide them. Search hits are not — they are the live
 * result of a Cmd+F the user just pressed, and a search that silently draws
 * nothing because of an unrelated terminal display setting reads as a broken
 * search, not as a preference being honoured.
 *
 * Kept free of the DOM and of the stores on purpose: the caller owns the
 * container, the repaint key and the settings read, and this owns only the
 * geometry. That split is what lets a test state the rule above directly instead
 * of mounting a canvas terminal to infer it.
 */

/** Colour of a block tick whose command failed. */
const FAILED_BLOCK = "#f85149";
/** Colour of a block tick whose command succeeded or is still running. */
const OK_BLOCK = "rgba(88,166,255,0.5)";
/** Colour of the tick marking a line where the user submitted a prompt. */
const USER_PROMPT = "#3fb950";
/** Colour of a search-match tick. */
const SEARCH_MATCH = "#e8984c";

export interface ScrollbarMarkBlock {
	promptLine: number;
	exitCode: number | null;
}

export interface ScrollbarMarksInput {
	/** Command blocks, drawn only when `showBlocks` is true. */
	blocks: readonly ScrollbarMarkBlock[];
	/** Rows where the user submitted a prompt; drawn only when `showBlocks` is true. */
	promptLines: readonly number[];
	/** Rows of the current search hits. Drawn regardless of `showBlocks`. */
	matchRows: readonly number[];
	/** Total rows in the scrollback, i.e. the denominator for every ratio. */
	totalRows: number;
	/** Evicted rows; block and prompt rows are all-time, search rows are grid-relative. */
	historyBase?: number;
	/** Pixel height of the scrollbar track. */
	trackH: number;
	/**
	 * Whether to draw the history markers, controlled by `showScrollbarMarks`.
	 * Independent of the Ctrl+Cmd timestamp overlay.
	 */
	showBlocks: boolean;
}

const tick = (top: number, color: string) =>
	`<div style="position:absolute;right:0;width:100%;height:2px;top:${top}px;background:${color}"></div>`;

export function buildScrollbarMarksHtml(input: ScrollbarMarksInput): string {
	const { blocks, promptLines, matchRows, totalRows, trackH, showBlocks, historyBase = 0 } = input;
	let html = "";
	if (showBlocks) {
		for (const block of blocks) {
			const color = block.exitCode !== null && block.exitCode !== 0 ? FAILED_BLOCK : OK_BLOCK;
			const row = block.promptLine - historyBase;
			if (row >= 0 && row < totalRows) html += tick((row / totalRows) * trackH, color);
		}
		// Drawn after the block ticks so a user prompt sits on top of the block
		// boundary it shares a row with. There are few of these — one per turn.
		for (const line of promptLines) {
			const row = line - historyBase;
			if (row >= 0 && row < totalRows) html += tick((row / totalRows) * trackH, USER_PROMPT);
		}
	}
	// Rounded to whole pixels and de-duplicated: a search with hundreds of hits in
	// a long scrollback maps many rows onto one pixel, and emitting a div per hit
	// costs the DOM without changing what is drawn.
	const seen = new Set<number>();
	for (const row of matchRows) {
		const rounded = Math.round((row / totalRows) * trackH);
		if (seen.has(rounded)) continue;
		seen.add(rounded);
		html += tick(rounded, SEARCH_MATCH);
	}
	return html;
}
