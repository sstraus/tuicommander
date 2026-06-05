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

/** FontFace descriptors we actually use (subset of the DOM FontFaceDescriptors). */
export interface FontDescriptors {
	weight?: string;
	style?: string;
	unicodeRange?: string;
}

/** One transferable font face: the woff2 bytes + how to register it. */
export interface FontPayload {
	family: string;
	source: ArrayBuffer;
	descriptors?: FontDescriptors;
}

/** Batched font registration. All faces load before painting is unblocked. */
export interface FontsMessage {
	type: "fonts";
	faces: FontPayload[];
}

/** A decoded-grid frame buffer to paint (decode/paint wiring lands in 1.3). */
export interface FrameMessage {
	type: "frame";
	buf: ArrayBuffer;
}

/** Discriminated union of all main->worker messages (grows in later steps). */
export type WorkerInboundMessage = InitMessage | FontsMessage | FrameMessage;

// --- Minimal structural interfaces so tests can inject fakes ---

export interface WorkerLike {
	postMessage(message: unknown, transfer?: Transferable[]): void;
	terminate?(): void;
}

export interface TransferableCanvas {
	transferControlToOffscreen(): OffscreenCanvas;
}

/** Minimal FontFace shape — real `FontFace` satisfies it; tests inject fakes. */
export interface FontFaceLike {
	family: string;
	load(): Promise<unknown>;
}

/**
 * Font environment abstraction. The real worker entry wires this to the global
 * `FontFace` constructor + `self.fonts`; tests inject a fake. Kept out of this
 * module so workerProtocol stays free of Worker/DOM globals.
 */
export interface FontEnv {
	create(family: string, source: ArrayBuffer, descriptors?: FontDescriptors): FontFaceLike;
	add(face: FontFaceLike): void;
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

	/**
	 * Post prefetched fonts to the worker in a single message, transferring each
	 * woff2 ArrayBuffer (zero-copy; neuters the sender's copy).
	 */
	postFonts(payloads: FontPayload[]): void {
		const message: FontsMessage = { type: "fonts", faces: payloads };
		this.worker.postMessage(
			message,
			payloads.map((p) => p.source),
		);
	}
}

// --- Worker side ---

export interface RendererState {
	ctx: OffscreenCanvasRenderingContext2D | null;
	/** True once every font in the last `fonts` message has finished loading. */
	fontsReady: boolean;
	/** In-flight font load promise (resolves after fonts ready + queue flush). */
	fontsLoading: Promise<void> | null;
	/** Frames received before fonts were ready — flushed in order, never dropped. */
	pendingFrames: ArrayBuffer[];
	/** Paint sink; wired to decode+paint in 1.3. Null = no-op (e.g. pre-wiring). */
	onFrame: ((buf: ArrayBuffer) => void) | null;
	fontEnv: FontEnv;
}

export interface RendererDeps {
	fontEnv?: FontEnv;
	onFrame?: (buf: ArrayBuffer) => void;
}

/** A fontEnv that errors if used — the real worker entry must supply one. */
const MISSING_FONT_ENV: FontEnv = {
	create() {
		throw new Error("renderer worker: no fontEnv provided");
	},
	add() {
		throw new Error("renderer worker: no fontEnv provided");
	},
};

export function createRendererState(deps: RendererDeps = {}): RendererState {
	return {
		ctx: null,
		fontsReady: false,
		fontsLoading: null,
		pendingFrames: [],
		onFrame: deps.onFrame ?? null,
		fontEnv: deps.fontEnv ?? MISSING_FONT_ENV,
	};
}

function flushPendingFrames(state: RendererState): void {
	while (state.pendingFrames.length > 0) {
		const buf = state.pendingFrames.shift();
		if (buf) state.onFrame?.(buf);
	}
}

/** Apply one inbound message to the worker's render state. */
export function reduceRendererMessage(state: RendererState, data: WorkerInboundMessage): void {
	switch (data.type) {
		case "init":
			state.ctx = data.canvas.getContext("2d", { alpha: false });
			break;
		case "fonts": {
			const loads = data.faces.map((face) => {
				const ff = state.fontEnv.create(face.family, face.source, face.descriptors);
				state.fontEnv.add(ff);
				return ff.load();
			});
			state.fontsLoading = Promise.all(loads).then(() => {
				state.fontsReady = true;
				flushPendingFrames(state);
			});
			break;
		}
		case "frame":
			// Gate on fonts: paint immediately when ready, else queue (never drop).
			if (state.fontsReady) state.onFrame?.(data.buf);
			else state.pendingFrames.push(data.buf);
			break;
		default:
			// Unknown/future message types are ignored (forward-compatible).
			break;
	}
}
