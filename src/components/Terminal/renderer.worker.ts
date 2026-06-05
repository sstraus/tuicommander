// --- Phase 1 render worker entry (DedicatedWorker, type: "module") ---
//
// Thin shell: all logic lives in the pure, unit-tested workerProtocol module.
// Instantiated once per canvas node from CanvasTerminal (gated by the
// offscreenRenderer setting + capability detection in later steps).

import {
	createRendererState,
	type FontDescriptors,
	type FontEnv,
	type FontFaceLike,
	reduceRendererMessage,
	type WorkerInboundMessage,
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

const state = createRendererState({ fontEnv });

self.onmessage = (e: MessageEvent<WorkerInboundMessage>) => {
	reduceRendererMessage(state, e.data);
};
