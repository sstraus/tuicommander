import intentMarker from "../../../src-tauri/crates/tuic-terminal/src/intent_marker.json";

/** A snapshot of an xterm buffer row — just the parts the overlay cares about. */
export interface RowSnapshot {
	text: string;
	isWrapped: boolean;
	/** Map a text boundary to its real terminal cell (wide/combining glyphs). */
	endColumn?: (offset: number) => number;
}

/** Re-declared here instead of imported: keeps the helper self-contained and
 *  avoids pulling the full Terminal module into unit tests. Must stay in
 *  sync with the patterns used in Terminal.tsx. */
/** Agent bullet glyphs: Claude's `●`/`⏺`, Codex's `•`. A new agent's bullet is added here, once. */
export const AGENT_BULLETS = "●⏺•";
export const SUGGEST_ANCHOR_RE = new RegExp(`^[\\s${AGENT_BULLETS}]*suggest:\\s+\\S`);
/** Stop condition for the continuation walk: a new `intent:` token at column 0. */
const INTENT_RE = /^intent:\s+\S/;
/**
 * Which rows get the intent highlight. The shared Rust/JS anchor grammar also
 * accepts an agent bullet, horizontal indent or the TUIC ack sentence. It is NOT
 * a walk stop condition — ending a suggest
 * block on an indented mention would swallow the block's closing bracket.
 */
export const INTENT_HIGHLIGHT_RE = new RegExp(`${intentMarker.anchor}[\\t ]+`);
/** Same title boundary as Rust capture; rendering never invents a second delimiter. */
const INTENT_TITLE_RE = new RegExp(intentMarker.title);
const INTENT_ANSWER_RE = new RegExp(intentMarker.answer);
/** Recover a missing newline only after a closed intent title, never at a
 * literal bubble in its description. Shared by terminal and Chat projection. */
export function splitIntentAnswer(body: string): { intent: string; answerStart?: number } {
	const answer = INTENT_ANSWER_RE.exec(body);
	if (!answer) return { intent: body };
	const answerStart = answer.index + answer[0].length - answer[1].length;
	const intent = body.slice(0, answerStart).trimEnd();
	return INTENT_TITLE_RE.test(intent) ? { intent, answerStart } : { intent: body };
}
/** Boundary excludes answer text before choosing the final parenthesized title. */
export function intentTitleEnd(body: string): number | undefined {
	const title = INTENT_TITLE_RE.exec(splitIntentAnswer(body).intent);
	return title ? title[1].length + title[2].length + 2 : undefined;
}
/**
 * The answer marker an agent prefixes to every sentence that directly answers
 * the user (💬). Matched on row text, which carries no escape sequences — bold,
 * colour and the like live in cell attributes — so styling cannot hide it.
 * Like `intent:`, it may sit behind the agent's own bullet: an agent bullet (see [`AGENT_BULLETS`]).
 */
export const ANSWER_MARKER_RE = new RegExp(`^[\\s${AGENT_BULLETS}]*(?:[-*+]\\s+|\\d+[.)]\\s+)?💬`);
/** UTF-16 offset of a live answer marker on one logical line. */
export function answerMarkerOffset(text: string): number | undefined {
	if (ANSWER_MARKER_RE.test(text)) return text.indexOf("💬");
	const anchor = INTENT_HIGHLIGHT_RE.exec(text);
	if (!anchor) return undefined;
	const offset = splitIntentAnswer(text.slice(anchor[0].length)).answerStart;
	return offset === undefined ? undefined : anchor[0].length + offset;
}
/** The indentation of a marked list item bounds its answer, unlike a paragraph. */
const ANSWER_LIST_RE = new RegExp(`^([\\t ${AGENT_BULLETS}]*)(?:[-*+]\\s+|\\d+[.)]\\s+)💬`);
/** An agent output row: a bullet glyph at the start of the line. Codex's `•` also
 *  marks ordinary list items, so it counts only at column 0; an indented one is content. */
export const OUTPUT_ROW_RE = /^(?:\s*[●⏺]|•)/;
/** A continuation row of an answer: indented under the agent's bullet. */
const ANSWER_CONTINUATION_RE = /^ {2,}\S/;
/** A user prompt row; an indented one still ends the answer. */
export const PROMPT_ROW_RE = /^\s*[❯›]/;
/** A fenced code block delimiter. Inside a fence a bullet, marker or prompt glyph is content. */
const FENCE_RE = /^\s*```/;
/** A list item, table row or quotation belongs to the answer above it. */
const ANSWER_STRUCTURE_RE = /^ {2,}(?:[-*+•] |\d+[.)] |\||>)/;
/** Codex's indented turn-duration footer is terminal status, not answer prose. */
const CODEX_FOOTER_RE =
	/^\s*Worked for \d+(?:\.\d+)?(?:ms|[smh])(?:\s+\d+(?:\.\d+)?(?:ms|[smh]))*(?:\s*•\s*\d{1,2}:\d{2})?\s*$/;

/** A new output, prompt, status or protocol row cannot belong to an answer. */
function isAnswerBoundary(text: string): boolean {
	return (
		OUTPUT_ROW_RE.test(text) ||
		PROMPT_ROW_RE.test(text) ||
		CODEX_FOOTER_RE.test(text) ||
		SUGGEST_ANCHOR_RE.test(text) ||
		INTENT_HIGHLIGHT_RE.test(text)
	);
}

/**
 * First output row after a user prompt. Claude echoes pasted hard lines with
 * indentation, so indentation alone cannot identify output. An agent bullet,
 * a column-zero marker, or a blank-separated marker starts output. A marker
 * embedded in uninterrupted pasted prose stays in the prompt. Soft wraps never
 * start a new block.
 */
export function promptEnd(start: number, totalRows: number, getRow: (i: number) => RowSnapshot | null): number {
	let next = start + 1;
	for (; next < totalRows; next++) {
		const row = getRow(next);
		if (!row) break;
		if (
			!row.isWrapped &&
			(OUTPUT_ROW_RE.test(row.text) ||
				(ANSWER_MARKER_RE.test(row.text) && (/^\S/.test(row.text) || getRow(next - 1)?.text.trim() === "")))
		)
			break;
	}
	return next;
}

/**
 * Last row of the answer that starts at `start` (a row matching
 * [`ANSWER_MARKER_RE`]). An answer is the marker row, every row it wraps onto and
 * the indented rows that follow. Blank gaps join marked paragraphs and structured
 * content, but an unmarked prose paragraph after a blank row ends the region. It ends before the
 * next non-answer bullet (a tool call), before a `suggest:`/`intent:` row, before a row that starts at column
 * 0 (user prompt, separator, status line) and before the trailing blank rows.
 * Adjacent answer markers extend the same region, including blank rows between them.
 * Rows inside a fenced code block belong to the answer whatever glyph they start with.
 */
export function answerExtent(
	start: number,
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
	onMarker?: (row: number) => void,
): number {
	onMarker?.(start);
	let last = start;
	let inFence = false;
	let afterBlank = false;
	const listIndent = ANSWER_LIST_RE.exec(getRow(start)?.text ?? "")?.[1].length;
	for (let i = start + 1; i < totalRows; i++) {
		const row = getRow(i);
		if (!row) break;
		if (row.isWrapped) {
			last = i;
			continue;
		}
		if (row.text.trim() === "") {
			afterBlank = true;
			continue;
		}
		// A sibling item or dedented prose is outside this list answer, even
		// when another marked sibling starts a separate answer immediately.
		if (listIndent !== undefined && (row.text.match(/^[\t ]*/)?.[0].length ?? 0) <= listIndent) break;
		if (!inFence && ANSWER_MARKER_RE.test(row.text)) {
			// A marked list item owns its own scope, even after a paragraph answer.
			if (listIndent === undefined && ANSWER_LIST_RE.test(row.text)) break;
			onMarker?.(i);
			last = i;
			afterBlank = false;
			continue;
		}
		if (listIndent === undefined && !ANSWER_CONTINUATION_RE.test(row.text)) break;
		if (
			listIndent === undefined &&
			afterBlank &&
			!inFence &&
			!ANSWER_STRUCTURE_RE.test(row.text) &&
			!FENCE_RE.test(row.text)
		)
			break;
		afterBlank = false;
		if (inFence) {
			inFence = !FENCE_RE.test(row.text);
		} else {
			if (isAnswerBoundary(row.text)) break;
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
const SUGGEST_STOP_RE = new RegExp(`^[\\t ]*(?:[${AGENT_BULLETS}][\\t ]+)?suggest:\\s+\\S`);

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

/** One row the plan classifies, and why. */
export interface OverlayBlock {
	row: number;
	kind: "suggest" | "continuation" | "intent" | "answer";
	/** Present only when ordinary prose shares the final intent row. */
	endColumn?: number;
	/** An answer can begin after an intent on the same physical row. */
	startColumn?: number;
}

/**
 * Which rows hold a `suggest:`/`intent:` marker or an answer on this screen, plus
 * a key that changes exactly when that set does. The marker rows are not painted
 * at all (see [`hiddenMarkerRows`]); only answer rows get an overlay strip.
 *
 * `firstRow` lets the caller start the scan above the viewport (negative row
 * indexes). A marker whose first row has scrolled off the top still has its
 * continuation rows on screen, and they are only recognisable from the anchor.
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
	firstRow = 0,
): { key: string; blocks: OverlayBlock[]; answerMarkers: ReadonlySet<number> } {
	const answerMarkers = new Set<number>();
	const blocks: OverlayBlock[] = [];
	const parts: string[] = [];
	let lastAnswerEnd: number | undefined;
	let inFence = false;
	for (let row = firstRow; row < totalRows; row++) {
		const snapshot = getRow(row);
		if (!snapshot) continue;
		const text = snapshot.text;

		// answerExtent owns fences inside marked answers. Standalone examples
		// reach this scan instead, and their protocol-looking text is literal too.
		if (!snapshot.isWrapped && FENCE_RE.test(text)) {
			inFence = !inFence;
			lastAnswerEnd = undefined;
			continue;
		}
		if (inFence) continue;

		if (!snapshot.isWrapped && PROMPT_ROW_RE.test(text)) {
			lastAnswerEnd = undefined;
			row = promptEnd(row, totalRows, getRow) - 1;
			continue;
		}

		if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, totalRows, getRow)) {
			blocks.push({ row, kind: "suggest" });
			parts.push(`s${row}`);
			const hiddenRows = continuationRowsAfterSuggest(row, totalRows, getRow);
			for (const contRow of hiddenRows) {
				blocks.push({ row: contRow, kind: "continuation" });
				parts.push(`c${contRow}`);
			}
			const end = hiddenRows.at(-1) ?? row;
			// Removing a standalone protocol paragraph must not leave both of
			// its separators adjacent. Keep the leading separator and omit one
			// trailing blank only when both were recorded around this block.
			if (getRow(row - 1)?.text.trim() === "" && getRow(end + 1)?.text.trim() === "") {
				blocks.push({ row: end + 1, kind: "continuation" });
				parts.push(`c${end + 1}`);
				row = end + 1;
			} else {
				row = end;
			}
		} else if (!snapshot.isWrapped && ANSWER_MARKER_RE.test(text)) {
			// The whole answer, not only its marker row. A wrapped row is never a
			// line start, so an emoji the wrap happens to land on is not a marker.
			const last = answerExtent(row, totalRows, getRow, (marker) => answerMarkers.add(marker));
			// Answer extraction keeps separate list scopes and excludes unmarked
			// paragraphs. The gutter instead joins confirmed answer sections across
			// their blank separators and indented headings. A later answer must
			// close the gap: trailing blanks/prose never inherit the gutter.
			if (lastAnswerEnd !== undefined) {
				let gap = lastAnswerEnd + 1;
				for (; gap < row; gap++) {
					const between = getRow(gap);
					if (!between) break;
					if (between.text.trim() === "") continue;
					if (
						!between.isWrapped &&
						(!ANSWER_CONTINUATION_RE.test(between.text) ||
							isAnswerBoundary(between.text) ||
							ANSWER_STRUCTURE_RE.test(between.text))
					)
						break;
				}
				if (gap === row) {
					for (gap = lastAnswerEnd + 1; gap < row; gap++) {
						blocks.push({ row: gap, kind: "answer" });
						parts.push(`a${gap}`);
					}
				}
			}
			lastAnswerEnd = last;
			for (; row <= last; row++) {
				blocks.push({ row, kind: "answer" });
				parts.push(`a${row}`);
			}
			row = last;
		} else if (INTENT_HIGHLIGHT_RE.test(text)) {
			// Choose the title over a whole soft-wrapped logical line, never a
			// physical fragment that happens to close an earlier aside.
			let body = "";
			let prefix = INTENT_HIGHLIGHT_RE.exec(text)![0].length;
			const extent: { row: number; snapshot: RowSnapshot; start: number; prefix: number }[] = [];
			let boundary: number | undefined;
			for (;;) {
				const current = getRow(row)!;
				extent.push({ row, snapshot: current, start: body.length, prefix });
				body += current.text.slice(prefix);
				prefix = 0;
				const next = row + 1 < totalRows ? getRow(row + 1) : null;
				if (next?.isWrapped) {
					row++;
					continue;
				}
				boundary = intentTitleEnd(body);
				if (boundary !== undefined || !next || next.text.trim() === "") break;
				// Ink also hard-wraps with cursor moves and indentation.
				if (
					!ANSWER_CONTINUATION_RE.test(next.text) ||
					FENCE_RE.test(next.text) ||
					isAnswerBoundary(next.text) ||
					ANSWER_MARKER_RE.test(next.text)
				)
					break;
				body += " ";
				row++;
			}
			for (const part of extent) {
				if (boundary !== undefined && part.start >= boundary) break;
				const end = boundary === undefined ? undefined : boundary - part.start + part.prefix;
				const partial = end !== undefined && part.snapshot.text.slice(end).trim() !== "";
				const endColumn = partial ? (part.snapshot.endColumn?.(end!) ?? end!) : undefined;
				blocks.push(
					endColumn === undefined ? { row: part.row, kind: "intent" } : { row: part.row, kind: "intent", endColumn },
				);
				parts.push(`i${part.row}${endColumn === undefined ? "" : `:${endColumn}`}`);
			}
			const answerStart = splitIntentAnswer(body).answerStart;
			if (answerStart !== undefined) {
				const first = extent.findLast((part) => part.start <= answerStart)!;
				const offset = answerStart - first.start + first.prefix;
				const startColumn = first.snapshot.endColumn?.(offset) ?? offset;
				const last = answerExtent(first.row, totalRows, getRow, (marker) => answerMarkers.add(marker));
				for (let answerRow = first.row; answerRow <= last; answerRow++) {
					blocks.push(
						answerRow === first.row
							? { row: answerRow, kind: "answer", startColumn }
							: { row: answerRow, kind: "answer" },
					);
					parts.push(`a${answerRow}${answerRow === first.row ? `:${startColumn}` : ""}`);
				}
				lastAnswerEnd = last;
				row = last;
			}
		}
	}
	return { key: parts.join(","), blocks, answerMarkers };
}

/** Suggest anchors and wrapped tails omitted by the frontend viewport projection.
 * Intent remains visible. Stored backend row and cell coordinates never change.
 */
export function hiddenMarkerRows(blocks: readonly OverlayBlock[]): Set<number> {
	const hidden = new Set<number>();
	for (const block of blocks) if (block.kind === "suggest" || block.kind === "continuation") hidden.add(block.row);
	return hidden;
}

function overlayDiv(top: number, height: number, background: string): HTMLDivElement {
	const div = document.createElement("div");
	div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
	return div;
}

/** Shared desktop/mobile answer gutter; no background tint or speech-bubble glyph. */
export const ANSWER_RULE_SHADOW = "inset 3px 0 0 var(--answer-rule)";

/**
 * Replace the contents of `container` with one absolutely positioned strip per
 * answer row: a transparent strip with a solid gutter bar. Intent rows get a
 * theme-aware tint; omitted suggest rows get no strip.
 */
export function paintOverlayBlocks(
	container: HTMLElement,
	blocks: readonly OverlayBlock[],
	cellHeight: number,
	cellWidth?: number,
): void {
	container.textContent = "";
	for (const block of blocks) {
		if (block.row < 0) continue;
		if (block.kind === "intent") {
			const div = overlayDiv(block.row * cellHeight, cellHeight, "var(--warning)");
			div.dataset.terminalMarker = "intent";
			if (block.endColumn !== undefined && cellWidth !== undefined) {
				div.style.right = "auto";
				div.style.width = `${block.endColumn * cellWidth}px`;
			}
			div.style.opacity = "0.12";
			container.appendChild(div);
		} else if (block.kind === "answer") {
			const div = overlayDiv(block.row * cellHeight, cellHeight, "transparent");
			div.style.boxShadow = ANSWER_RULE_SHADOW;
			if (block.startColumn !== undefined && cellWidth !== undefined)
				div.style.left = `${block.startColumn * cellWidth}px`;
			container.appendChild(div);
		}
	}
}

/** Cache identity for the answer overlay geometry. */
export function answerOverlayKey(markerKey: string, cellHeight: number, rows: readonly number[]): string {
	return JSON.stringify([markerKey, cellHeight, rows]);
}
