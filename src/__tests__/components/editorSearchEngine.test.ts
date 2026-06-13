import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { buildQuery, matchStats } from "../../components/CodeEditorPanel/editorSearchEngine";

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
