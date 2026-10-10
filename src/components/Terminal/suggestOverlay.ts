/** A snapshot of an xterm buffer row — just the parts the overlay cares about. */
export interface RowSnapshot {
	text: string;
	isWrapped: boolean;
}

/** Re-declared here instead of imported: keeps the helper self-contained and
 *  avoids pulling the full Terminal module into unit tests. Must stay in
 *  sync with the patterns used in Terminal.tsx. */
export const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
/** Stop condition for the continuation walk: a new `intent:` token at column 0. */
const INTENT_RE = /^intent:\s+\S/;
/**
 * Which rows get the intent highlight. Deliberately looser than [`INTENT_RE`]:
 * an agent renders its own intent line behind a bullet (`⏺ intent: …`), and that
 * row must still be tinted. It is NOT a walk stop condition — ending a suggest
 * block on an indented mention would swallow the block's closing bracket.
 */
export const INTENT_HIGHLIGHT_RE = /^[\s●⏺]*intent:\s+/;
/**
 * The answer marker an agent prefixes to every sentence that directly answers
 * the user (💬). Matched on row text, which carries no escape sequences — bold,
 * colour and the like live in cell attributes — so styling cannot hide it.
 * Like `intent:`, it may sit behind the agent's own bullet.
 */
export const ANSWER_MARKER_RE = /^[\s●⏺]*💬/;
/** An agent output row: a bullet glyph at the start of the line. */
const OUTPUT_ROW_RE = /^\s*[●⏺]/;
/** A continuation row of an answer: indented under the agent's bullet. */
const ANSWER_CONTINUATION_RE = /^ {2,}\S/;
/** A user prompt row; an indented one still ends the answer. */
const PROMPT_ROW_RE = /^\s*❯/;
/** A fenced code block delimiter. Inside a fence a bullet, marker or prompt glyph is content. */
const FENCE_RE = /^\s*```/;

/**
 * Last row of the answer that starts at `start` (a row matching
 * [`ANSWER_MARKER_RE`]). An answer is the marker row, every row it wraps onto and
 * the indented rows that follow, blank paragraph gaps included. It ends before the
 * next bullet (a tool call or another answer), before a `suggest:`/`intent:` row, before a row that starts at column
 * 0 (user prompt, separator, status line) and before the trailing blank rows.
 * Rows inside a fenced code block belong to the answer whatever glyph they start with.
 */
export function answerExtent(start: number, totalRows: number, getRow: (i: number) => RowSnapshot | null): number {
	let last = start;
	let inFence = false;
	for (let i = start + 1; i < totalRows; i++) {
		const row = getRow(i);
		if (!row) break;
		if (row.isWrapped) {
			last = i;
			continue;
		}
		if (row.text.trim() === "") continue;
		if (!ANSWER_CONTINUATION_RE.test(row.text)) break;
		if (inFence) {
			inFence = !FENCE_RE.test(row.text);
		} else {
			if (OUTPUT_ROW_RE.test(row.text) || ANSWER_MARKER_RE.test(row.text) || PROMPT_ROW_RE.test(row.text)) break;
			if (SUGGEST_ANCHOR_RE.test(row.text) || INTENT_HIGHLIGHT_RE.test(row.text)) break;
			inFence = FENCE_RE.test(row.text);
		}
		last = i;
	}
	return last;
}

/** A rendered top-level block of a chat message, reduced to what the answer rule reads. */
export interface ChatBlock {
	kind: "paragraph" | "code" | "other";
	text: string;
	/** The block opens a 💬 answer. Only a paragraph can. */
	marker: boolean;
}

/**
 * Block ranges `[first, last]` of the answers in a rendered chat message, by the
 * same rule as the grid: the blocks are laid out as the grid rows of an agent
 * reply (indented under the bullet, a blank row between blocks) and `answerExtent`
 * decides where each answer ends.
 */
export function answerBlockRanges(blocks: readonly ChatBlock[]): [number, number][] {
	const rows: RowSnapshot[] = [];
	const owner: number[] = [];
	const push = (text: string, block: number) => {
		rows.push({ text, isWrapped: false });
		owner.push(block);
	};
	const firstRow: number[] = [];
	blocks.forEach((block, index) => {
		if (index > 0) push("", index);
		firstRow.push(rows.length);
		if (block.kind === "code") for (const line of ["```", ...block.text.split("\n"), "```"]) push(`  ${line}`, index);
		else push(block.kind === "paragraph" ? `  ${block.marker ? "💬 " : ""}${block.text}` : `  - ${block.text}`, index);
	});
	const ranges: [number, number][] = [];
	for (let index = 0; index < blocks.length; index++) {
		if (!blocks[index].marker) continue;
		const last = owner[answerExtent(firstRow[index], rows.length, (r) => rows[r] ?? null)];
		ranges.push([index, last]);
		index = last;
	}
	return ranges;
}

/** Match a NEW `suggest:` anchor for stop-detection during a continuation
 *  walk. Does NOT require `|` on the same row — the Rust parser allows the
 *  first `|` to arrive on a wrapped continuation line, so a row like
 *  `suggest: long item that wraps...` (with the pipe on the next row) is
 *  still a new block boundary and the walk MUST stop here. (#1380-3b9c) */
const SUGGEST_STOP_RE = /^[\t ]*(?:[●⏺][\t ]+)?suggest:\s+\S/;

/**
 * Given a suggest anchor row at `anchorIndex`, return the 0-based indexes of
 * subsequent rows that should be visually hidden as continuations of the same
 * `suggest: [ … ]` block.
 *
 * The bracket pair bounds the token: the closing `]` is a hard terminator, so
 * the walk simply hides every row after the anchor up to and including the row
 * that carries `]`. A single-line suggest closes on the anchor itself → nothing
 * extra to hide. A new `suggest:`/`intent:` token before the `]` stops the walk
 * defensively. Because the `]` bounds the block, stray pipe rows (Makefile /
 * mermaid / tables) can never be swallowed.
 */
export function continuationRowsAfterSuggest(
	anchorIndex: number,
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
): number[] {
	const anchor = getRow(anchorIndex);
	// Single-line bracketed suggest closes on the anchor row.
	if (!anchor || anchor.text.includes("]")) return [];
	const hidden: number[] = [];
	for (let i = anchorIndex + 1; i < totalRows; i++) {
		const row = getRow(i);
		if (!row) break;
		// A new token begins a different block — stop before it.
		if (SUGGEST_STOP_RE.test(row.text) || INTENT_RE.test(row.text)) break;
		hidden.push(i);
		// The closing `]` ends the bracketed token — hide it, then stop.
		if (row.text.includes("]")) break;
	}
	return hidden;
}

/**
 * Determine whether the row at `anchorIndex` is the start of a `suggest: [ … ]`
 * block — i.e. one the Rust parser would accept and render as chips.
 *
 * Requires the bracketed form: a `suggest:` anchor at column 0 that opens a `[`
 * and contains a `|` separator. When the terminal is wide enough both land on
 * the anchor row; on narrow terminals the first `|` may wrap onto a continuation
 * row, so wrapped rows are checked too.
 */
export function isSuggestBlock(
	anchorIndex: number,
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
): boolean {
	const row = getRow(anchorIndex);
	if (!row) return false;

	// Must look like a bracketed suggest anchor at column 0.
	if (!SUGGEST_ANCHOR_RE.test(row.text) || !row.text.includes("[")) return false;

	// Fast path: pipe on the same line — classic case.
	if (row.text.includes("|")) return true;

	// Otherwise the first `|` may have wrapped onto a continuation row.
	for (let i = anchorIndex + 1; i < totalRows; i++) {
		const next = getRow(i);
		if (!next?.isWrapped) break;
		if (next.text.includes("|")) return true;
	}

	return false;
}

/** One row the overlay masks, and why. */
export interface OverlayBlock {
	row: number;
	kind: "suggest" | "continuation" | "intent" | "answer";
}

/**
 * Which rows the suggest/intent overlay must mask on this screen, plus a key
 * that changes exactly when that set does.
 *
 * The key is the point: the overlay rebuilds its DOM only when the plan differs
 * from the last one, and most repaints do not move a suggest block. Deciding
 * that from freshly built `<div>`s meant creating and dropping the whole overlay
 * on every frame to discover it was unchanged, so the plan is computed first and
 * the elements are built only once the key says they are needed.
 */
export function planSuggestOverlay(
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
): { key: string; blocks: OverlayBlock[] } {
	const blocks: OverlayBlock[] = [];
	const parts: string[] = [];
	for (let row = 0; row < totalRows; row++) {
		const snapshot = getRow(row);
		if (!snapshot) continue;
		const text = snapshot.text;

		if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, totalRows, getRow)) {
			blocks.push({ row, kind: "suggest" });
			parts.push(`s${row}`);
			const hiddenRows = continuationRowsAfterSuggest(row, totalRows, getRow);
			for (const contRow of hiddenRows) {
				blocks.push({ row: contRow, kind: "continuation" });
				parts.push(`c${contRow}`);
			}
			if (hiddenRows.length > 0) row = hiddenRows.at(-1)!;
		} else if (!snapshot.isWrapped && ANSWER_MARKER_RE.test(text)) {
			// The whole answer, not only its marker row. A wrapped row is never a
			// line start, so an emoji the wrap happens to land on is not a marker.
			const last = answerExtent(row, totalRows, getRow);
			for (; row <= last; row++) {
				blocks.push({ row, kind: "answer" });
				parts.push(`a${row}`);
			}
			row = last;
		} else if (INTENT_HIGHLIGHT_RE.test(text)) {
			blocks.push({ row, kind: "intent" });
			parts.push(`i${row}`);
		}
	}
	return { key: parts.join(","), blocks };
}

function overlayDiv(top: number, height: number, background: string): HTMLDivElement {
	const div = document.createElement("div");
	div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
	return div;
}

/**
 * Replace the contents of `container` with one absolutely positioned strip per
 * planned block. Masks (`suggest`, `continuation`) paint the terminal
 * background over the row; `intent` and `answer` are translucent tints, so the
 * row's text stays readable underneath. An answer also gets a solid gutter bar.
 */
export function paintOverlayBlocks(
	container: HTMLElement,
	blocks: readonly OverlayBlock[],
	cellHeight: number,
	bg: string,
): void {
	container.textContent = "";
	for (const block of blocks) {
		const top = block.row * cellHeight;
		if (block.kind === "answer") {
			const div = overlayDiv(top, cellHeight, "rgba(94,190,140,0.14)");
			div.style.boxShadow = "inset 3px 0 0 rgba(94,190,140,0.9)";
			container.appendChild(div);
		} else {
			container.appendChild(overlayDiv(top, cellHeight, block.kind === "intent" ? "rgba(181,147,90,0.12)" : bg));
		}
	}
}
