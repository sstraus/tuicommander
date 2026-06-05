import { beforeEach, describe, expect, it, vi } from "vitest";

import {
	createRendererState,
	reduceRendererMessage,
	type TransferableCanvas,
	WorkerRenderer,
	type WorkerLike,
} from "../workerProtocol";

// --- Fakes ---

/** Fake canvas node that mimics transferControlToOffscreen's once-per-node throw. */
function makeFakeCanvas() {
	const off = { __offscreen: Symbol("off") } as unknown as OffscreenCanvas;
	let transfers = 0;
	const canvas: TransferableCanvas & { transfers: () => number; off: OffscreenCanvas } = {
		transferControlToOffscreen() {
			transfers++;
			if (transfers > 1) {
				throw new DOMException("already transferred", "InvalidStateError");
			}
			return off;
		},
		transfers: () => transfers,
		off,
	};
	return canvas;
}

function makeFakeWorker() {
	const posts: Array<{ msg: unknown; transfer?: Transferable[] }> = [];
	const worker: WorkerLike & { posts: typeof posts } = {
		postMessage(msg: unknown, transfer?: Transferable[]) {
			posts.push({ msg, transfer });
		},
		posts,
	};
	return worker;
}

describe("WorkerRenderer init handshake + once-per-node transfer guard", () => {
	it("transfers the canvas and posts a single init message carrying the transferable", () => {
		const worker = makeFakeWorker();
		const canvas = makeFakeCanvas();
		const r = new WorkerRenderer(worker);

		const did = r.init(canvas);

		expect(did).toBe(true);
		expect(canvas.transfers()).toBe(1);
		expect(worker.posts).toHaveLength(1);
		const { msg, transfer } = worker.posts[0];
		expect(msg).toEqual({ type: "init", canvas: canvas.off });
		// The OffscreenCanvas MUST be in the transfer list (zero-copy / neuter).
		expect(transfer).toEqual([canvas.off]);
	});

	it("is a no-op when init is called twice on the SAME node (ref-identity guard)", () => {
		const worker = makeFakeWorker();
		const canvas = makeFakeCanvas();
		const r = new WorkerRenderer(worker);

		expect(r.init(canvas)).toBe(true);
		// Second call on the same node must NOT call transferControlToOffscreen again
		// (which would throw) and must NOT post a second init.
		expect(r.init(canvas)).toBe(false);
		expect(canvas.transfers()).toBe(1);
		expect(worker.posts).toHaveLength(1);
	});

	it("transfers afresh for a DIFFERENT node (detach/reattach remount)", () => {
		const worker = makeFakeWorker();
		const r = new WorkerRenderer(worker);
		const a = makeFakeCanvas();
		const b = makeFakeCanvas();

		expect(r.init(a)).toBe(true);
		expect(r.init(b)).toBe(true);
		expect(a.transfers()).toBe(1);
		expect(b.transfers()).toBe(1);
		expect(worker.posts).toHaveLength(2);
		expect(worker.posts[1].msg).toEqual({ type: "init", canvas: b.off });
		expect(worker.posts[1].transfer).toEqual([b.off]);
	});
});

describe("worker-side init reducer", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
	});

	it("acquires an alpha:false 2d context from the transferred OffscreenCanvas on init", () => {
		const fakeCtx = { __ctx: true } as unknown as OffscreenCanvasRenderingContext2D;
		const getContext = vi.fn().mockReturnValue(fakeCtx);
		const off = { getContext } as unknown as OffscreenCanvas;

		const state = createRendererState();
		expect(state.ctx).toBeNull();

		reduceRendererMessage(state, { type: "init", canvas: off });

		expect(getContext).toHaveBeenCalledWith("2d", { alpha: false });
		expect(state.ctx).toBe(fakeCtx);
	});

	it("ignores unknown message types without throwing", () => {
		const state = createRendererState();
		expect(() => reduceRendererMessage(state, { type: "bogus" } as never)).not.toThrow();
		expect(state.ctx).toBeNull();
	});
});
