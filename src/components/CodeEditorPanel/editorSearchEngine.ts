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
