// --- Phase 1 render-worker protocol (shared by main thread + worker) ---
//
// Pure, no Worker/DOM globals at import time, so both sides import it and it
// stays unit-testable. The risky bit lives here: transferControlToOffscreen()
// is IRREVERSIBLE per canvas node and may only be called once per element — so
// WorkerRenderer.init() guards on node ref-identity.

// --- Message types (main -> worker) ---

export interface InitMessage {
	type: "init";
	canvas: OffscreenCanvas;
}

/** Discriminated union of all main->worker messages (grows in later steps). */
export type WorkerInboundMessage = InitMessage;

// --- Minimal structural interfaces so tests can inject fakes ---

export interface WorkerLike {
	postMessage(message: unknown, transfer?: Transferable[]): void;
	terminate?(): void;
}

export interface TransferableCanvas {
	transferControlToOffscreen(): OffscreenCanvas;
}

// --- Main-thread side ---

/**
 * Owns one render worker and the single OffscreenCanvas transfer for a canvas
 * node. `transferControlToOffscreen()` can be called at most once per element
 * (irreversible), so init() is idempotent per node via ref-identity:
 *
 *   - same node twice  -> no-op (returns false), never double-transfers/throws
 *   - a different node  -> transfers afresh (detach/reattach remounts a new
 *     <canvas>, which is a distinct element)
 */
export class WorkerRenderer {
	private readonly worker: WorkerLike;
	private transferredNode: TransferableCanvas | null = null;

	constructor(worker: WorkerLike) {
		this.worker = worker;
	}

	/**
	 * Transfer the canvas to the worker and post the init handshake.
	 * @returns true if a transfer happened, false if this node was already transferred.
	 */
	init(canvas: TransferableCanvas): boolean {
		if (this.transferredNode === canvas) return false;
		const offscreen = canvas.transferControlToOffscreen();
		const message: InitMessage = { type: "init", canvas: offscreen };
		this.worker.postMessage(message, [offscreen]);
		this.transferredNode = canvas;
		return true;
	}
}

// --- Worker side ---

export interface RendererState {
	ctx: OffscreenCanvasRenderingContext2D | null;
}

export function createRendererState(): RendererState {
	return { ctx: null };
}

/** Apply one inbound message to the worker's render state. */
export function reduceRendererMessage(state: RendererState, data: WorkerInboundMessage): void {
	switch (data.type) {
		case "init":
			state.ctx = data.canvas.getContext("2d", { alpha: false });
			break;
		default:
			// Unknown/future message types are ignored (forward-compatible).
			break;
	}
}
