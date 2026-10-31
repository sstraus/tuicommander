import type { DecodedRow, StyledRange } from "./canvasTerminalUtils";
import { cellText } from "./canvasTerminalUtils";
import {
	ANSWER_MARKER_RE,
	answerExtent,
	answerMarkerOffset,
	PROMPT_ROW_RE,
	promptEnd,
	type RowSnapshot,
} from "./suggestOverlay";

/** One turn of the answers-only view: what the user asked, then only the 💬 answers. */
export interface AnswersTurn {
	prompt: string | null;
	answers: string[];
}

/** Rows fetched per `terminal_styled_rows` call. */
export const TURN_FETCH_CHUNK = 256;
/** Rows read back when no user prompt is known, so an unbounded scrollback is never swept. */
export const TURN_MAX_ROWS = 5000;

// Extended pictographs plus the East Asian wide blocks (Hangul Jamo, CJK, Hangul syllables,
// compatibility ideographs and forms, fullwidth forms, supplementary ideographs).
const WIDE_CHAR =
	/\p{Extended_Pictographic}|[\u1100-\u115F\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6]|[\u{20000}-\u{3FFFD}]/u;

/**
 * Text of a row for copying. The wire encodes the spacer cell after a wide
 * character as codepoint 0, which `rowText` renders as a space; here it is dropped
 * so "💬 x" does not become "💬  x".
 */
export function rowCopyText(row: DecodedRow): string {
	let text = "";
	for (let col = 0; col < row.count; col++) {
		const contents = cellText(row, col);
		if (contents === "" && col > 0 && row.codepoints[col - 1] !== 0 && WIDE_CHAR.test(cellText(row, col - 1))) continue;
		text += contents === "" ? " " : contents;
	}
	return text;
}

/** Rows shown in a prompt preview; classification still skips the complete prompt echo. */
export const PROMPT_MAX_ROWS = 50;

/** Text of rows `[from, to]`: soft-wrapped rows concatenated, hard lines joined with a newline, indent under the bullet removed. */
function joinAnswerRows(
	rows: readonly RowSnapshot[],
	from: number,
	to: number,
	markers: ReadonlySet<number>,
	firstText = rows[from].text,
): string {
	let text = firstText.trimStart().replace(/^[●⏺•]\s*/, "");
	for (let i = from + 1; i <= to; i++) {
		const line = rows[i].text;
		text += rows[i].isWrapped
			? line
			: `\n${(markers.has(i) ? line.trimStart().replace(/^[●⏺•]\s*/, "") : line.replace(/^ {1,2}/, "")).trimEnd()}`;
	}
	return text.replace(/[ \t]+$/gm, "").trimEnd();
}

/** Join the soft-wrapped rows starting at `from` into one logical line; returns it and the next row. */
function joinLogicalLine(rows: readonly RowSnapshot[], from: number): { text: string; next: number } {
	let text = rows[from].text;
	let next = from + 1;
	while (next < rows.length && rows[next].isWrapped) {
		text += rows[next].text;
		next++;
	}
	return { text: text.trimEnd(), next };
}

/**
 * The prompt block at the top of a turn: the prompt row plus every following row
 * up to the first agent output (bullet), unindented 💬 row or blank-separated 💬 row. A multi-line or hard-wrapped
 * prompt spans several logical lines, so reading one logical line cut it short
 * (#1369). Soft-wrapped rows are concatenated, hard lines joined with a newline.
 */
function readPrompt(rows: readonly RowSnapshot[]): { text: string; next: number } {
	let text = rows[0].text;
	const next = promptEnd(0, rows.length, (i) => rows[i] ?? null);
	for (let i = 1; i < Math.min(next, PROMPT_MAX_ROWS); i++) {
		const row = rows[i];
		text = row.isWrapped ? text + row.text : `${text.replace(/[ \t]+$/, "")}\n${row.text}`;
	}
	return { text: text.trimEnd(), next };
}

/**
 * Build one turn of the answers-only view from its rows. With `hasPrompt` the
 * first row starts the user's prompt (see `readPrompt`); every logical line after
 * it that starts with the 💬 marker begins an answer region, in order, merging adjacent marked paragraphs and spanning the same rows the
 * terminal highlights (`answerExtent`).
 */
export function buildAnswersTurn(rows: readonly RowSnapshot[], hasPrompt: boolean): AnswersTurn {
	let i = 0;
	let prompt: string | null = null;
	if (hasPrompt && rows.length > 0) {
		const block = readPrompt(rows);
		prompt = block.text;
		i = block.next;
	}
	const answers: string[] = [];
	while (i < rows.length) {
		if (!rows[i].isWrapped && PROMPT_ROW_RE.test(rows[i].text)) {
			i = promptEnd(i, rows.length, (r) => rows[r] ?? null);
			continue;
		}
		const logical = joinLogicalLine(rows, i);
		let marker = rows[i].isWrapped ? undefined : answerMarkerOffset(logical.text);
		let firstText = rows[i].text;
		if (marker !== undefined && !ANSWER_MARKER_RE.test(firstText)) {
			// Locate the recovered marker without changing stored row coordinates.
			while (i + 1 < logical.next && marker >= rows[i].text.length) {
				marker -= rows[i].text.length;
				i++;
			}
			firstText = rows[i].text.slice(marker);
		}
		if (marker !== undefined) {
			const markers = new Set<number>();
			const last = answerExtent(
				i,
				rows.length,
				(r) => (r === i ? { text: firstText, isWrapped: false } : (rows[r] ?? null)),
				(marker) => markers.add(marker),
			);
			answers.push(joinAnswerRows(rows, i, last, markers, firstText));
			i = last + 1;
		} else {
			i = joinLogicalLine(rows, i).next;
		}
	}
	return { prompt, answers };
}

/**
 * Read the rows `[startAbs, endAbs)` (all-time row indexes) through `fetchRange`,
 * which is the same `terminal_styled_rows` reader the scroll row cache uses.
 * Rows the backend no longer holds come back as blank so the result stays aligned;
 * a failed chunk aborts the read (null) rather than yielding a partial turn.
 */
export async function readTurnRows(
	fetchRange: (start: number, count: number) => Promise<StyledRange | null>,
	startAbs: number,
	endAbs: number,
): Promise<RowSnapshot[] | null> {
	const byAbs = new Map<number, { text: string; wrapped: boolean }>();
	for (let start = startAbs; start < endAbs; start += TURN_FETCH_CHUNK) {
		const range = await fetchRange(start, Math.min(TURN_FETCH_CHUNK, endAbs - start));
		if (!range) return null;
		for (const { abs, row } of range.rows) byAbs.set(abs, { text: rowCopyText(row), wrapped: row.wrapped });
	}
	const rows: RowSnapshot[] = [];
	// A row's wire flag says it continues onto the NEXT row; a snapshot says it continues the previous one.
	// DEFERRED (2026-10-04) — a buffer that starts mid-wrapped-answer (eviction cut the head off) has no
	// row startAbs-1 to read, so its first row looks like a head and can show a ghost answer in the
	// answers-only view. Cosmetic and rare; avoiding it needs a look-back mechanism (Boss: not worth it).
	for (let abs = startAbs; abs < endAbs; abs++)
		rows.push({ text: byAbs.get(abs)?.text ?? "", isWrapped: byAbs.get(abs - 1)?.wrapped ?? false });
	return rows;
}

/**
 * All-time row indexes of the user prompts still in the scrollback, ascending and
 * unique. Stored `promptLines` already use all-time coordinates.
 */
export function promptStarts(promptLines: readonly number[], historyBase: number, endAbs: number): number[] {
	const starts = new Set<number>();
	for (const line of promptLines) if (line >= historyBase && line < endAbs) starts.add(line);
	return [...starts].sort((a, b) => a - b);
}

/** Finished turns by start row, valid for one `historyBase` (eviction changes the retained prefix). */
export interface TurnCache {
	base: number;
	turns: Map<number, { endAbs: number; hasPrompt: boolean; turn: AnswersTurn }>;
}

export const newTurnCache = (): TurnCache => ({ base: -1, turns: new Map() });

type RangeReader = (start: number, count: number) => Promise<StyledRange | null>;

/**
 * The whole session as turns: one per user prompt still in the scrollback, each
 * with its full prompt and its 💬 answers; the last one is the running turn. With
 * no known prompt the last `TURN_MAX_ROWS` rows form a single prompt-less turn.
 * Retained output before the first known prompt is also a prompt-less turn,
 * from the retained history base: prompt tracking may start after the agent did.
 * Finished turns come from `cache` (same object each time, so the view keeps their
 * DOM); a failed read aborts the whole build (null).
 */
export async function readAnswersHistory(
	fetchRange: RangeReader,
	promptLines: readonly number[],
	historyBase: number,
	endAbs: number,
	cache: TurnCache,
): Promise<AnswersTurn[] | null> {
	const starts = promptStarts(promptLines, historyBase, endAbs);
	if (starts.length === 0) {
		const rows = await readTurnRows(fetchRange, Math.max(historyBase, endAbs - TURN_MAX_ROWS), endAbs);
		return rows && [buildAnswersTurn(rows, false)];
	}
	const firstPrompt = starts[0];
	const prefixStart = historyBase;
	if (prefixStart < firstPrompt) starts.unshift(prefixStart);
	if (cache.base !== historyBase) {
		cache.base = historyBase;
		cache.turns.clear();
	}
	const turns: AnswersTurn[] = [];
	for (let i = 0; i < starts.length; i++) {
		const start = starts[i];
		const end = i + 1 < starts.length ? starts[i + 1] : endAbs;
		const finished = i + 1 < starts.length;
		const hasPrompt = start >= firstPrompt;
		const hit = cache.turns.get(start);
		if (finished && hit?.endAbs === end && hit.hasPrompt === hasPrompt) {
			turns.push(hit.turn);
			continue;
		}
		const rows = await readTurnRows(fetchRange, start, end);
		if (!rows) return null;
		const turn = buildAnswersTurn(rows, hasPrompt);
		if (finished) cache.turns.set(start, { endAbs: end, hasPrompt, turn });
		turns.push(turn);
	}
	return turns;
}

/** Structural equality, so an unchanged refresh keeps the panel DOM (and the user's selection). */
export function sameAnswersHistory(a: readonly AnswersTurn[] | null, b: readonly AnswersTurn[] | null): boolean {
	if (a === b) return true;
	if (!a || !b || a.length !== b.length) return false;
	return a.every(
		(t, i) =>
			t.prompt === b[i].prompt &&
			t.answers.length === b[i].answers.length &&
			t.answers.every((x, j) => x === b[i].answers[j]),
	);
}
