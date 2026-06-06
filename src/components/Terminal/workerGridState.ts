// --- Phase 1.3 worker-side grid state + repaint scheduling ---
//
// The grid-relevant subset of CanvasTerminal.onFrame, isolated so it is pure
// and unit-testable (the main onFrame also handles selection, links, scroll
// re-requests and acks, which stay on the main thread). The worker only needs
// to maintain a rowMap + dirty tracking from decoded frames, then paint via the
// shared gridRenderer.
//
// Phase 3: the frame->grid DECISION (geom/scroll/full-replace/scroll-wait) is now
// shared with the main onFrame via decideFrameGrid, so the main overlay's rowMap
// and the worker's paint rowMap can never diverge on what to clear/merge. Only the
// side effects differ by design (main also clears selection/links, re-requests a
// full frame, and tracks currentFrame; the worker just maintains its rowMap).

import { type CellMetrics, type DecodedFrame, type DecodedRow, GUTTER_PX } from "./canvasTerminalUtils";
import type { GridRenderer } from "./gridRenderer";
import type { ResizeMessage } from "./workerProtocol";

export interface WorkerGridState {
	rowMap: Map<number, DecodedRow>;
	pendingDirtyRows: Set<number>;
	fullRepaintNeeded: boolean;
	lastScreenRows: number;
	lastScreenCols: number;
	lastDisplayOffset: number;
	lastHistorySize: number;
}

export function createWorkerGridState(): WorkerGridState {
	return {
		rowMap: new Map(),
		pendingDirtyRows: new Set(),
		fullRepaintNeeded: true,
		lastScreenRows: -1,
		lastScreenCols: -1,
		lastDisplayOffset: -1,
		lastHistorySize: -1,
	};
}

/** Previous frame geometry/scroll state needed to decide what a new frame implies. */
export interface FrameGridPrev {
	lastScreenRows: number;
	lastScreenCols: number;
	lastDisplayOffset: number;
	lastHistorySize: number;
}

/** What a newly-decoded frame means for the rowMap (shared by main + worker). */
export interface FrameGridDecision {
	geomChanged: boolean;
	scrollChanged: boolean;
	/** The frame carries a full screen of rows → replace the rowMap wholesale. */
	fullReplace: boolean;
	/** Partial frame after a scroll → clear and wait for a full frame; do NOT merge. */
	scrollWait: boolean;
}

/**
 * Decide what a decoded frame implies for the rowMap — the single source of truth
 * shared by the main onFrame (overlay rowMap) and the worker applyFrameToGrid
 * (paint rowMap), so the two can never diverge on clear/merge.
 *
 * `fallbackRows` is the screen-row count to assume when the frame omits its own
 * (frame.screenRows === 0): the main passes lastResizeRows, the worker passes its
 * lastScreenRows. The backend always sets frame.screenRows in practice, so the
 * fallback only differs on degenerate frames.
 */
export function decideFrameGrid(prev: FrameGridPrev, frame: DecodedFrame, fallbackRows: number): FrameGridDecision {
	const geomChanged = frame.screenRows !== prev.lastScreenRows || frame.screenCols !== prev.lastScreenCols;
	const scrollChanged = frame.displayOffset !== prev.lastDisplayOffset || frame.historySize !== prev.lastHistorySize;
	const screenRowCount = frame.screenRows || fallbackRows || 24;
	const fullReplace = frame.rows.length >= screenRowCount;
	const scrollWait = !fullReplace && scrollChanged && !geomChanged;
	return { geomChanged, scrollChanged, fullReplace, scrollWait };
}

/**
 * Apply a decoded frame to the worker grid state via the shared decideFrameGrid.
 * Geometry change or full-screen frame replaces the rowMap (full repaint); a
 * partial frame after a scroll discards stale rows and waits for the full frame
 * the MAIN thread re-requests — it does NOT merge the partial rows (mirrors
 * onFrame), else the worker's paint would desync from main's overlay; an
 * otherwise-partial frame merges rows and marks them dirty (incremental).
 * Only ever SETS fullRepaintNeeded; the render pass clears it + pendingDirtyRows.
 */
export function applyFrameToGrid(s: WorkerGridState, frame: DecodedFrame): FrameGridDecision {
	const decision = decideFrameGrid(s, frame, s.lastScreenRows);

	if (decision.geomChanged) {
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
	}

	s.lastScreenRows = frame.screenRows;
	s.lastScreenCols = frame.screenCols;
	s.lastDisplayOffset = frame.displayOffset;
	s.lastHistorySize = frame.historySize;

	if (decision.fullReplace) {
		// Backend sent the whole screen → replace rowMap to discard stale entries.
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
	} else if (decision.scrollWait) {
		// Partial frame after a scroll: old rowMap entries map to wrong rows now.
		// Clear and wait for the re-requested full frame — do NOT merge the partial
		// rows (that would desync the worker's paint from main's overlay rowMap).
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
		return decision;
	}

	for (const row of frame.rows) {
		s.rowMap.set(row.index, row);
		s.pendingDirtyRows.add(row.index);
	}
	return decision;
}

export interface RepaintScheduler {
	/** Mark dirty and ensure a rAF is pending (coalesces multiple calls). */
	schedule(): void;
	/** Cancel any pending rAF (e.g. on teardown). */
	stop(): void;
}

/** Optional timer-based fallback for environments where rAF can be suspended. */
export interface RepaintTimers {
	setTimer: (cb: () => void, ms: number) => number;
	clearTimer: (id: number) => void;
	/** Max wait before forcing a paint when rAF hasn't fired (default 100ms). */
	fallbackMs?: number;
}

/**
 * Dirty-flag rAF coalescer: render-on-dirty only, at most one paint in flight. No
 * continuous loop — work is scheduled only when schedule() is called, so the
 * worker is idle between frames.
 *
 * WebKit SUSPENDS `requestAnimationFrame` inside a DedicatedWorker under CPU
 * pressure or backgrounding (e.g. a Rust build pinning the CPU), which froze the
 * terminal glyph paint while input still worked. When `timers` is supplied, a
 * setTimeout fallback races the rAF so a paint always lands within `fallbackMs`
 * even if rAF never fires; whichever wins cancels the other. Timers are throttled
 * but not suspended like worker rAF, so this guarantees forward progress.
 */
export function createRepaintScheduler(
	raf: (cb: FrameRequestCallback) => number,
	cancelRaf: (id: number) => void,
	render: () => void,
	timers?: RepaintTimers,
): RepaintScheduler {
	let rafId: number | null = null;
	let timerId: number | null = null;
	let dirty = false;
	const fallbackMs = timers?.fallbackMs ?? 100;

	function clearPending(): void {
		if (rafId !== null) {
			cancelRaf(rafId);
			rafId = null;
		}
		if (timerId !== null && timers) {
			timers.clearTimer(timerId);
			timerId = null;
		}
	}

	function tick(): void {
		// Whichever of rAF/timer fired first — cancel the loser, then paint.
		clearPending();
		if (!dirty) return;
		dirty = false;
		render();
	}

	function schedule(): void {
		dirty = true;
		if (rafId === null) rafId = raf(tick);
		if (timers && timerId === null) timerId = timers.setTimer(tick, fallbackMs);
	}

	function stop(): void {
		clearPending();
		dirty = false;
	}

	return { schedule, stop };
}

/** What applyResize needs to mutate (worker entry supplies these). */
export interface ResizeTarget {
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
	gridRenderer: GridRenderer;
	setMetrics(m: CellMetrics): void;
	setFont(family: string, weight: number | string): void;
}

/**
 * Apply a resize/config message inside the worker. Mirrors CanvasTerminal's
 * remeasure: device pixels = logical * dpr, then re-apply the dpr scale + gutter
 * translate (setting canvas size resets the transform). Theme/metrics/font are
 * pushed so the shared gridRenderer can paint identically to the main path.
 */
export function applyResize(t: ResizeTarget, msg: ResizeMessage): void {
	t.ctx.canvas.width = Math.round(msg.w * msg.dpr);
	t.ctx.canvas.height = Math.round(msg.h * msg.dpr);
	// Canvas resize clears the transform; re-establish identity → scale → gutter.
	t.ctx.setTransform(1, 0, 0, 1, 0, 0);
	t.ctx.scale(msg.dpr, msg.dpr);
	t.ctx.translate(GUTTER_PX, 0);
	t.gridRenderer.setTheme(msg.bgDefault, msg.fgDefault);
	t.setFont(msg.fontFamily, msg.fontWeight);
	t.setMetrics(msg.metrics);
}
