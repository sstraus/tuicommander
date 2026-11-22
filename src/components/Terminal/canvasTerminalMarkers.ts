import { hiddenMarkerRows, type OverlayBlock, planSuggestOverlay, promptEnd, type RowSnapshot } from "./suggestOverlay";

export interface MarkerPlan {
	hidden: ReadonlySet<number>;
	answerMarkers: ReadonlySet<number>;
	key: string;
	blocks: OverlayBlock[];
	/** Offscreen prompt whose missing prefix must be read before classifying this viewport. */
	pendingPrompt?: number;
	promptBoundary?: PromptBoundary;
}

export interface PromptBoundary {
	end: number;
	/** False when the read ended while still inside the prompt. */
	resolved: boolean;
}

/** Ordinary marker continuation lookback; prompt ownership is independent of this bound. */
export const MARKER_LOOKBACK_ROWS = 128;
/** Older prompt anchors need independently cached ownership, even within lookback. */
const PROMPT_INLINE_ROWS = 32;
/** Maximum synchronous ownership prefix and rows fetched by one background read. */
export const MARKER_PREFIX_BUDGET = 128;

export function markerPromptStart(promptRows: readonly number[], base = 0): number | undefined {
	let latest: number | undefined;
	for (const row of promptRows)
		if (row < base - PROMPT_INLINE_ROWS && (latest === undefined || row > latest)) latest = row;
	return latest;
}

/** Shared by live-frame and cached smooth-scroll painting; includes the bottom overscan row. */
export function planCanvasMarkers(
	rows: number,
	getRow: (i: number) => RowSnapshot | null,
	promptRows: readonly number[] = [],
	boundary?: PromptBoundary,
): MarkerPlan {
	let firstRow = -MARKER_LOOKBACK_ROWS;
	const start = markerPromptStart(promptRows);
	let promptBoundary: PromptBoundary | undefined;
	if (start !== undefined) {
		if (boundary && !boundary.resolved && boundary.end < rows + 1)
			return { hidden: new Set(), answerMarkers: new Set(), key: "", blocks: [], pendingPrompt: start };
		let end = boundary?.end;
		if (end === undefined) {
			let missing = !getRow(start);
			const limit = Math.min(rows + 1, start + MARKER_PREFIX_BUDGET);
			end = promptEnd(start, limit, (i) => {
				const row = getRow(i);
				if (!row) missing = true;
				return row;
			});
			// A cache miss is not evidence that the prompt ended. Fetch its prefix
			// rather than guessing from the indentation of the visible marker.
			if (missing || (end === limit && limit < rows + 1))
				return { hidden: new Set(), answerMarkers: new Set(), key: "", blocks: [], pendingPrompt: start };
		}
		promptBoundary = { end, resolved: boundary?.resolved ?? end < rows + 1 };
		firstRow = Math.max(firstRow, end);
	}
	const plan = planSuggestOverlay(rows + 1, getRow, firstRow);
	return {
		promptBoundary,
		hidden: hiddenMarkerRows(plan.blocks),
		answerMarkers: plan.answerMarkers,
		key: plan.key,
		blocks: plan.blocks.filter((block) => block.row < rows),
	};
}

/** Read only until output starts, not the rest of a potentially long-running turn. */
export async function readMarkerPromptBoundary(
	start: number,
	end: number,
	readRows: (start: number, end: number) => Promise<RowSnapshot[] | null>,
	budget = Number.POSITIVE_INFINITY,
): Promise<PromptBoundary | null> {
	const stop = Math.min(end, start + budget);
	for (let from = start; from < stop; from += 256) {
		// Include the previous row so the wire wrap flag at a chunk boundary is preserved.
		const overlap = from > start ? 1 : 0;
		const chunk = await readRows(from - overlap, Math.min(from + 256, stop));
		if (!chunk) return null;
		// The preceding row supplies blank separation; only the new rows are classified.
		const next = promptEnd(0, chunk.length, (i) => chunk[i] ?? null);
		if (next < chunk.length) return { end: from - overlap + next, resolved: true };
	}
	return { end: stop, resolved: false };
}

/** Where a plan's viewport-relative rows sit: the absolute top row and the row-cache generation. */
export interface MarkerPlanAnchor {
	base: number;
	generation: number;
}

/** `plan` with every viewport-relative row moved by `shift`. */
function shiftMarkerPlan(plan: MarkerPlan, shift: number): MarkerPlan {
	if (shift === 0) return plan;
	return {
		hidden: new Set([...plan.hidden].map((row) => row + shift)),
		answerMarkers: new Set([...plan.answerMarkers].map((row) => row + shift)),
		key: `${plan.key}@${shift}`,
		blocks: plan.blocks.map((block) => ({ ...block, row: block.row + shift })),
	};
}

/**
 * Reuse a plan while every row generation it inspected is unchanged.
 *
 * With an `anchor`, a plan that waits for an offscreen prompt boundary returns
 * the last resolved plan of the same cache generation, moved by the scroll since
 * then, and keeps `pendingPrompt` so the caller still starts the read. Painting
 * the pending plan itself would show every hidden marker row and drop every
 * answer gutter until the read lands, then repaint them: two jumps for each
 * output row that scrolls the prompt further out of the cache.
 */
export function createCanvasMarkerPlanner(): (
	rows: number,
	getRow: (i: number) => RowSnapshot | null,
	prompts?: readonly number[],
	boundary?: PromptBoundary,
	anchor?: MarkerPlanAnchor,
) => MarkerPlan {
	const plan = createCachedCanvasMarkerPlanner();
	let resolved: { plan: MarkerPlan; rows: number; anchor: MarkerPlanAnchor } | undefined;
	return (rows, getRow, prompts = [], boundary, anchor) => {
		const fresh = plan(rows, getRow, prompts, boundary);
		if (!anchor) return fresh;
		if (fresh.pendingPrompt === undefined) {
			resolved = { plan: fresh, rows, anchor };
			return fresh;
		}
		if (!resolved || resolved.rows !== rows || resolved.anchor.generation !== anchor.generation) return fresh;
		return {
			...shiftMarkerPlan(resolved.plan, resolved.anchor.base - anchor.base),
			pendingPrompt: fresh.pendingPrompt,
		};
	};
}

function createCachedCanvasMarkerPlanner(): typeof planCanvasMarkers {
	let previous: MarkerPlan | undefined;
	let previousRows = -1;
	let previousPrompt: number | undefined;
	let previousBoundary: PromptBoundary | undefined;
	let dependencies = new Map<number, RowSnapshot | null>();
	return (rows, getRow, prompts = [], boundary) => {
		const prompt = markerPromptStart(prompts);
		if (
			previous &&
			rows === previousRows &&
			prompt === previousPrompt &&
			boundary?.end === previousBoundary?.end &&
			boundary?.resolved === previousBoundary?.resolved &&
			[...dependencies].every(([index, snapshot]) => getRow(index) === snapshot)
		)
			return previous;
		dependencies = new Map();
		previous = planCanvasMarkers(
			rows,
			(index) => {
				if (!dependencies.has(index)) dependencies.set(index, getRow(index));
				return dependencies.get(index)!;
			},
			prompts,
			boundary,
		);
		previousRows = rows;
		previousPrompt = prompt;
		previousBoundary = boundary;
		return previous;
	};
}

/** Cache text and wrap interpretation separately; the preceding row may change alone. */
export function cachedMarkerSnapshot<T extends object>(
	read: (row: T) => string,
	endColumn?: (row: T, offset: number) => number,
): (row: T, wrapped: boolean) => RowSnapshot {
	const snapshots = new WeakMap<T, RowSnapshot>();
	return (row, wrapped) => {
		let snapshot = snapshots.get(row);
		if (!snapshot || snapshot.isWrapped !== wrapped) {
			snapshot = { text: snapshot?.text ?? read(row), isWrapped: wrapped };
			if (endColumn) snapshot.endColumn = (offset) => endColumn(row, offset);
			snapshots.set(row, snapshot);
		}
		return snapshot;
	};
}
