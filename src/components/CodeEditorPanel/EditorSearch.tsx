import { findNext, findPrevious, replaceAll, replaceNext, setSearchQuery } from "@codemirror/search";
import type { EditorView } from "@codemirror/view";
import { type Component, createEffect, createSignal, onCleanup } from "solid-js";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { buildQuery, createMatchScanner, MATCH_COUNT_CAP, matchStats, SEARCH_HEAVY_BYTES } from "./editorSearchEngine";

/** Run `cb` when the main thread is idle. WKWebView lacks requestIdleCallback,
 *  so fall back to a short timer that reports a small idle budget. */
const onIdle: (cb: (deadline: { timeRemaining: () => number }) => void) => void =
	typeof window !== "undefined" && "requestIdleCallback" in window
		? (cb) => window.requestIdleCallback(cb)
		: (cb) => setTimeout(() => cb({ timeRemaining: () => 8 }), 16);

export interface EditorSearchProps {
	visible: boolean;
	view: EditorView | undefined;
	/** When true, the replace row is offered (read-only files get search only). */
	editable: boolean;
	onClose: () => void;
}

const EMPTY_OPTS: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };

/**
 * Drives the shared <SearchBar> against a CodeMirror EditorView, replacing
 * CodeMirror's built-in search panel so the editor matches the terminal's search
 * UX. The replace row is only wired when the file is editable.
 */
export const EditorSearch: Component<EditorSearchProps> = (props) => {
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);
	const [truncated, setTruncated] = createSignal(false);

	let lastTerm = "";
	let lastOpts: SearchOptions = EMPTY_OPTS;
	/** Bumped on every stats refresh so a slow in-flight async scan that's been
	 *  superseded (new query, closed, unmounted) stops updating the signals. */
	let scanGen = 0;
	/** Debounces the expensive jump-to-match + count on search-as-you-type so
	 *  typing stays smooth on large files (findNext scans the rope; the count is
	 *  async but still worth coalescing). Explicit navigation runs immediately. */
	let searchDebounce: ReturnType<typeof setTimeout> | undefined;
	onCleanup(() => {
		scanGen++;
		clearTimeout(searchDebounce);
	});

	/** Push a query into the view (highlights + scrollbar ticks). Stats are read
	 *  separately via refreshStats *after* the cursor-moving command runs — the
	 *  active-match index is only correct once findNext/replace has moved the
	 *  selection, so computing stats here too would just be a wasted full scan. */
	const setQuery = (term: string, opts: SearchOptions, replace = "") => {
		props.view?.dispatch({ effects: setSearchQuery.of(buildQuery(term, opts, replace)) });
	};

	const resetStats = () => {
		setMatchCount(0);
		setMatchIndex(-1);
		setTruncated(false);
	};

	const refreshStats = () => {
		const view = props.view;
		// Supersede any in-flight async scan even when we bail or go synchronous.
		const gen = ++scanGen;
		if (!view || !lastTerm) return;
		const query = buildQuery(lastTerm, lastOpts);

		// Small docs: one synchronous scan is simplest and flicker-free.
		if (view.state.doc.length <= SEARCH_HEAVY_BYTES) {
			const stats = matchStats(view.state, query);
			setMatchCount(stats.count);
			setMatchIndex(stats.index);
			setTruncated(stats.truncated);
			return;
		}

		// Large docs: count in windowed steps across idle callbacks so the scan
		// never blocks typing. Negative count = "still counting" → counter hidden.
		setMatchCount(-1);
		setMatchIndex(-1);
		setTruncated(false);
		const scanner = createMatchScanner(view.state, query, view.state.selection.main);
		const pump = (deadline: { timeRemaining: () => number }) => {
			if (gen !== scanGen) return; // superseded
			let res = scanner.step();
			while (!res.done && deadline.timeRemaining() > 2) res = scanner.step();
			if (res.done) {
				setMatchCount(res.count);
				setMatchIndex(res.index);
				setTruncated(res.truncated);
			} else {
				onIdle(pump);
			}
		};
		onIdle(pump);
	};

	// Clear the query (and thus highlights + scrollbar ticks) when search closes.
	createEffect(() => {
		if (!props.visible) {
			props.view?.dispatch({ effects: setSearchQuery.of(buildQuery("", EMPTY_OPTS)) });
			lastTerm = "";
			clearTimeout(searchDebounce);
			scanGen++; // cancel any in-flight async count
			setMatchIndex(-1);
			setMatchCount(0);
			setTruncated(false);
		}
	});

	const handleSearch = (term: string, opts: SearchOptions) => {
		lastTerm = term;
		lastOpts = opts;
		// Highlights/scrollbar ticks update immediately (CM only decorates the
		// viewport, so this is cheap even on huge files).
		setQuery(term, opts);
		clearTimeout(searchDebounce);
		if (!term || !props.view) {
			scanGen++; // cancel any in-flight async count
			resetStats();
			return;
		}
		// Debounce the jump-to-first-match + count so rapid typing doesn't scan the
		// rope per keystroke. Explicit Enter/Next/Prev (below) run without delay.
		searchDebounce = setTimeout(() => {
			if (!props.view) return;
			findNext(props.view);
			refreshStats();
		}, 120);
	};

	const handleNext = () => {
		if (!lastTerm || !props.view) return;
		clearTimeout(searchDebounce);
		findNext(props.view);
		refreshStats();
	};

	const handlePrev = () => {
		if (!lastTerm || !props.view) return;
		clearTimeout(searchDebounce);
		findPrevious(props.view);
		refreshStats();
	};

	const handleReplace = (replacement: string) => {
		if (!lastTerm || !props.view) return;
		clearTimeout(searchDebounce);
		setQuery(lastTerm, lastOpts, replacement);
		replaceNext(props.view);
		refreshStats();
	};

	const handleReplaceAll = (replacement: string) => {
		if (!lastTerm || !props.view) return;
		clearTimeout(searchDebounce);
		setQuery(lastTerm, lastOpts, replacement);
		replaceAll(props.view);
		refreshStats();
	};

	return (
		<SearchBar
			visible={props.visible}
			onSearch={handleSearch}
			onNext={handleNext}
			onPrev={handlePrev}
			onClose={props.onClose}
			matchIndex={matchIndex()}
			matchCount={matchCount()}
			matchLabel={truncated() ? `${MATCH_COUNT_CAP}+` : undefined}
			onReplace={props.editable ? handleReplace : undefined}
			onReplaceAll={props.editable ? handleReplaceAll : undefined}
		/>
	);
};
