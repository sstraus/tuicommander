/**
 * VS Code-style git change markers for the CodeMirror editor gutter.
 *
 * `parseDiffToChanges` is a pure function (unified diff text → per-line change
 * status) so it can be unit-tested without CodeMirror. The CodeMirror plumbing
 * (a StateField holding a RangeSet of gutter markers, fed by a StateEffect)
 * lives below it.
 */

import { type Extension, RangeSet, StateEffect, StateField } from "@codemirror/state";
import { EditorView, GutterMarker, gutter } from "@codemirror/view";

export type ChangeType = "added" | "modified" | "deleted";

export interface GutterChange {
	/** 1-based line number in the *new* (current) file. */
	line: number;
	type: ChangeType;
}

/**
 * Parse a unified diff (e.g. `git diff HEAD -- file`) into per-line change
 * markers for the new file.
 *
 * Classification per contiguous change block (a run of `-`/`+` lines between
 * context lines), matching the gitgutter/VS Code convention:
 *   - only additions          → "added"   (each new line)
 *   - additions + deletions   → "modified" (the new lines that replaced old ones)
 *   - only deletions          → "deleted" (a single marker on the line that now
 *     occupies the position where content was removed)
 */
export function parseDiffToChanges(diff: string): GutterChange[] {
	const changes: GutterChange[] = [];
	if (!diff) return changes;

	let newLine = 0; // 1-based line in the new file at the current cursor
	let inHunk = false;
	let delCount = 0; // consecutive deletions in the current block
	let addCount = 0; // consecutive additions in the current block
	let addStart = 0; // newLine where the current addition run began

	const flush = () => {
		if (addCount > 0) {
			const type: ChangeType = delCount > 0 ? "modified" : "added";
			for (let i = 0; i < addCount; i++) changes.push({ line: addStart + i, type });
		} else if (delCount > 0) {
			// Pure deletion: mark the line now sitting where content was removed.
			changes.push({ line: newLine, type: "deleted" });
		}
		delCount = 0;
		addCount = 0;
	};

	for (const raw of diff.split("\n")) {
		// A new file section resets hunk tracking (multi-file diffs).
		if (raw.startsWith("diff --git")) {
			flush();
			inHunk = false;
			continue;
		}
		if (raw.startsWith("@@")) {
			flush();
			const m = /@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
			newLine = m ? Number.parseInt(m[1], 10) : 0;
			inHunk = true;
			continue;
		}
		if (!inHunk) continue;

		const c = raw[0];
		if (c === " ") {
			flush();
			newLine++;
		} else if (c === "+") {
			if (addCount === 0) addStart = newLine;
			addCount++;
			newLine++;
		} else if (c === "-") {
			// An addition run ending in a deletion means two separate blocks.
			if (addCount > 0) flush();
			delCount++;
		} else if (c === "\\") {
			// "\ No newline at end of file" — not a content line.
		} else {
			flush();
		}
	}
	flush();
	return changes;
}

// --- CodeMirror integration ---

const setChanges = StateEffect.define<GutterChange[]>();

class ChangeGutterMarker extends GutterMarker {
	constructor(readonly kind: ChangeType) {
		super();
		this.elementClass = `cm-gitMarker cm-gitMarker-${kind}`;
	}
	override toDOM() {
		return document.createElement("span");
	}
}

/** Build a sorted RangeSet of gutter markers, clamped to the document. */
function buildMarkers(state: EditorView["state"], changes: GutterChange[]): RangeSet<GutterMarker> {
	const lineCount = state.doc.lines;
	// Dedup per line; "added"/"modified" win over a coincident "deleted".
	const byLine = new Map<number, ChangeType>();
	for (const ch of changes) {
		const line = Math.min(Math.max(ch.line, 1), lineCount);
		const existing = byLine.get(line);
		if (!existing || existing === "deleted") byLine.set(line, ch.type);
	}
	const markers = [...byLine.entries()]
		.sort((a, b) => a[0] - b[0])
		.map(([line, kind]) => new ChangeGutterMarker(kind).range(state.doc.line(line).from));
	return RangeSet.of(markers, true);
}

const changeField = StateField.define<RangeSet<GutterMarker>>({
	create: () => RangeSet.empty,
	update(set, tr) {
		let next = set.map(tr.changes);
		for (const e of tr.effects) {
			if (e.is(setChanges)) next = buildMarkers(tr.state, e.value);
		}
		return next;
	},
});

const gutterTheme = EditorView.baseTheme({
	".cm-changeGutter": { width: "3px", paddingLeft: "1px" },
	".cm-gitMarker": { display: "block", width: "3px", height: "100%" },
	".cm-gitMarker-added": { background: "rgba(158, 206, 106, 0.9)" },
	".cm-gitMarker-modified": { background: "rgba(100, 149, 237, 0.9)" },
	// Deletion shows a small downward caret at the top of the line.
	".cm-gitMarker-deleted": {
		position: "relative",
		background: "transparent",
	},
	".cm-gitMarker-deleted::before": {
		content: '""',
		position: "absolute",
		left: "-1px",
		top: "0",
		borderLeft: "4px solid transparent",
		borderRight: "4px solid transparent",
		borderTop: "5px solid rgba(247, 118, 142, 0.95)",
	},
});

const changeGutter = gutter({
	class: "cm-changeGutter",
	markers: (view) => view.state.field(changeField),
});

/** The editor extension that renders git change markers in the gutter. */
export function gitChangeGutter(): Extension {
	return [changeField, changeGutter, gutterTheme];
}

/** Build the StateEffect that updates the gutter markers. */
export function setChangesEffect(changes: GutterChange[]): StateEffect<GutterChange[]> {
	return setChanges.of(changes);
}
