// --- Phase 1 render worker entry (DedicatedWorker, type: "module") ---
//
// Thin shell: all logic lives in the pure, unit-tested workerProtocol module.
// Instantiated once per canvas node from CanvasTerminal (gated by the
// offscreenRenderer setting + capability detection in later steps).

import { createRendererState, reduceRendererMessage, type WorkerInboundMessage } from "./workerProtocol";

const state = createRendererState();

self.onmessage = (e: MessageEvent<WorkerInboundMessage>) => {
	reduceRendererMessage(state, e.data);
};
