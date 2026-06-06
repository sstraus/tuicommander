import { describe, expect, it, vi } from "vitest";

import type { DecodedFrame, DecodedRow } from "../canvasTerminalUtils";
import {
	applyFrameToGrid,
	createRepaintScheduler,
	createWorkerGridState,
	decideFrameGrid,
	type FrameGridPrev,
} from "../workerGridState";

function makeRow(index: number, count = 8): DecodedRow {
	return {
		index,
		count,
		codepoints: new Uint32Array(count),
		fg: new Uint32Array(count),
		bg: new Uint32Array(count),
		attrs: new Uint8Array(count),
	};
}

function makeFrame(opts: {
	screenRows: number;
	screenCols?: number;
	displayOffset?: number;
	historySize?: number;
	rows: DecodedRow[];
}): DecodedFrame {
	return {
		cursorRow: 0,
		cursorCol: 0,
		cursorVisible: true,
		cursorShape: "block",
		displayOffset: opts.displayOffset ?? 0,
		historySize: opts.historySize ?? 0,
		hasSelection: false,
		keyboardFlags: 0,
		bell: false,
		mouseMode: 0,
		sgrMouse: false,
		focusReporting: false,
		bracketedPaste: false,
		screenRows: opts.screenRows,
		screenCols: opts.screenCols ?? 80,
		rows: opts.rows,
	};
}

describe("applyFrameToGrid", () => {
	it("populates the rowMap and forces full repaint on a full-screen frame", () => {
		const s = createWorkerGridState();
		const rows = [makeRow(0), makeRow(1), makeRow(2)];
		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows }));

		expect(s.rowMap.size).toBe(3);
		expect(s.fullRepaintNeeded).toBe(true);
		expect([...s.pendingDirtyRows].sort((a, b) => a - b)).toEqual([0, 1, 2]);
	});

	it("merges a partial update incrementally without forcing full repaint", () => {
		const s = createWorkerGridState();
		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows: [makeRow(0), makeRow(1), makeRow(2)] }));
		// Simulate a render pass clearing the dirty flags.
		s.fullRepaintNeeded = false;
		s.pendingDirtyRows.clear();

		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows: [makeRow(1, 5)] }));

		expect(s.fullRepaintNeeded).toBe(false);
		expect([...s.pendingDirtyRows]).toEqual([1]);
		expect(s.rowMap.get(1)?.count).toBe(5);
		expect(s.rowMap.size).toBe(3);
	});

	it("clears the rowMap and forces full repaint on a geometry change", () => {
		const s = createWorkerGridState();
		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows: [makeRow(0), makeRow(1), makeRow(2)] }));
		s.fullRepaintNeeded = false;
		s.pendingDirtyRows.clear();

		applyFrameToGrid(s, makeFrame({ screenRows: 5, screenCols: 100, rows: [makeRow(0, 4)] }));

		expect(s.fullRepaintNeeded).toBe(true);
		// rowMap cleared then the single incoming row applied
		expect(s.rowMap.size).toBe(1);
	});

	it("discards stale rows WITHOUT merging the partial frame after a scroll (waits for full frame)", () => {
		const s = createWorkerGridState();
		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows: [makeRow(0), makeRow(1), makeRow(2)] }));
		s.fullRepaintNeeded = false;
		s.pendingDirtyRows.clear();

		const decision = applyFrameToGrid(
			s,
			makeFrame({ screenRows: 3, displayOffset: 5, historySize: 100, rows: [makeRow(0, 6)] }),
		);

		// Mirrors main onFrame: the partial rows are keyed to the OLD viewportTop, so
		// merging them under the new displayOffset would paint ghost content. Clear and
		// wait for the full frame main re-requests — the partial rows are NOT merged.
		expect(decision.scrollWait).toBe(true);
		expect(s.fullRepaintNeeded).toBe(true);
		expect(s.rowMap.size).toBe(0);
		expect(s.pendingDirtyRows.size).toBe(0);
	});
});

describe("decideFrameGrid (shared main/worker grid decision)", () => {
	const prev: FrameGridPrev = { lastScreenRows: 24, lastScreenCols: 80, lastDisplayOffset: 0, lastHistorySize: 100 };

	it("flags geomChanged when screen rows or cols differ", () => {
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 30, rows: [] }), 24).geomChanged).toBe(true);
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, screenCols: 100, rows: [] }), 24).geomChanged).toBe(true);
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, screenCols: 80, rows: [] }), 24).geomChanged).toBe(false);
	});

	it("flags scrollChanged when displayOffset or historySize differ", () => {
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 5, historySize: 100, rows: [] }), 24)
				.scrollChanged,
		).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 0, historySize: 200, rows: [] }), 24)
				.scrollChanged,
		).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 0, historySize: 100, rows: [] }), 24)
				.scrollChanged,
		).toBe(false);
	});

	it("flags fullReplace when the frame carries >= screenRows rows", () => {
		const rows = Array.from({ length: 24 }, (_, i) => makeRow(i));
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, historySize: 100, rows }), 24).fullReplace).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, historySize: 100, rows: [makeRow(0)] }), 24).fullReplace,
		).toBe(false);
	});

	it("uses fallbackRows for the full-replace threshold when frame.screenRows is 0", () => {
		const rows = Array.from({ length: 3 }, (_, i) => makeRow(i));
		// frame.screenRows 0 → threshold = fallbackRows (3) → 3 rows is a full replace.
		// geomChanged because 0 !== prev.lastScreenRows (24).
		const d = decideFrameGrid(prev, makeFrame({ screenRows: 0, historySize: 100, rows }), 3);
		expect(d.fullReplace).toBe(true);
	});

	it("flags scrollWait only for a partial frame after a pure scroll (no geom change)", () => {
		const partialScroll = decideFrameGrid(
			prev,
			makeFrame({ screenRows: 24, displayOffset: 5, historySize: 100, rows: [makeRow(0)] }),
			24,
		);
		expect(partialScroll.scrollWait).toBe(true);

		// A geometry change is not a scrollWait even if scroll also changed.
		const geomAndScroll = decideFrameGrid(
			prev,
			makeFrame({ screenRows: 30, displayOffset: 5, historySize: 100, rows: [makeRow(0)] }),
			24,
		);
		expect(geomAndScroll.scrollWait).toBe(false);

		// A full frame after a scroll is a fullReplace, not a scrollWait.
		const fullScroll = decideFrameGrid(
			prev,
			makeFrame({
				screenRows: 24,
				displayOffset: 5,
				historySize: 100,
				rows: Array.from({ length: 24 }, (_, i) => makeRow(i)),
			}),
			24,
		);
		expect(fullScroll.scrollWait).toBe(false);
		expect(fullScroll.fullReplace).toBe(true);
	});
});

// --- rowMap parity: the worker's applyFrameToGrid must track the SAME rowMap the
// main onFrame builds. Both use decideFrameGrid; this reference applier replicates
// the grid-relevant subset of CanvasTerminal.onFrame (side effects omitted) and
// asserts the rowMap keys + flags stay identical across a representative sequence.
describe("rowMap parity: worker applyFrameToGrid vs main onFrame grid logic", () => {
	interface MainGrid extends FrameGridPrev {
		rowMap: Map<number, DecodedRow>;
		fullRepaintNeeded: boolean;
	}
	function mainGridApply(g: MainGrid, frame: DecodedFrame, lastResizeRows: number): void {
		const d = decideFrameGrid(g, frame, lastResizeRows);
		if (d.geomChanged) {
			g.rowMap.clear();
			g.fullRepaintNeeded = true;
		}
		if (d.scrollChanged || d.geomChanged) {
			g.lastDisplayOffset = frame.displayOffset;
			g.lastHistorySize = frame.historySize;
			g.lastScreenRows = frame.screenRows;
			g.lastScreenCols = frame.screenCols;
		}
		if (d.fullReplace) {
			g.rowMap.clear();
			g.fullRepaintNeeded = true;
		} else if (d.scrollWait) {
			g.rowMap.clear();
			g.fullRepaintNeeded = true;
			return; // main returns early here (re-requests a full frame); rows NOT merged
		}
		for (const row of frame.rows) g.rowMap.set(row.index, row);
	}

	it("keeps identical rowMap keys + fullRepaint across geom/scroll/partial/full frames", () => {
		const worker = createWorkerGridState();
		const main: MainGrid = {
			rowMap: new Map(),
			fullRepaintNeeded: true,
			lastScreenRows: -1,
			lastScreenCols: -1,
			lastDisplayOffset: -1,
			lastHistorySize: -1,
		};
		const RESIZE_ROWS = 3;
		const full = () => makeFrame({ screenRows: 3, historySize: 100, rows: [makeRow(0), makeRow(1), makeRow(2)] });
		const partialEdit = (hist: number) => makeFrame({ screenRows: 3, historySize: hist, rows: [makeRow(1, 5)] });
		const partialAfterScroll = () =>
			makeFrame({ screenRows: 3, displayOffset: 5, historySize: 100, rows: [makeRow(0, 6)] });
		const geomChange = () => makeFrame({ screenRows: 5, screenCols: 100, rows: [makeRow(0, 4)] });

		const sequence = [full(), partialEdit(100), partialAfterScroll(), full(), geomChange(), full()];
		for (const frame of sequence) {
			applyFrameToGrid(worker, frame);
			mainGridApply(main, frame, RESIZE_ROWS);
			expect([...worker.rowMap.keys()].sort((a, b) => a - b)).toEqual([...main.rowMap.keys()].sort((a, b) => a - b));
			expect(worker.fullRepaintNeeded).toBe(main.fullRepaintNeeded);
			// Simulate each side's render pass clearing the repaint flags.
			worker.fullRepaintNeeded = false;
			worker.pendingDirtyRows.clear();
			main.fullRepaintNeeded = false;
		}
	});
});

describe("createRepaintScheduler (dirty-flag rAF coalescing)", () => {
	it("coalesces multiple schedule() calls into a single rAF and renders once", () => {
		const cap: { cb?: FrameRequestCallback } = {};
		const raf = vi.fn((cb: FrameRequestCallback) => {
			cap.cb = cb;
			return 1;
		});
		const cancel = vi.fn();
		const render = vi.fn();
		const sched = createRepaintScheduler(raf, cancel, render);

		sched.schedule();
		sched.schedule();
		sched.schedule();
		expect(raf).toHaveBeenCalledTimes(1);
		expect(render).not.toHaveBeenCalled();

		cap.cb?.(0);
		expect(render).toHaveBeenCalledTimes(1);
	});

	it("renders nothing on a tick with no pending dirty work", () => {
		const cap: { cb?: FrameRequestCallback } = {};
		const raf = vi.fn((cb: FrameRequestCallback) => {
			cap.cb = cb;
			return 1;
		});
		const render = vi.fn();
		const sched = createRepaintScheduler(raf, vi.fn(), render);

		sched.schedule();
		cap.cb?.(0); // renders once, clears dirty
		expect(render).toHaveBeenCalledTimes(1);

		// A new rAF is requested only on the next schedule()
		sched.schedule();
		expect(raf).toHaveBeenCalledTimes(2);
		cap.cb?.(0);
		expect(render).toHaveBeenCalledTimes(2);
	});

	it("stop() cancels a pending rAF", () => {
		const raf = vi.fn(() => 42);
		const cancel = vi.fn();
		const sched = createRepaintScheduler(raf, cancel, vi.fn());

		sched.schedule();
		sched.stop();
		expect(cancel).toHaveBeenCalledWith(42);
	});

	it("renders via the timer fallback when rAF never fires (WebKit worker suspension)", () => {
		const timerCap: { cb?: () => void } = {};
		const raf = vi.fn(() => 1); // rAF is requested but never invokes its callback
		const cancelRaf = vi.fn();
		const render = vi.fn();
		const setTimer = vi.fn((cb: () => void) => {
			timerCap.cb = cb;
			return 7;
		});
		const clearTimer = vi.fn();
		const sched = createRepaintScheduler(raf, cancelRaf, render, {
			setTimer,
			clearTimer,
			fallbackMs: 100,
		});

		sched.schedule();
		expect(raf).toHaveBeenCalledTimes(1);
		expect(setTimer).toHaveBeenCalledTimes(1);
		expect(render).not.toHaveBeenCalled();

		// rAF stayed suspended; the timer fires and paints instead.
		timerCap.cb?.();
		expect(render).toHaveBeenCalledTimes(1);
		// The losing rAF is cancelled so it can't double-render later.
		expect(cancelRaf).toHaveBeenCalledWith(1);
	});

	it("rAF winning the race cancels the pending fallback timer (no double render)", () => {
		const rafCap: { cb?: FrameRequestCallback } = {};
		const raf = vi.fn((cb: FrameRequestCallback) => {
			rafCap.cb = cb;
			return 1;
		});
		const render = vi.fn();
		const clearTimer = vi.fn();
		const sched = createRepaintScheduler(raf, vi.fn(), render, {
			setTimer: () => 7,
			clearTimer,
			fallbackMs: 100,
		});

		sched.schedule();
		rafCap.cb?.(0);
		expect(render).toHaveBeenCalledTimes(1);
		expect(clearTimer).toHaveBeenCalledWith(7);
	});

	it("stop() cancels both the rAF and the fallback timer", () => {
		const cancelRaf = vi.fn();
		const clearTimer = vi.fn();
		const sched = createRepaintScheduler(() => 42, cancelRaf, vi.fn(), {
			setTimer: () => 7,
			clearTimer,
			fallbackMs: 100,
		});

		sched.schedule();
		sched.stop();
		expect(cancelRaf).toHaveBeenCalledWith(42);
		expect(clearTimer).toHaveBeenCalledWith(7);
	});
});

// --- applyResize (geometry + dpr + theme + metrics + font) ---

import { GUTTER_PX } from "../canvasTerminalUtils";
import type { GridRenderer } from "../gridRenderer";
import { applyResize } from "../workerGridState";
import type { ResizeMessage } from "../workerProtocol";

function makeMetrics(): import("../canvasTerminalUtils").CellMetrics {
	return {
		cellWidth: 8,
		cellHeight: 16,
		baseline: 12,
		fontSize: 14,
		dpr: 2,
		scaledCellWidth: 16,
		scaledCellHeight: 32,
	};
}

function makeFakeCtx() {
	const canvas = { width: 0, height: 0 };
	const calls: string[] = [];
	const ctx = {
		canvas,
		setTransform: (a: number, b: number, c: number, d: number, e: number, f: number) =>
			calls.push(`setTransform(${a},${b},${c},${d},${e},${f})`),
		scale: (x: number, y: number) => calls.push(`scale(${x},${y})`),
		translate: (x: number, y: number) => calls.push(`translate(${x},${y})`),
	} as unknown as OffscreenCanvasRenderingContext2D;
	return { ctx, canvas, calls };
}

function makeResizeMsg(): ResizeMessage {
	return {
		type: "resize",
		w: 100,
		h: 50,
		dpr: 2,
		cols: 12,
		rows: 3,
		metrics: makeMetrics(),
		bgDefault: "#222",
		fgDefault: "#ddd",
		fontFamily: "Hack",
		fontWeight: 400,
	};
}

describe("applyResize", () => {
	it("sets the canvas device pixels to logical*dpr and applies dpr scale + gutter translate", () => {
		const { ctx, canvas, calls } = makeFakeCtx();
		const setTheme = vi.fn();
		const gridRenderer = { setTheme } as unknown as GridRenderer;
		let metrics: unknown = null;
		let font: { family: string; weight: number | string } | null = null;

		applyResize(
			{ ctx, gridRenderer, setMetrics: (m) => (metrics = m), setFont: (family, weight) => (font = { family, weight }) },
			makeResizeMsg(),
		);

		expect(canvas.width).toBe(200); // 100 * dpr 2
		expect(canvas.height).toBe(100); // 50 * dpr 2
		expect(calls).toEqual([`setTransform(1,0,0,1,0,0)`, `scale(2,2)`, `translate(${GUTTER_PX},0)`]);
		expect(setTheme).toHaveBeenCalledWith("#222", "#ddd");
		expect(metrics).toEqual(makeMetrics());
		expect(font).toEqual({ family: "Hack", weight: 400 });
	});
});
