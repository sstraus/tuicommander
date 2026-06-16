import { EditorState } from "@codemirror/state";
import type { DecorationSet } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import type { GutterChange } from "../../components/CodeEditorPanel/gitGutter";
import {
	type BlameLine,
	buildBlameDecorations,
	formatBlameText,
	indexBlame,
	indexChanges,
	pickBlameText,
} from "../../components/CodeEditorPanel/inlineBlame";

const BLAME: BlameLine[] = [
	{
		hash: "a",
		author: "Alice",
		author_time: 1_700_000_000,
		summary: "Initial commit",
		line_number: 1,
		content: "line1",
	},
	{ hash: "b", author: "Bob", author_time: 1_700_000_001, summary: "Second commit", line_number: 2, content: "line2" },
	{ hash: "c", author: "Carol", author_time: 1_700_000_002, summary: "Third commit", line_number: 3, content: "line3" },
];

/** Doc with three lines; cursor placed at the start of `line` (1-based). */
function stateOnLine(line: number): EditorState {
	const doc = "line1\nline2\nline3";
	const state = EditorState.create({ doc });
	const anchor = state.doc.line(line).from;
	return EditorState.create({ doc, selection: { anchor } });
}

/** Flatten a decoration set into [{from, to, text}] for assertions. */
function widgets(set: DecorationSet, len: number): { from: number; to: number; text: string }[] {
	const out: { from: number; to: number; text: string }[] = [];
	set.between(0, len, (from, to, value) => {
		out.push({ from, to, text: (value.spec.widget as unknown as { text: string }).text });
	});
	return out;
}

describe("formatBlameText", () => {
	it("renders 'author · relative · summary' with the summary", () => {
		const text = formatBlameText(BLAME[0]);
		expect(text.startsWith("Alice · ")).toBe(true);
		expect(text.endsWith(" · Initial commit")).toBe(true);
	});

	it("omits the summary segment when the commit has no subject", () => {
		const text = formatBlameText({ ...BLAME[0], summary: "" });
		expect(text.startsWith("Alice · ")).toBe(true);
		expect(text.includes(" · Initial")).toBe(false);
		expect(text.includes("undefined")).toBe(false);
	});
});

describe("pickBlameText", () => {
	it("returns the blame annotation for a committed line", () => {
		const text = pickBlameText(2, indexBlame(BLAME), new Map());
		expect(text?.startsWith("Bob · ")).toBe(true);
		expect(text?.endsWith(" · Second commit")).toBe(true);
	});

	it("returns 'You · Uncommitted changes' for a modified line, ignoring stale blame", () => {
		const changes: GutterChange[] = [{ line: 2, type: "modified" }];
		expect(pickBlameText(2, indexBlame(BLAME), indexChanges(changes))).toBe("You · Uncommitted changes");
	});

	it("treats an added line as uncommitted", () => {
		const changes: GutterChange[] = [{ line: 3, type: "added" }];
		expect(pickBlameText(3, indexBlame(BLAME), indexChanges(changes))).toBe("You · Uncommitted changes");
	});

	it("does NOT treat a deleted marker as uncommitted (shows blame)", () => {
		const changes: GutterChange[] = [{ line: 1, type: "deleted" }];
		const text = pickBlameText(1, indexBlame(BLAME), indexChanges(changes));
		expect(text?.endsWith(" · Initial commit")).toBe(true);
	});

	it("returns null for a line with neither blame nor change", () => {
		expect(pickBlameText(99, indexBlame(BLAME), new Map())).toBeNull();
	});
});

describe("buildBlameDecorations", () => {
	it("emits one widget at the end of the active line", () => {
		const state = stateOnLine(1);
		const set = buildBlameDecorations(state, indexBlame(BLAME), new Map(), true);
		const w = widgets(set, state.doc.length);
		expect(w).toHaveLength(1);
		expect(w[0].from).toBe(state.doc.line(1).to);
		expect(w[0].to).toBe(state.doc.line(1).to);
		expect(w[0].text.endsWith(" · Initial commit")).toBe(true);
	});

	it("moves the widget to the new line and updates the text when the cursor moves", () => {
		const line1 = buildBlameDecorations(stateOnLine(1), indexBlame(BLAME), new Map(), true);
		const s2 = stateOnLine(2);
		const line2 = buildBlameDecorations(s2, indexBlame(BLAME), new Map(), true);

		const w1 = widgets(line1, 17);
		const w2 = widgets(line2, s2.doc.length);
		expect(w1[0].text.endsWith(" · Initial commit")).toBe(true);
		expect(w2[0].from).toBe(s2.doc.line(2).to);
		expect(w2[0].text.endsWith(" · Second commit")).toBe(true);
		// The annotation followed the cursor — different position and text.
		expect(w2[0].from).not.toBe(w1[0].from);
	});

	it("shows 'Uncommitted' on a modified active line", () => {
		const state = stateOnLine(2);
		const changes: GutterChange[] = [{ line: 2, type: "modified" }];
		const set = buildBlameDecorations(state, indexBlame(BLAME), indexChanges(changes), true);
		const w = widgets(set, state.doc.length);
		expect(w).toHaveLength(1);
		expect(w[0].text).toBe("You · Uncommitted changes");
	});

	it("emits nothing when disabled", () => {
		const state = stateOnLine(1);
		const set = buildBlameDecorations(state, indexBlame(BLAME), new Map(), false);
		expect(widgets(set, state.doc.length)).toHaveLength(0);
	});

	it("emits nothing when the active line has no blame and no change", () => {
		const state = stateOnLine(1);
		const set = buildBlameDecorations(state, new Map(), new Map(), true);
		expect(widgets(set, state.doc.length)).toHaveLength(0);
	});
});
