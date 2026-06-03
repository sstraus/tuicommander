import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { hoverLinkField, setHoverLink } from "../../components/CodeEditorPanel/CodeEditorTab";

/** Collect the [from, to] ranges held by the hover-link decoration field. */
function ranges(state: EditorState): { from: number; to: number }[] {
	const out: { from: number; to: number }[] = [];
	state.field(hoverLinkField).between(0, state.doc.length, (from, to) => {
		out.push({ from, to });
	});
	return out;
}

describe("hoverLinkField", () => {
	it("drops a stale hover range when the document is replaced with empty (the RangeError repro)", () => {
		const big = "x".repeat(20000);
		let state = EditorState.create({ doc: big, extensions: [hoverLinkField] });
		// Hover sets a decoration near the end of the big document.
		state = state.update({ effects: setHoverLink.of({ from: 15090, to: 15097 }) }).state;
		expect(ranges(state)).toEqual([{ from: 15090, to: 15097 }]);

		// Swap the whole document for an empty one (file switch / failed read).
		state = state.update({ changes: { from: 0, to: state.doc.length, insert: "" } }).state;

		// Without mapping, the field would still hold [15090, 15097] and a subsequent
		// transaction throws "Position 15097 is out of range for changeset of length 0".
		expect(ranges(state)).toEqual([]);
		expect(() => state.update({ selection: { anchor: 0 } }).state).not.toThrow();
	});

	it("remaps the hover range through an edit that shifts it", () => {
		let state = EditorState.create({ doc: "hello world", extensions: [hoverLinkField] });
		state = state.update({ effects: setHoverLink.of({ from: 6, to: 11 }) }).state; // "world"
		state = state.update({ changes: { from: 0, insert: "AB" } }).state; // shift right by 2
		expect(ranges(state)).toEqual([{ from: 8, to: 13 }]);
	});

	it("clears the decoration on a null setHoverLink effect", () => {
		let state = EditorState.create({ doc: "hello", extensions: [hoverLinkField] });
		state = state.update({ effects: setHoverLink.of({ from: 0, to: 5 }) }).state;
		state = state.update({ effects: setHoverLink.of(null) }).state;
		expect(ranges(state)).toEqual([]);
	});
});
