import { findNext, findPrevious, replaceAll, replaceNext, setSearchQuery } from "@codemirror/search";
import type { EditorView } from "@codemirror/view";
import { type Component, createEffect, createSignal } from "solid-js";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { buildQuery, MATCH_COUNT_CAP, matchStats } from "./editorSearchEngine";

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
		if (!view || !lastTerm) return;
		const stats = matchStats(view.state, buildQuery(lastTerm, lastOpts));
		setMatchCount(stats.count);
		setMatchIndex(stats.index);
		setTruncated(stats.truncated);
	};

	// Clear the query (and thus highlights + scrollbar ticks) when search closes.
	createEffect(() => {
		if (!props.visible) {
			props.view?.dispatch({ effects: setSearchQuery.of(buildQuery("", EMPTY_OPTS)) });
			lastTerm = "";
			setMatchIndex(-1);
			setMatchCount(0);
			setTruncated(false);
		}
	});

	const handleSearch = (term: string, opts: SearchOptions) => {
		lastTerm = term;
		lastOpts = opts;
		setQuery(term, opts);
		// Jump to the first match from the cursor so search-as-you-type tracks it.
		if (term && props.view) {
			findNext(props.view);
			refreshStats();
		} else {
			resetStats();
		}
	};

	const handleNext = () => {
		if (!lastTerm || !props.view) return;
		findNext(props.view);
		refreshStats();
	};

	const handlePrev = () => {
		if (!lastTerm || !props.view) return;
		findPrevious(props.view);
		refreshStats();
	};

	const handleReplace = (replacement: string) => {
		if (!lastTerm || !props.view) return;
		setQuery(lastTerm, lastOpts, replacement);
		replaceNext(props.view);
		refreshStats();
	};

	const handleReplaceAll = (replacement: string) => {
		if (!lastTerm || !props.view) return;
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
