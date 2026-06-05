// --- Phase 1.3 worker-side grid state + repaint scheduling ---
//
// The grid-relevant subset of CanvasTerminal.onFrame, isolated so it is pure
// and unit-testable (the main onFrame also handles selection, links, scroll
// re-requests and acks, which stay on the main thread). The worker only needs
// to maintain a rowMap + dirty tracking from decoded frames, then paint via the
// shared gridRenderer.
//
// NOTE: this duplicates the rowMap-maintenance logic from onFrame (NOT the
// paint logic — that is unified in gridRenderer). Unifying onFrame's grid
// subset too is a smaller, separate follow-up; kept apart for now to avoid
// destabilising the entangled live onFrame.

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

/**
 * Apply a decoded frame to the worker grid state. Mirrors onFrame's grid logic:
 * geometry change or full-screen frame replaces the rowMap (full repaint); a
 * scroll change with a partial frame discards stale rows (full repaint); an
 * otherwise-partial frame merges rows and marks them dirty (incremental).
 * Only ever SETS fullRepaintNeeded; the render pass clears it + pendingDirtyRows.
 */
export function applyFrameToGrid(s: WorkerGridState, frame: DecodedFrame): void {
	const screenRowCount = frame.screenRows || s.lastScreenRows || 24;
	const geomChanged = frame.screenRows !== s.lastScreenRows || frame.screenCols !== s.lastScreenCols;
	const scrollChanged = frame.displayOffset !== s.lastDisplayOffset || frame.historySize !== s.lastHistorySize;

	if (geomChanged) {
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
	}

	s.lastScreenRows = frame.screenRows;
	s.lastScreenCols = frame.screenCols;
	s.lastDisplayOffset = frame.displayOffset;
	s.lastHistorySize = frame.historySize;

	if (frame.rows.length >= screenRowCount) {
		// Backend sent the whole screen → replace rowMap to discard stale entries.
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
	} else if (scrollChanged && !geomChanged) {
		// Partial frame after a scroll: old rowMap entries map to wrong rows now.
		s.rowMap.clear();
		s.fullRepaintNeeded = true;
	}

	for (const row of frame.rows) {
		s.rowMap.set(row.index, row);
		s.pendingDirtyRows.add(row.index);
	}
}

export interface RepaintScheduler {
	/** Mark dirty and ensure a rAF is pending (coalesces multiple calls). */
	schedule(): void;
	/** Cancel any pending rAF (e.g. on teardown). */
	stop(): void;
}

/**
 * Dirty-flag rAF coalescer: render-on-dirty only, at most one rAF in flight. No
 * continuous loop — a rAF is requested only when schedule() is called, so the
 * worker is idle between frames.
 */
export function createRepaintScheduler(
	raf: (cb: FrameRequestCallback) => number,
	cancelRaf: (id: number) => void,
	render: () => void,
): RepaintScheduler {
	let rafId: number | null = null;
	let dirty = false;

	function tick(): void {
		rafId = null;
		if (!dirty) return;
		dirty = false;
		render();
	}

	function schedule(): void {
		dirty = true;
		if (rafId === null) rafId = raf(tick);
	}

	function stop(): void {
		if (rafId !== null) {
			cancelRaf(rafId);
			rafId = null;
		}
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
