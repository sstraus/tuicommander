/**
 * GitLens/Zed-style inline git blame for the CodeMirror editor: a single dim
 * annotation ("Author · relative time · summary") at the end of the active line,
 * following the cursor.
 *
 * Mirrors gitGutter.ts's shape — a `setBlame` StateEffect feeds a `blameField`,
 * and a `ViewPlugin` emits exactly one `Decoration.widget` at the end of the
 * cursor's line, recomputed only on selection/doc/blame/enabled changes (never on
 * a blame *fetch* per keystroke — CodeEditorTab fetches on load/save/revision).
 *
 * Uncommitted lines reuse gitGutter's `changesField` (the diff-vs-HEAD already
 * loaded for the change gutter): an added/modified line shows "You · Uncommitted
 * changes" instead of the now-stale HEAD attribution.
 *
 * Blame data is fetched in Rust (`get_file_blame`); this module only renders.
 */

import { type EditorState, type Extension, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from "@codemirror/view";
import { formatRelativeTime } from "../../utils/time";
import { type ChangeType, changesField, type GutterChange } from "./gitGutter";

/** Mirrors the Rust `BlameLine` struct (git.rs). */
export interface BlameLine {
	hash: string;
	author: string;
	/** Unix timestamp in seconds. */
	author_time: number;
	/** Commit subject (first line of the message). */
	summary: string;
	/** 1-based line number in the current file. */
	line_number: number;
	content: string;
}

// --- Effects + fields ---

const setBlame = StateEffect.define<BlameLine[]>();
const setBlameEnabled = StateEffect.define<boolean>();

/** Build the effect that pushes fresh blame data into the editor. */
export function setBlameEffect(lines: BlameLine[]): StateEffect<BlameLine[]> {
	return setBlame.of(lines);
}

/** Build the effect that toggles the annotation on/off (settings-driven). */
export function setBlameEnabledEffect(enabled: boolean): StateEffect<boolean> {
	return setBlameEnabled.of(enabled);
}

const blameField = StateField.define<BlameLine[]>({
	create: () => [],
	update(value, tr) {
		for (const e of tr.effects) if (e.is(setBlame)) return e.value;
		return value;
	},
});

const enabledField = StateField.define<boolean>({
	create: () => true,
	update(value, tr) {
		for (const e of tr.effects) if (e.is(setBlameEnabled)) return e.value;
		return value;
	},
});

// --- Pure helpers (unit-tested without a DOM view) ---

/** Render the dim annotation text for a committed line. */
export function formatBlameText(bl: BlameLine): string {
	const rel = formatRelativeTime(bl.author_time * 1000);
	return bl.summary ? `${bl.author} · ${rel} · ${bl.summary}` : `${bl.author} · ${rel}`;
}

/** Index blame entries by their 1-based line number (last write wins). */
export function indexBlame(lines: BlameLine[]): Map<number, BlameLine> {
	const m = new Map<number, BlameLine>();
	for (const bl of lines) m.set(bl.line_number, bl);
	return m;
}

/** Collapse gutter changes to one type per line — added/modified beats deleted. */
export function indexChanges(changes: GutterChange[]): Map<number, ChangeType> {
	const m = new Map<number, ChangeType>();
	for (const c of changes) {
		const existing = m.get(c.line);
		if (!existing || existing === "deleted") m.set(c.line, c.type);
	}
	return m;
}

/**
 * Pick the annotation text for a 1-based line, or null if there's nothing to show.
 * An uncommitted (added/modified) line wins over the stale HEAD blame.
 */
export function pickBlameText(
	lineNumber: number,
	blameByLine: Map<number, BlameLine>,
	changeByLine: Map<number, ChangeType>,
): string | null {
	const change = changeByLine.get(lineNumber);
	if (change === "added" || change === "modified") return "You · Uncommitted changes";
	const bl = blameByLine.get(lineNumber);
	return bl ? formatBlameText(bl) : null;
}

class BlameWidget extends WidgetType {
	constructor(readonly text: string) {
		super();
	}
	override eq(other: BlameWidget) {
		return other.text === this.text;
	}
	override toDOM() {
		const span = document.createElement("span");
		span.className = "cm-inlineBlame";
		span.textContent = this.text;
		return span;
	}
	override ignoreEvent() {
		return true;
	}
}

/**
 * Build the (zero or one) widget decoration for the current cursor line. Pure
 * over (state, blame map, change map, enabled) so it's testable without a view.
 */
export function buildBlameDecorations(
	state: EditorState,
	blameByLine: Map<number, BlameLine>,
	changeByLine: Map<number, ChangeType>,
	enabled: boolean,
): DecorationSet {
	if (!enabled) return Decoration.none;
	const line = state.doc.lineAt(state.selection.main.head);
	const text = pickBlameText(line.number, blameByLine, changeByLine);
	if (text === null) return Decoration.none;
	// side: 1 → after the line content; anchored at line end so it trails the code.
	return Decoration.set([Decoration.widget({ widget: new BlameWidget(text), side: 1 }).range(line.to)]);
}

// --- View plugin ---

const inlineBlamePlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;
		private blameByLine: Map<number, BlameLine>;
		private changeByLine: Map<number, ChangeType>;

		constructor(view: EditorView) {
			this.blameByLine = indexBlame(view.state.field(blameField));
			this.changeByLine = indexChanges(view.state.field(changesField));
			this.decorations = buildBlameDecorations(
				view.state,
				this.blameByLine,
				this.changeByLine,
				view.state.field(enabledField),
			);
		}

		update(u: ViewUpdate) {
			const blameChanged = u.startState.field(blameField) !== u.state.field(blameField);
			const changesChanged = u.startState.field(changesField) !== u.state.field(changesField);
			const enabledChanged = u.startState.field(enabledField) !== u.state.field(enabledField);
			if (blameChanged) this.blameByLine = indexBlame(u.state.field(blameField));
			if (changesChanged) this.changeByLine = indexChanges(u.state.field(changesField));
			// Recompute only when the cursor line, the document, or the data changed —
			// never on unrelated viewport/scroll updates.
			if (u.selectionSet || u.docChanged || blameChanged || changesChanged || enabledChanged) {
				this.decorations = buildBlameDecorations(
					u.state,
					this.blameByLine,
					this.changeByLine,
					u.state.field(enabledField),
				);
			}
		}
	},
	{ decorations: (v) => v.decorations },
);

const inlineBlameTheme = EditorView.baseTheme({
	".cm-inlineBlame": {
		paddingLeft: "2.5em",
		color: "var(--fg-muted)",
		fontStyle: "italic",
		opacity: "0.75",
		// Non-interactive trailing annotation — never part of selection or clicks.
		userSelect: "none",
		pointerEvents: "none",
	},
});

/**
 * The editor extension: inline blame annotation on the active line. `changesField`
 * is shared with `gitChangeGutter()` (CodeMirror dedupes the identical field), so
 * the widget reads the same diff-vs-HEAD without a second fetch.
 */
export function inlineBlame(): Extension {
	return [blameField, enabledField, changesField, inlineBlamePlugin, inlineBlameTheme];
}
