import { beforeEach, describe, expect, it, vi } from "vitest";

import {
	createRendererState,
	type FontEnv,
	type FontFaceLike,
	type FontPayload,
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

// --- Font loading + ready-gating ---

function makeFakeFontEnv() {
	const created: Array<{ family: string; descriptors?: unknown }> = [];
	const added: FontFaceLike[] = [];
	const env: FontEnv = {
		create(family, _source, descriptors) {
			const face: FontFaceLike = { family, load: () => Promise.resolve(face) };
			created.push({ family, descriptors });
			return face;
		},
		add(face) {
			added.push(face);
		},
	};
	return { env, created, added };
}

function payload(family: string, weight: string): FontPayload {
	return { family, source: new ArrayBuffer(8), descriptors: { weight } };
}

describe("font loading + ready-gating", () => {
	it("queues a frame that arrives before fonts are ready, then flushes after load (not dropped)", async () => {
		const { env } = makeFakeFontEnv();
		const painted: ArrayBuffer[] = [];
		const state = createRendererState({ fontEnv: env, onFrame: (b) => painted.push(b) });

		const frameBuf = new ArrayBuffer(4);
		reduceRendererMessage(state, { type: "frame", buf: frameBuf });
		expect(painted).toHaveLength(0);
		expect(state.pendingFrames).toHaveLength(1);
		expect(state.fontsReady).toBe(false);

		reduceRendererMessage(state, { type: "fonts", faces: [payload("Hack", "400")] });
		await state.fontsLoading;

		expect(state.fontsReady).toBe(true);
		expect(painted).toEqual([frameBuf]);
		expect(state.pendingFrames).toHaveLength(0);
	});

	it("creates and adds a FontFace per payload on a fonts message", async () => {
		const { env, created, added } = makeFakeFontEnv();
		const state = createRendererState({ fontEnv: env });

		reduceRendererMessage(state, {
			type: "fonts",
			faces: [payload("Hack", "400"), payload("Symbols Nerd Font Mono", "400")],
		});
		await state.fontsLoading;

		expect(created.map((c) => c.family)).toEqual(["Hack", "Symbols Nerd Font Mono"]);
		expect(created[0].descriptors).toEqual({ weight: "400" });
		expect(added).toHaveLength(2);
	});

	it("paints a frame immediately once fonts are ready", async () => {
		const { env } = makeFakeFontEnv();
		const painted: ArrayBuffer[] = [];
		const state = createRendererState({ fontEnv: env, onFrame: (b) => painted.push(b) });

		reduceRendererMessage(state, { type: "fonts", faces: [payload("Hack", "400")] });
		await state.fontsLoading;

		const b = new ArrayBuffer(4);
		reduceRendererMessage(state, { type: "frame", buf: b });
		expect(painted).toEqual([b]);
		expect(state.pendingFrames).toHaveLength(0);
	});

	it("preserves frame order when flushing the queue", async () => {
		const { env } = makeFakeFontEnv();
		const painted: ArrayBuffer[] = [];
		const state = createRendererState({ fontEnv: env, onFrame: (b) => painted.push(b) });

		const f1 = new ArrayBuffer(1);
		const f2 = new ArrayBuffer(2);
		reduceRendererMessage(state, { type: "frame", buf: f1 });
		reduceRendererMessage(state, { type: "frame", buf: f2 });
		reduceRendererMessage(state, { type: "fonts", faces: [payload("Hack", "400")] });
		await state.fontsLoading;

		expect(painted).toEqual([f1, f2]);
	});

	it("WorkerRenderer.postFonts posts one fonts message with sources as transferables", () => {
		const worker = makeFakeWorker();
		const r = new WorkerRenderer(worker);
		const s1 = new ArrayBuffer(8);
		const s2 = new ArrayBuffer(8);

		r.postFonts([
			{ family: "Hack", source: s1, descriptors: { weight: "400" } },
			{ family: "Hack", source: s2, descriptors: { weight: "700" } },
		]);

		expect(worker.posts).toHaveLength(1);
		expect(worker.posts[0].msg).toMatchObject({ type: "fonts" });
		expect(worker.posts[0].transfer).toEqual([s1, s2]);
	});
});
