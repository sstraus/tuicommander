// --- Phase 1 render worker entry (DedicatedWorker, type: "module") ---
//
// Thin shell wiring the pure, unit-tested modules together:
//   decode (canvasTerminalUtils) -> grid state (workerGridState) -> paint
//   (gridRenderer) on the transferred OffscreenCanvas, gated on fonts ready
//   (workerProtocol) and coalesced via a dirty-flag rAF.
// Instantiated once per canvas node from CanvasTerminal (gated by the
// offscreenRenderer setting + capability detection in later steps).
//
// NOTE: metrics + theme + font are posted by the main thread in Step 1.5; until
// then paintGrid is a no-op (no metrics), but the decode/grid/scheduling
// pipeline is fully wired.

import { type CellMetrics, decodeBinaryFrame } from "./canvasTerminalUtils";
import { createGridRenderer, type GridRenderer } from "./gridRenderer";
import { applyFrameToGrid, applyResize, createRepaintScheduler, createWorkerGridState } from "./workerGridState";
import {
	createRendererState,
	type FontDescriptors,
	type FontEnv,
	type FontFaceLike,
	reduceRendererMessage,
	type WorkerMessage,
} from "./workerProtocol";

// Real font environment: the global FontFace constructor + the worker's font
// set. `self.fonts` is the DedicatedWorkerGlobalScope FontFaceSet (absent from
// the DOM typelib we compile against, hence the cast).
const fontEnv: FontEnv = {
	create(family: string, source: ArrayBuffer, descriptors?: FontDescriptors): FontFaceLike {
		return new FontFace(family, source, descriptors as FontFaceDescriptors);
	},
	add(face: FontFaceLike): void {
		(self as unknown as { fonts: { add(f: FontFaceLike): void } }).fonts.add(face);
	},
};

const gridState = createWorkerGridState();
let gridRenderer: GridRenderer | null = null;

// Set by Step 1.5 resize/theme messages; until then paint is skipped.
let metrics: CellMetrics | null = null;
let fontFamily = "monospace";
let fontWeight: number | string = 400;

// Frame-timing instrumentation (toggled by the main thread via the "timing"
// message). When off, every guarded block is skipped — zero overhead. `dirtySince`
// stamps when a frame first made the grid dirty; the gap to paint is the "present"
// latency that exposes WebKit deprioritizing the worker's rAF/timer.
let timingEnabled = false;
let dirtySince = 0;

function postTimingSample(kind: "paint" | "sched", ms: number): void {
	(self as unknown as { postMessage(m: unknown): void }).postMessage({ type: "frameTiming", kind, ms });
}

const scheduler = createRepaintScheduler(
	(cb) => self.requestAnimationFrame(cb),
	(id) => self.cancelAnimationFrame(id),
	() => {
		if (!gridRenderer || !metrics) return;
		const paintStart = timingEnabled ? performance.now() : 0;
		if (timingEnabled && dirtySince) {
			postTimingSample("sched", paintStart - dirtySince);
			dirtySince = 0;
		}
		const dirty = gridState.pendingDirtyRows.size > 0 ? new Set(gridState.pendingDirtyRows) : undefined;
		gridRenderer.paintGrid(gridState.rowMap, metrics, {
			fullRepaint: gridState.fullRepaintNeeded,
			dirtyIndices: dirty,
		});
		gridState.fullRepaintNeeded = false;
		gridState.pendingDirtyRows.clear();
		if (timingEnabled) postTimingSample("paint", performance.now() - paintStart);
	},
	// WebKit can fully suspend worker rAF under CPU pressure (e.g. a Rust build at
	// 100% CPU), freezing the terminal. A setTimeout fallback guarantees the paint
	// still lands so glyphs/Ink animations never go completely immobile.
	// 16ms (≈vsync): WebKit also DEPRIORITIZES worker rAF for sporadic paints (one
	// keystroke at a time), so the fallback — not rAF — drives single-char repaints.
	// At 100ms that added a ~2-char input lag vs the main-thread path; 16ms removes it.
	{
		setTimer: (cb, ms) => self.setTimeout(cb, ms) as unknown as number,
		clearTimer: (id) => self.clearTimeout(id),
		fallbackMs: 16,
	},
);

const state = createRendererState({
	fontEnv,
	// Gated by fonts-ready in the reducer; never called before fonts load.
	onFrame: (buf) => {
		const frame = decodeBinaryFrame(buf);
		// Return the drained buffer to the main thread for pool recycling
		// (ping-pong, zero-copy). Decode has already read it into typed arrays.
		(self as unknown as { postMessage(m: unknown, t: Transferable[]): void }).postMessage(buf, [buf]);
		if (!frame) return;
		applyFrameToGrid(gridState, frame);
		if (timingEnabled && dirtySince === 0) dirtySince = performance.now();
		scheduler.schedule();
	},
});

self.onmessage = (e: MessageEvent<WorkerMessage>) => {
	const msg = e.data;
	if (msg.type === "timing") {
		timingEnabled = msg.enabled;
		dirtySince = 0;
		return;
	}
	if (msg.type === "resize") {
		if (gridRenderer && state.ctx) {
			applyResize(
				{
					ctx: state.ctx,
					gridRenderer,
					setMetrics: (m) => {
						metrics = m;
					},
					setFont: (family, weight) => {
						fontFamily = family;
						fontWeight = weight;
					},
				},
				msg,
			);
			// Theme/font may have changed; drop memoized color/font strings.
			gridRenderer.invalidateCaches();
			scheduler.schedule();
		}
		return;
	}

	reduceRendererMessage(state, msg);
	// Once the OffscreenCanvas context exists (after init), build the renderer.
	if (msg.type === "init" && state.ctx && !gridRenderer) {
		gridRenderer = createGridRenderer(state.ctx, {
			fontWeight: () => fontWeight,
			getFontFamily: () => fontFamily,
		});
	}
};
