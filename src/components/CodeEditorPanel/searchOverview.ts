/**
 * Search-match overview ruler for the CodeMirror editor: paints an orange tick on
 * a thin strip down the right edge for every line that contains a search match, so
 * matches are findable at a glance in long files — mirrors the CanvasTerminal
 * scrollbar marks (`--attention`).
 *
 * Sibling of gitGutter.ts's OverviewRuler. The two intentionally overlap on the
 * same right-edge strip with distinct colors (git = added/modified/deleted, search
 * = `--attention`); this ruler sits on a higher z-index so matches stay visible on
 * top of change ticks. Match positions come straight from the @codemirror/search
 * query, so no separate state plumbing is needed.
 */

import { getSearchQuery, type SearchQuery } from "@codemirror/search";
import type { Extension, Text } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

/** Hard cap on cursor iterations so a broad query in a huge file can't stall the
 * UI. The per-line dedup below usually exits far earlier (one tick per line). */
const MAX_ITERATIONS = 50_000;

/**
 * Collect the set of 1-based line numbers that contain at least one match for
 * `query` in `doc`. Deduped by line (one tick per line), bailing out as soon as
 * every line already has a match or the iteration cap is hit. Pure — exported for
 * testing without an EditorView.
 */
export function collectMatchLines(query: SearchQuery, doc: Text, maxIterations = MAX_ITERATIONS): Set<number> {
	const lines = new Set<number>();
	if (!query.valid) return lines;
	const total = doc.lines;
	if (total <= 0) return lines;
	const cursor = query.getCursor(doc);
	let iterations = 0;
	let next = cursor.next();
	while (!next.done && lines.size < total && iterations < maxIterations) {
		iterations++;
		lines.add(doc.lineAt(next.value.from).number);
		next = cursor.next();
	}
	return lines;
}

class SearchOverviewRuler {
	private readonly dom: HTMLElement;

	constructor(private readonly view: EditorView) {
		this.dom = document.createElement("div");
		this.dom.className = "cm-searchOverview";
		view.dom.appendChild(this.dom);
		this.render();
	}

	update(u: ViewUpdate) {
		const queryChanged = !getSearchQuery(u.startState).eq(getSearchQuery(u.state));
		if (u.docChanged || queryChanged) this.render();
	}

	private render() {
		this.dom.textContent = "";
		const { state } = this.view;
		const query = getSearchQuery(state);
		// While a search is active, this flag hides the git-change overview ruler so
		// the scrollbar shows ONLY the orange match marks — like the terminal. Closing
		// search clears the query (setSearchQuery to empty) → git ruler returns.
		this.view.dom.classList.toggle("cm-searching", query.valid);
		if (!query.valid) return;

		const total = state.doc.lines;
		if (total <= 0) return;

		// One tick per line: dedup by line so many matches on a line don't stack,
		// and once every line already has a tick there's nothing left to find.
		const seen = new Set<number>();
		const frag = document.createDocumentFragment();
		const cursor = query.getCursor(state);
		let iterations = 0;
		let next = cursor.next();
		while (!next.done && seen.size < total && iterations < MAX_ITERATIONS) {
			iterations++;
			const line = state.doc.lineAt(next.value.from).number;
			if (!seen.has(line)) {
				seen.add(line);
				const tick = document.createElement("div");
				tick.className = "cm-searchOverview-tick";
				// Center the tick on the line's relative position down the document.
				tick.style.top = `${((line - 0.5) / total) * 100}%`;
				frag.appendChild(tick);
			}
			next = cursor.next();
		}
		this.dom.appendChild(frag);
	}

	destroy() {
		this.view.dom.classList.remove("cm-searching");
		this.dom.remove();
	}
}

const searchOverviewPlugin = ViewPlugin.fromClass(SearchOverviewRuler);

const searchOverviewTheme = EditorView.baseTheme({
	".cm-searchOverview": {
		position: "absolute",
		top: "0",
		right: "0",
		bottom: "0",
		// 14px to match the terminal's scrollbar track. The strip is a sibling of
		// .cm-scroller with a high z-index, so marks paint directly ON TOP of the
		// scrollbar at the right edge — same look as the terminal, not inset beside it.
		width: "14px",
		pointerEvents: "none",
		zIndex: "201",
	},
	".cm-searchOverview-tick": {
		position: "absolute",
		right: "0",
		// Full-width marks spanning the strip — matches the terminal scrollbar marks.
		width: "100%",
		height: "2px",
		background: "var(--attention, #e8984c)",
	},
	// Hide the git-change overview ruler while searching so only match marks show.
	".cm-searching .cm-changeOverview": {
		display: "none",
	},
});

/** The editor extension: search-match ticks on the scrollbar overview ruler. */
export function searchOverview(): Extension {
	return [searchOverviewPlugin, searchOverviewTheme];
}
