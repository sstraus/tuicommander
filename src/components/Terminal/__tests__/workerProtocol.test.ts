import { beforeEach, describe, expect, it, vi } from "vitest";

import {
	chooseRenderer,
	createRendererState,
	type FontEnv,
	type FontFaceLike,
	type FontPayload,
	receiveFrame,
	reduceRendererMessage,
	type TransferableCanvas,
	type WorkerLike,
	WorkerRenderer,
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

// --- Frame transfer (ping-pong) + ack-before-transfer ---

describe("WorkerRenderer.postFrame + buffer pool (ping-pong)", () => {
	it("transfers the frame buffer (neuters sender) as a frame message", () => {
		const worker = makeFakeWorker();
		const r = new WorkerRenderer(worker);
		const buf = new ArrayBuffer(64);

		r.postFrame(buf);

		expect(worker.posts).toHaveLength(1);
		expect(worker.posts[0].msg).toMatchObject({ type: "frame", buf });
		expect(worker.posts[0].transfer).toEqual([buf]);
	});

	it("recycles a returned buffer and reuses it on acquire (zero re-alloc)", () => {
		const r = new WorkerRenderer(makeFakeWorker());
		const buf = new ArrayBuffer(128);

		r.recycle(buf);
		expect(r.acquire(128)).toBe(buf); // same instance reused
	});

	it("allocates a fresh buffer when the pool is empty (never exhausts)", () => {
		const r = new WorkerRenderer(makeFakeWorker());
		const got = r.acquire(256);
		expect(got).toBeInstanceOf(ArrayBuffer);
		expect(got.byteLength).toBe(256);
	});

	it("does not return an undersized pooled buffer; allocs to fit", () => {
		const r = new WorkerRenderer(makeFakeWorker());
		r.recycle(new ArrayBuffer(8));
		const got = r.acquire(64);
		expect(got.byteLength).toBe(64);
	});

	it("bounds the pool (double-buffer): excess recycled buffers are dropped", () => {
		const r = new WorkerRenderer(makeFakeWorker());
		const a = new ArrayBuffer(16);
		const b = new ArrayBuffer(16);
		const c = new ArrayBuffer(16);
		r.recycle(a);
		r.recycle(b);
		r.recycle(c); // over cap of 2 -> dropped

		const first = r.acquire(16);
		const second = r.acquire(16);
		const third = r.acquire(16); // pool empty -> fresh alloc
		expect([first, second]).toContain(a);
		expect([first, second]).toContain(b);
		expect(third).not.toBe(a);
		expect(third).not.toBe(b);
		expect(third).not.toBe(c);
	});

	it("survives back-to-back frames: pool never exhausts, acquire always returns", () => {
		const r = new WorkerRenderer(makeFakeWorker());
		for (let i = 0; i < 10; i++) {
			const buf = r.acquire(32); // always returns (pooled or fresh)
			expect(buf.byteLength).toBeGreaterThanOrEqual(32);
			r.postFrame(buf); // transfer to worker
			r.recycle(new ArrayBuffer(32)); // worker returns a drained buffer
		}
	});
});

describe("receiveFrame — ack → decode → transfer ordering (Option A: main decodes, worker paints)", () => {
	it("worker mode: acks first, decodes on main, THEN transfers (decode before neuter)", () => {
		const order: string[] = [];
		const buf = new ArrayBuffer(16);
		const frame = receiveFrame(buf, {
			ack: () => order.push("ack"),
			decode: (b) => {
				order.push("decode");
				expect(b).toBe(buf); // decode sees the intact buffer
				return { ok: true };
			},
			transferToWorker: (b) => {
				order.push("transfer");
				expect(b).toBe(buf);
			},
		});

		expect(order).toEqual(["ack", "decode", "transfer"]);
		expect(frame).toEqual({ ok: true });
	});

	it("main mode: acks, decodes, and does NOT transfer (no worker)", () => {
		const order: string[] = [];
		const frame = receiveFrame(new ArrayBuffer(8), {
			ack: () => order.push("ack"),
			decode: () => {
				order.push("decode");
				return 42;
			},
			// transferToWorker omitted (main mode)
		});

		expect(order).toEqual(["ack", "decode"]);
		expect(frame).toBe(42);
	});

	it("returns a null decode result (caller bails on its own bookkeeping)", () => {
		const order: string[] = [];
		const transferred: ArrayBuffer[] = [];
		const buf = new ArrayBuffer(4);
		const frame = receiveFrame<null>(buf, {
			ack: () => order.push("ack"),
			decode: () => null, // malformed frame
			transferToWorker: (b) => transferred.push(b),
		});

		expect(frame).toBeNull();
		// Ack + transfer still happen even when decode yields null (buffer recycled, flag cleared).
		expect(order).toEqual(["ack"]);
		expect(transferred).toEqual([buf]);
	});
});

describe("WorkerRenderer.postResize", () => {
	it("posts a resize message carrying geometry, metrics, theme and font", () => {
		const worker = makeFakeWorker();
		const r = new WorkerRenderer(worker);
		const metrics = {
			cellWidth: 8,
			cellHeight: 16,
			baseline: 12,
			fontSize: 14,
			dpr: 2,
			scaledCellWidth: 16,
			scaledCellHeight: 32,
		};

		r.postResize({
			w: 100,
			h: 50,
			dpr: 2,
			cols: 12,
			rows: 3,
			metrics,
			bgDefault: "#222",
			fgDefault: "#ddd",
			fontFamily: "Hack",
			fontWeight: 400,
		});

		expect(worker.posts).toHaveLength(1);
		expect(worker.posts[0].msg).toMatchObject({
			type: "resize",
			w: 100,
			h: 50,
			dpr: 2,
			bgDefault: "#222",
			fontFamily: "Hack",
		});
		// config is structured-cloned, not transferred
		expect(worker.posts[0].transfer).toBeUndefined();
	});
});

describe("chooseRenderer (capability detection)", () => {
	it("uses the worker only when enabled AND supported", () => {
		expect(chooseRenderer(true, true)).toBe("worker");
	});
	it("falls back to main when the setting is off", () => {
		expect(chooseRenderer(false, true)).toBe("main");
	});
	it("falls back to main when the API is unsupported", () => {
		expect(chooseRenderer(true, false)).toBe("main");
	});
	it("is main when both off", () => {
		expect(chooseRenderer(false, false)).toBe("main");
	});
});

describe("WorkerRenderer.terminate", () => {
	it("terminates the underlying worker", () => {
		const terminate = vi.fn();
		const worker: WorkerLike = { postMessage: () => {}, terminate };
		const r = new WorkerRenderer(worker);
		r.terminate();
		expect(terminate).toHaveBeenCalledTimes(1);
	});

	it("is a no-op when the worker has no terminate()", () => {
		const r = new WorkerRenderer({ postMessage: () => {} });
		expect(() => r.terminate()).not.toThrow();
	});
});
