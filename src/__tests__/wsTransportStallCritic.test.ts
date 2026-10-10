import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WsTransport } from "../components/Terminal/canvasTerminalTransport";

class FakeWs {
	static all: FakeWs[] = [];
	binaryType = "blob";
	protocol = "";
	onopen: (() => void) | null = null;
	onclose: (() => void) | null = null;
	onerror: (() => void) | null = null;
	onmessage: ((e: { data: unknown }) => void) | null = null;
	closeCalls = 0;
	/** A half-open TCP link: close() never completes the closing handshake. */
	static closeFiresOnclose = true;
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
	frame() {
		this.onmessage?.({ data: new ArrayBuffer(8) });
	}
	text(obj: object) {
		this.onmessage?.({ data: JSON.stringify(obj) });
	}
}

const transports: WsTransport[] = [];

function setup() {
	const errors: unknown[] = [];
	const t = new WsTransport("sess");
	transports.push(t);
	void t.onEvent("stream-error", (e) => errors.push(e));
	return { t, errors };
}

beforeEach(() => {
	vi.useFakeTimers();
	FakeWs.all = [];
	FakeWs.closeFiresOnclose = true;
	vi.stubGlobal("WebSocket", FakeWs);
});
afterEach(async () => {
	// The fixture owns sockets deliberately left connecting by reconnect assertions.
	for (const ws of FakeWs.all) ws.open();
	for (const t of transports.splice(0)) t.unsubscribe();
	await Promise.resolve();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe("WsTransport stall handling (critic 1421-733e)", () => {
	it("keeps reconnecting after a stalled replay even when the dead socket never fires onclose — catches: reconnect gated solely on the stalled socket's close event", async () => {
		FakeWs.closeFiresOnclose = false;
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000);
		expect(errors).toHaveLength(1);
		// Backoff is 1s for the first retry; allow generous slack.
		await vi.advanceTimersByTimeAsync(5_000);
		expect(FakeWs.all.length).toBeGreaterThan(1);
	});

	it("a text-only partial replay does not count as the initial frame — catches: timer cleared by a cwd/event text frame", async () => {
		const { t, errors } = setup();
		void t.onEvent("cwd", () => {});
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		FakeWs.all[0].text({ type: "cwd", cwd: "/x" });
		await vi.advanceTimersByTimeAsync(15_000);
		expect(errors).toHaveLength(1);
	});

	it("a second failure after a recovered stream is reported again — catches: failureReported never reset after a delivered frame", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(15_000); // stall -> error 1, reconnect
		await vi.advanceTimersByTimeAsync(2_000);
		const second = FakeWs.all.at(-1)!;
		second.open();
		second.frame(); // recovered
		second.onclose?.(); // drops again
		expect(errors).toHaveLength(2);
	});

	it("stops opening sockets after the reconnect cap with only one toast event — catches: unbounded reconnect or one toast per attempt", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		for (let i = 0; i < 25; i++) {
			await vi.advanceTimersByTimeAsync(60_000);
			const last = FakeWs.all.at(-1)!;
			// Each reconnect socket opens and then stalls without a frame.
			last.open();
		}
		expect(errors).toHaveLength(1);
		// 1 initial + at most 10 retries.
		expect(FakeWs.all.length).toBeLessThanOrEqual(11);
	});

	it("unsubscribe while the initial-frame timer is armed reports nothing and opens nothing — catches: timer firing after teardown", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		t.unsubscribe();
		await vi.advanceTimersByTimeAsync(120_000);
		expect(errors).toHaveLength(0);
		expect(FakeWs.all).toHaveLength(1);
	});

	it("resubscribe while the old socket's timer is armed does not let the old timer kill the new socket — catches: stale timer closing the replacement", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		FakeWs.all[0].open();
		await sub;
		await vi.advanceTimersByTimeAsync(10_000);
		const re = t.resubscribe();
		const fresh = FakeWs.all.at(-1)!;
		fresh.open();
		await re;
		fresh.frame();
		await vi.advanceTimersByTimeAsync(30_000);
		expect(errors).toHaveLength(0);
		expect(fresh.closeCalls).toBe(0);
	});

	it("a throwing frame consumer is reported once and does not escape the socket handler — catches: renderer exception leaking as an unhandled error / silent stall", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {
			throw new Error("render boom");
		});
		const ws = FakeWs.all[0];
		ws.open();
		await sub;
		expect(() => ws.frame()).not.toThrow();
		expect(errors).toHaveLength(1);
	});

	it("a socket error before open rejects subscribe and reports exactly once — catches: double toast from onerror + onclose", async () => {
		const { t, errors } = setup();
		const sub = t.subscribe(() => {});
		const ws = FakeWs.all[0];
		ws.onerror?.();
		ws.onclose?.();
		await expect(sub).rejects.toThrow();
		expect(errors).toHaveLength(1);
	});
});
