import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import {
	buildQuery,
	createMatchScanner,
	matchStats,
	type ScanProgress,
} from "../../components/CodeEditorPanel/editorSearchEngine";

function stateOf(doc: string, selFrom?: number, selTo?: number): EditorState {
	return EditorState.create({
		doc,
		selection: selFrom === undefined ? undefined : { anchor: selFrom, head: selTo ?? selFrom },
	});
}

describe("buildQuery", () => {
	it("maps SearchBar options onto the CodeMirror SearchQuery", () => {
		const q = buildQuery("foo", { caseSensitive: true, regex: true, wholeWord: true }, "bar");
		expect(q.search).toBe("foo");
		expect(q.caseSensitive).toBe(true);
		expect(q.regexp).toBe(true);
		expect(q.wholeWord).toBe(true);
		expect(q.replace).toBe("bar");
	});

	it("defaults replace to empty string", () => {
		expect(buildQuery("x", { caseSensitive: false, regex: false, wholeWord: false }).replace).toBe("");
	});
});

describe("matchStats", () => {
	const opts = { caseSensitive: false, regex: false, wholeWord: false };

	it("counts all matches and finds the active one under the selection", () => {
		// matches at offsets 0, 4, 12
		const state = stateOf("foo\nfoo bar foo\nbaz", 4, 7);
		const stats = matchStats(state, buildQuery("foo", opts));
		expect(stats.count).toBe(3);
		expect(stats.index).toBe(1);
		expect(stats.truncated).toBe(false);
	});

	it("reports index -1 when the selection is not on a match", () => {
		const state = stateOf("foo bar foo", 0, 0);
		expect(matchStats(state, buildQuery("foo", opts)).index).toBe(-1);
	});

	it("returns zero for an invalid (empty) query", () => {
		expect(matchStats(stateOf("anything"), buildQuery("", opts))).toEqual({
			count: 0,
			index: -1,
			truncated: false,
		});
	});

	it("flags truncation when the count hits the cap", () => {
		const state = stateOf("a a a a a");
		const stats = matchStats(state, buildQuery("a", opts), 3);
		expect(stats.count).toBe(3);
		expect(stats.truncated).toBe(true);
	});
});

describe("createMatchScanner", () => {
	const opts = { caseSensitive: false, regex: false, wholeWord: false };

	/** Drive the scanner to completion, one window per step (forces the multi-window
	 *  path), and return the final snapshot. */
	function runToCompletion(
		state: EditorState,
		term: string,
		sel?: { from: number; to: number },
		window = 7,
	): ScanProgress {
		const scanner = createMatchScanner(state, buildQuery(term, opts), sel ?? { from: -1, to: -1 }, { window });
		let res = scanner.step();
		let guard = 0;
		while (!res.done && guard++ < 100_000) res = scanner.step();
		return res;
	}

	it("counts every match across many windows, matching the synchronous scan", () => {
		const doc = "foo bar ".repeat(100); // 100 "foo" matches over 800 chars
		const state = stateOf(doc);
		const res = runToCompletion(state, "foo");
		expect(res.done).toBe(true);
		expect(res.truncated).toBe(false);
		expect(res.count).toBe(matchStats(state, buildQuery("foo", opts)).count);
		expect(res.count).toBe(100);
	});

	it("counts matches landing exactly on window boundaries exactly once", () => {
		// Matches at offsets 0, 3, 6 with a window of 3 → one match owned per window.
		const state = stateOf("foofoofoo");
		const res = runToCompletion(state, "foo", undefined, 3);
		expect(res.count).toBe(3);
		expect(res.done).toBe(true);
	});

	it("locates the active match under the selection", () => {
		const doc = "foo ".repeat(60); // matches at 0,4,8,... ; 30th match starts at 116
		const state = stateOf(doc, 116, 119);
		const res = runToCompletion(state, "foo", { from: 116, to: 119 });
		expect(res.index).toBe(29);
		expect(res.count).toBe(60);
	});

	it("stops at the cap and flags truncation", () => {
		const state = stateOf("a a a a a a a a a a");
		const scanner = createMatchScanner(state, buildQuery("a", opts), { from: -1, to: -1 }, { window: 3, cap: 4 });
		let res = scanner.step();
		while (!res.done) res = scanner.step();
		expect(res.count).toBe(4);
		expect(res.truncated).toBe(true);
		expect(res.done).toBe(true);
	});

	it("completes immediately with zero for an empty query", () => {
		const res = runToCompletion(stateOf("anything"), "");
		expect(res).toEqual({ count: 0, index: -1, truncated: false, done: true });
	});
});
