// --- Phase 1 render-worker protocol (shared by main thread + worker) ---
//
// Pure, no Worker/DOM globals at import time, so both sides import it and it
// stays unit-testable. The risky bit lives here: transferControlToOffscreen()
// is IRREVERSIBLE per canvas node and may only be called once per element — so
// WorkerRenderer.init() guards on node ref-identity.

import type { CellMetrics } from "./canvasTerminalUtils";

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

/**
 * Geometry + DPR + theme + metrics + font. Main does ALL DOM reads
 * (getBoundingClientRect / getComputedStyle) and posts the result; the worker
 * never touches the DOM. `w`/`h` are logical (CSS) pixels; the worker sets the
 * device pixels to `w*dpr` / `h*dpr` and scales accordingly.
 */
export interface ResizeMessage {
	type: "resize";
	w: number;
	h: number;
	dpr: number;
	cols: number;
	rows: number;
	metrics: CellMetrics;
	bgDefault: string;
	fgDefault: string;
	fontFamily: string;
	fontWeight: number | string;
}

/** Discriminated union of all main->worker messages reduced by the worker state. */
export type WorkerInboundMessage = InitMessage | FontsMessage | FrameMessage;

/** All messages the worker entry handles (reducer messages + resize config). */
export type WorkerMessage = WorkerInboundMessage | ResizeMessage;

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
	/** Ping-pong double buffer: keep at most this many drained buffers for reuse. */
	private static readonly MAX_FRAME_POOL = 2;

	private readonly worker: WorkerLike;
	private transferredNode: TransferableCanvas | null = null;
	/** Drained frame buffers returned by the worker, recycled to avoid GC churn. */
	private readonly framePool: ArrayBuffer[] = [];

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

	/** Post geometry/DPR/theme/metrics/font config to the worker (structured clone). */
	postResize(config: Omit<ResizeMessage, "type">): void {
		const message: ResizeMessage = { type: "resize", ...config };
		this.worker.postMessage(message);
	}

	/**
	 * Transfer a frame buffer to the worker (zero-copy; neuters the sender's
	 * reference). The worker drains it on decode and posts it back for recycling.
	 */
	postFrame(buf: ArrayBuffer): void {
		const message: FrameMessage = { type: "frame", buf };
		this.worker.postMessage(message, [buf]);
	}

	/**
	 * Recycle a drained buffer the worker returned. Bounded (ping-pong): once the
	 * pool is full, extra buffers are dropped to GC so it never grows unbounded.
	 */
	recycle(buf: ArrayBuffer): void {
		if (this.framePool.length < WorkerRenderer.MAX_FRAME_POOL) {
			this.framePool.push(buf);
		}
	}

	/**
	 * Get a writable buffer of at least `byteLength`: reuse a pooled one that
	 * fits, else allocate a fresh one. Always returns — the pool never exhausts,
	 * so the main thread is never blocked waiting on the worker to return one.
	 */
	acquire(byteLength: number): ArrayBuffer {
		for (let i = 0; i < this.framePool.length; i++) {
			if (this.framePool[i].byteLength >= byteLength) {
				return this.framePool.splice(i, 1)[0];
			}
		}
		return new ArrayBuffer(byteLength);
	}
}

/**
 * Sacrosanct frame-receipt ordering: ack on the MAIN thread FIRST (the backend
 * ack only clears the in-flight flag; the ticker sends the next frame on its
 * own schedule), THEN transfer the buffer to the worker. Worker paint is never
 * on the ack path, so the 16ms ticker cannot starve.
 */
export function dispatchFrameToWorker(buf: ArrayBuffer, renderer: WorkerRenderer, ack: () => void): void {
	ack();
	renderer.postFrame(buf);
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
