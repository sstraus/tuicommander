/**
 * Thin bridge between the shared <SearchBar> UI and the @codemirror/search
 * commands, so the code editor uses the same search component as the terminal
 * instead of CodeMirror's built-in panel. Pure helpers here (query building +
 * match counting) are unit-tested; the imperative view commands live in
 * EditorSearch.tsx.
 */

import { SearchQuery } from "@codemirror/search";
import type { EditorState } from "@codemirror/state";
import type { SearchOptions } from "../shared/DomSearchEngine";

/** Cap on counted matches — beyond this the counter shows "N+" and stops scanning
 * so a broad query in a huge file can't stall the UI. */
export const MATCH_COUNT_CAP = 2000;

/** Past this document size, the match count is computed with the windowed
 * `createMatchScanner` spread across idle callbacks instead of one synchronous
 * `matchStats` scan, which would freeze the UI: the scan walks the whole rope
 * when matches are sparse, and the cap only bounds the dense case. Live
 * search-as-you-type stays on (debounced) — same as VSCode's behaviour. */
export const SEARCH_HEAVY_BYTES = 10 * 1024 * 1024;

/** Window size (chars) processed per scanner step. Each step's cost is bounded
 * by the window regardless of match density, so stepping across idle callbacks
 * keeps the UI responsive. */
const SCAN_WINDOW = 1_000_000;

/** Build a CodeMirror SearchQuery from the SearchBar's term + options. */
export function buildQuery(term: string, opts: SearchOptions, replace?: string): SearchQuery {
	return new SearchQuery({
		search: term,
		caseSensitive: opts.caseSensitive,
		regexp: opts.regex,
		wholeWord: opts.wholeWord,
		replace: replace ?? "",
	});
}

export interface MatchStats {
	/** Total matches, capped at MATCH_COUNT_CAP. */
	count: number;
	/** 0-based index of the match under the current main selection, or -1. */
	index: number;
	/** True when the count hit the cap and is therefore a lower bound. */
	truncated: boolean;
}

/**
 * Count matches for `query` in `state` and locate the one under the current main
 * selection (the active match, after CodeMirror moves the selection to it).
 */
export function matchStats(state: EditorState, query: SearchQuery, cap = MATCH_COUNT_CAP): MatchStats {
	if (!query.valid) return { count: 0, index: -1, truncated: false };
	const sel = state.selection.main;
	const cursor = query.getCursor(state);
	let count = 0;
	let index = -1;
	let next = cursor.next();
	while (!next.done) {
		if (next.value.from === sel.from && next.value.to === sel.to) index = count;
		count++;
		if (count >= cap) return { count, index, truncated: true };
		next = cursor.next();
	}
	return { count, index, truncated: false };
}

/** A snapshot from a scanner step. `done` is true once the whole document has
 *  been scanned (or the cap was hit). */
export interface ScanProgress extends MatchStats {
	done: boolean;
}

/**
 * A resumable, windowed match counter. Each `step()` scans a bounded slice of the
 * document, so its cost stays small even when matches are sparse (a plain
 * `matchStats` walks the whole rope to find them, freezing the UI on huge files).
 * Drive it across idle callbacks, calling `step()` until `done`.
 *
 * Windows are scanned with a small overlap so a match straddling a boundary is
 * still found whole; each match is counted by the window that *owns* its start
 * (`from` in `[pos, ownedEnd)`), which dedupes the overlap.
 */
export function createMatchScanner(
	state: EditorState,
	query: SearchQuery,
	sel: { from: number; to: number },
	opts?: { window?: number; cap?: number },
): { step: (windows?: number) => ScanProgress } {
	const docLen = state.doc.length;
	const win = opts?.window ?? SCAN_WINDOW;
	const cap = opts?.cap ?? MATCH_COUNT_CAP;
	// Overlap must cover the longest possible match start-to-boundary gap. The
	// search term length bounds literal matches; clamp to a floor for regex.
	const overlap = Math.max(query.search.length, 256);
	// Invalid (empty) query → nothing to scan.
	let pos = query.valid ? 0 : docLen;
	let count = 0;
	let index = -1;

	return {
		step(windows = 1): ScanProgress {
			let processed = 0;
			while (pos < docLen && processed < windows && count < cap) {
				const ownedEnd = Math.min(pos + win, docLen);
				const scanEnd = Math.min(pos + win + overlap, docLen);
				const cursor = query.getCursor(state, pos, scanEnd);
				let next = cursor.next();
				while (!next.done) {
					const m = next.value;
					// Count only matches whose start is owned by this window.
					if (m.from >= pos && m.from < ownedEnd) {
						if (m.from === sel.from && m.to === sel.to) index = count;
						count++;
						if (count >= cap) break;
					}
					next = cursor.next();
				}
				pos = ownedEnd;
				processed++;
			}
			const truncated = count >= cap;
			return { count, index, truncated, done: truncated || pos >= docLen };
		},
	};
}
