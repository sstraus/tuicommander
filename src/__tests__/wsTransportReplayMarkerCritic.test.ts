import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WsTransport } from "../components/Terminal/canvasTerminalTransport";
import { DEFLATE_SUBPROTOCOL, FRAME_TAG } from "../components/Terminal/wsFrameCodec";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup } from "../transportRuntime";

class FakeWs {
	static all: FakeWs[] = [];
	static closeFiresOnclose = true;
	binaryType = "blob";
	protocol = "";
	onopen: (() => void) | null = null;
	onclose: (() => void) | null = null;
	onerror: (() => void) | null = null;
	onmessage: ((e: { data: unknown }) => void) | null = null;
	closeCalls = 0;
	constructor(
		public url: string,
		_protocols?: string | string[],
	) {
		FakeWs.all.push(this);
	}
	close() {
		this.closeCalls++;
		if (FakeWs.closeFiresOnclose) queueMicrotask(() => this.onclose?.());
	}
	open() {
		this.onopen?.();
	}
	send(data: unknown) {
		this.onmessage?.({ data });
	}
	frame() {
		this.send(new ArrayBuffer(8));
	}
	marker() {
		this.send(JSON.stringify({ type: "grid-replay-empty" }));
	}
	taggedMarker() {
		const body = new TextEncoder().encode(JSON.stringify({ type: "grid-replay-empty" }));
		const out = new Uint8Array(body.length + 1);
		out[0] = FRAME_TAG.text;
		out.set(body, 1);
		this.send(out.buffer);
	}
}

const transports: WsTransport[] = [];

function setup(connectionId?: string) {
	const errors: unknown[] = [];
	const t = new WsTransport("sess", connectionId);
	transports.push(t);
	t.onStreamError((e) => errors.push(e));
	return { t, errors };
}

beforeEach(() => {
	vi.useFakeTimers();
	FakeWs.all = [];
	FakeWs.closeFiresOnclose = true;
	vi.stubGlobal("WebSocket", FakeWs);
	setRemoteBaseUrlLookup((id) => (id === "conn" ? "http://remote.test:9876" : undefined));
	setRemoteTokenLookup((id) => (id === "conn" ? "tok" : undefined));
});
afterEach(async () => {
	// The fixture owns sockets deliberately left connecting by reconnect assertions.
	for (const ws of FakeWs.all) ws.open();
	for (const t of transports.splice(0)) t.unsubscribe();
	await Promise.resolve();
	vi.useRealTimers();
	vi.unstubAllGlobals();
	setRemoteBaseUrlLookup(() => undefined);
	setRemoteTokenLookup(() => undefined);
});

describe("WsTransport empty-replay marker (critic 1421-733e round 2)", () => {
	it("a tagged marker on a negotiated socket finishes the replay — catches: marker only honoured on the plain-framing path", async () => {
		const { t, errors } = setup("conn");
		const sub = t.subscribe(() => {});
		FakeWs.all[0].protocol = DEFLATE_SUBPROTOCOL;
		FakeWs.all[0].open();
		await sub;
		FakeWs.all[0].taggedMarker();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(errors).toHaveLength(0);
		expect(FakeWs.all).toHaveLength(1);
		expect(FakeWs.all[0].closeCalls).toBe(0);
	});

	it("a late marker from the detached socket does not disarm the replacement's timer — catches: marker handled without the stale-socket guard", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000);
		await vi.advanceTimersByTimeAsync(1_500);
		expect(FakeWs.all).toHaveLength(2);
		FakeWs.all[1].open();
		FakeWs.all[0].marker();
		await vi.advanceTimersByTimeAsync(15_000);
		expect(FakeWs.all[1].closeCalls).toBe(1);
	});

	it("a late binary frame from the detached socket never reaches the renderer — catches: reconnect not detaching the old socket before close completes", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t } = setup();
		const frames: ArrayBuffer[] = [];
		const sub = t.subscribe((d) => frames.push(d));
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000);
		FakeWs.all[0].frame();
		expect(frames).toHaveLength(0);
	});

	it("the marker restores the reconnect backoff to its first step — catches: marker clears the timer but leaves reconnectAttempts raised", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		// Three stalls in a row: attempts climb to 3 (delays 1s, 2s, 4s).
		for (let i = 0; i < 3; i++) {
			await vi.advanceTimersByTimeAsync(15_000);
			await vi.advanceTimersByTimeAsync(10_000);
			FakeWs.all.at(-1)!.open();
		}
		const live = FakeWs.all.at(-1)!;
		const before = FakeWs.all.length;
		live.marker();
		live.onclose?.();
		await vi.advanceTimersByTimeAsync(1_100);
		expect(FakeWs.all.length).toBe(before + 1);
	});

	it("a consumer that always throws stops reconnecting at the cap and leaves no timers — catches: attempts reset before the renderer accepted the frame", async () => {
		const { t } = setup();
		const sub = t.subscribe(() => {
			throw new Error("render failed");
		});
		FakeWs.all[0].open();
		await sub;
		for (let i = 0; i < 30; i++) {
			const ws = FakeWs.all.at(-1)!;
			ws.open();
			ws.frame();
			await vi.advanceTimersByTimeAsync(40_000);
		}
		expect(FakeWs.all.length).toBeLessThanOrEqual(11);
		expect(vi.getTimerCount()).toBe(0);
	});

	it("an undecodable negotiated frame reconnects even when the dead socket never closes — catches: decode failure recovery gated on onclose", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t, errors } = setup("conn");
		const sub = t.subscribe(() => {});
		FakeWs.all[0].protocol = DEFLATE_SUBPROTOCOL;
		FakeWs.all[0].open();
		await sub;
		FakeWs.all[0].send(new Uint8Array([0x7f, 1, 2, 3]).buffer);
		await vi.advanceTimersByTimeAsync(2_000);
		expect(errors).toHaveLength(1);
		expect(FakeWs.all.length).toBe(2);
	});

	it("unsubscribe right after a replay timeout cancels the pending direct reconnect — catches: reconnect timer scheduled by the timeout path survives teardown", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000);
		t.unsubscribe();
		await vi.advanceTimersByTimeAsync(120_000);
		expect(FakeWs.all).toHaveLength(1);
		expect(vi.getTimerCount()).toBe(0);
	});

	it("resubscribe during the pending direct reconnect opens exactly one socket — catches: old reconnect timer fires alongside the resubscribe socket", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000);
		const re = t.resubscribe();
		FakeWs.all[1].open();
		await re;
		await vi.advanceTimersByTimeAsync(5_000);
		expect(FakeWs.all).toHaveLength(2);
	});
});
