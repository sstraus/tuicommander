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

/**
 * Collapse contiguous same-type changed lines into one entry per run, anchored at
 * the run's first line — so the overview ruler shows a single tick for a whole-new
 * file (every line "added") instead of a solid bar. A line gap OR a type change
 * starts a new run. Exported for testing.
 */
export function coalesceChangeRuns(
	changes: GutterChange[],
	lineCount: number,
): Array<{ line: number; type: ChangeType }> {
	const sorted = [...collapseByLine(changes, lineCount).entries()].sort((a, b) => a[0] - b[0]);
	const runs: Array<{ line: number; type: ChangeType }> = [];
	let prevLine = Number.NEGATIVE_INFINITY;
	let prevType: ChangeType | null = null;
	for (const [line, type] of sorted) {
		if (line === prevLine + 1 && type === prevType) {
			prevLine = line;
			continue;
		}
		runs.push({ line, type });
		prevLine = line;
		prevType = type;
	}
	return runs;
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
		const total = this.view.state.doc.lines;
		this.dom.textContent = "";
		const runs = coalesceChangeRuns(this.view.state.field(changesField), total);
		if (runs.length === 0) return;
		const frag = document.createDocumentFragment();
		for (const { line, type } of runs) {
			const tick = document.createElement("div");
			tick.className = "cm-changeOverview-tick";
			// Center the tick on the run's first line, relative to the document.
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
		// 14px to match the terminal's scrollbar marks (sibling of .cm-scroller, high
		// z-index → painted ON the scrollbar at the right edge), not a 4px stub beside it.
		width: "14px",
		pointerEvents: "none",
		zIndex: "200",
	},
	".cm-changeOverview-tick": {
		position: "absolute",
		right: "0",
		width: "100%",
		height: "2px",
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
