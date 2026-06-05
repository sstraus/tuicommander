import { describe, expect, it, vi } from "vitest";

import type { DecodedFrame, DecodedRow } from "../canvasTerminalUtils";
import { applyFrameToGrid, createRepaintScheduler, createWorkerGridState } from "../workerGridState";

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

	it("discards stale rows and forces full repaint when a partial frame arrives after scroll", () => {
		const s = createWorkerGridState();
		applyFrameToGrid(s, makeFrame({ screenRows: 3, rows: [makeRow(0), makeRow(1), makeRow(2)] }));
		s.fullRepaintNeeded = false;
		s.pendingDirtyRows.clear();

		applyFrameToGrid(s, makeFrame({ screenRows: 3, displayOffset: 5, historySize: 100, rows: [makeRow(0, 6)] }));

		expect(s.fullRepaintNeeded).toBe(true);
		expect(s.rowMap.size).toBe(1);
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
