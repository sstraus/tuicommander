/**
 * VS Code-style git change markers for the CodeMirror editor gutter, plus a
 * scrollbar overview ruler showing the same changes as ticks down the right edge
 * (so they're findable at a glance in long files).
 *
 * The unified-diff → per-line-change parsing lives in Rust (`get_gutter_changes`,
 * all business logic in Rust); this module only holds the shared `GutterChange`
 * shape and the CodeMirror plumbing. A single `setChanges` StateEffect feeds both
 * the gutter (a RangeSet of markers) and the overview ruler (the raw change list).
 */

import { type Extension, RangeSet, StateEffect, StateField } from "@codemirror/state";
import { EditorView, GutterMarker, gutter, ViewPlugin, type ViewUpdate } from "@codemirror/view";

export type ChangeType = "added" | "modified" | "deleted";

export interface GutterChange {
	/** 1-based line number in the *new* (current) file. */
	line: number;
	type: ChangeType;
}

// --- CodeMirror integration ---

const setChanges = StateEffect.define<GutterChange[]>();

/** Marker colors, shared by the gutter and the scrollbar overview ruler. */
const CHANGE_COLORS: Record<ChangeType, string> = {
	added: "rgba(158, 206, 106, 0.9)",
	modified: "rgba(100, 149, 237, 0.9)",
	deleted: "rgba(247, 118, 142, 0.95)",
};

/**
 * Collapse changes to one entry per line, clamped to `[1, lineCount]`. A coincident
 * "added"/"modified" wins over a "deleted" on the same line (matches the gutter).
 */
function collapseByLine(changes: GutterChange[], lineCount: number): Map<number, ChangeType> {
	const byLine = new Map<number, ChangeType>();
	for (const ch of changes) {
		const line = Math.min(Math.max(ch.line, 1), lineCount);
		const existing = byLine.get(line);
		if (!existing || existing === "deleted") byLine.set(line, ch.type);
	}
	return byLine;
}

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
	const markers = [...collapseByLine(changes, state.doc.lines).entries()]
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
	".cm-gitMarker-added": { background: CHANGE_COLORS.added },
	".cm-gitMarker-modified": { background: CHANGE_COLORS.modified },
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
		borderTop: `5px solid ${CHANGE_COLORS.deleted}`,
	},
});

const changeGutter = gutter({
	class: "cm-changeGutter",
	markers: (view) => view.state.field(changeField),
});

// --- Scrollbar overview ruler ---

/** Holds the raw change list (fed by the same `setChanges` effect as the gutter)
 * so the overview ruler can render ticks without re-parsing. */
const changesField = StateField.define<GutterChange[]>({
	create: () => [],
	update(value, tr) {
		for (const e of tr.effects) {
			if (e.is(setChanges)) return e.value;
		}
		return value;
	},
});

/**
 * Paints the change list as colored ticks down a thin strip at the editor's right
 * edge — a VS Code-style overview ruler. Lives on `view.dom` (the non-scrolling
 * editor root) so ticks map to absolute document position, not the scrolled view.
 */
class OverviewRuler {
	private readonly dom: HTMLElement;

	constructor(private readonly view: EditorView) {
		this.dom = document.createElement("div");
		this.dom.className = "cm-changeOverview";
		view.dom.appendChild(this.dom);
		this.render();
	}

	update(u: ViewUpdate) {
		if (u.docChanged || u.startState.field(changesField) !== u.state.field(changesField)) {
			this.render();
		}
	}

	private render() {
		const changes = this.view.state.field(changesField);
		const total = this.view.state.doc.lines;
		this.dom.textContent = "";
		if (changes.length === 0 || total <= 0) return;
		const frag = document.createDocumentFragment();
		for (const [line, type] of collapseByLine(changes, total)) {
			const tick = document.createElement("div");
			tick.className = "cm-changeOverview-tick";
			// Center the tick on the line's relative position down the document.
			tick.style.top = `${((line - 0.5) / total) * 100}%`;
			tick.style.background = CHANGE_COLORS[type];
			frag.appendChild(tick);
		}
		this.dom.appendChild(frag);
	}

	destroy() {
		this.dom.remove();
	}
}

const overviewRuler = ViewPlugin.fromClass(OverviewRuler);

const overviewTheme = EditorView.baseTheme({
	".cm-changeOverview": {
		position: "absolute",
		top: "0",
		right: "0",
		bottom: "0",
		width: "4px",
		pointerEvents: "none",
		zIndex: "200",
	},
	".cm-changeOverview-tick": {
		position: "absolute",
		right: "0",
		width: "4px",
		height: "2px",
		borderRadius: "1px",
	},
});

/** The editor extension: git change markers in the gutter + scrollbar overview. */
export function gitChangeGutter(): Extension {
	return [changeField, changesField, changeGutter, gutterTheme, overviewRuler, overviewTheme];
}

/** Build the StateEffect that updates the gutter markers. */
export function setChangesEffect(changes: GutterChange[]): StateEffect<GutterChange[]> {
	return setChanges.of(changes);
}
