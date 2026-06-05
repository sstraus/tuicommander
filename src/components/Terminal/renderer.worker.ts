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
import {
	applyFrameToGrid,
	applyResize,
	createRepaintScheduler,
	createWorkerGridState,
} from "./workerGridState";
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

const scheduler = createRepaintScheduler(
	(cb) => self.requestAnimationFrame(cb),
	(id) => self.cancelAnimationFrame(id),
	() => {
		if (!gridRenderer || !metrics) return;
		const dirty = gridState.pendingDirtyRows.size > 0 ? new Set(gridState.pendingDirtyRows) : undefined;
		gridRenderer.paintGrid(gridState.rowMap, metrics, {
			fullRepaint: gridState.fullRepaintNeeded,
			dirtyIndices: dirty,
		});
		gridState.fullRepaintNeeded = false;
		gridState.pendingDirtyRows.clear();
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
		scheduler.schedule();
	},
});

self.onmessage = (e: MessageEvent<WorkerMessage>) => {
	const msg = e.data;
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
