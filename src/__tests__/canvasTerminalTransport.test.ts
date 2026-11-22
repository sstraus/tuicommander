import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock @tauri-apps/api/core
vi.mock("@tauri-apps/api/core", () => {
	const mockChannel = class {
		onmessage: ((data: unknown) => void) | null = null;
		id = 1;
	};
	return {
		invoke: vi.fn().mockResolvedValue(undefined),
		Channel: mockChannel,
	};
});

// Mock @tauri-apps/api/event
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock transport for isTauri
vi.mock("../transport", () => ({
	isTauri: vi.fn().mockReturnValue(true),
	rpc: vi.fn().mockResolvedValue(undefined),
}));

import {
	createTransport,
	TauriTransport,
	toBinaryPayload,
	WsTransport,
} from "../components/Terminal/canvasTerminalTransport";
import { DEFLATE_SUBPROTOCOL, FRAME_TAG } from "../components/Terminal/wsFrameCodec";
import { isTauri } from "../transport";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup } from "../transportRuntime";

/**
 * Build the bytes `mcp_http::ws_compression` puts on a negotiated socket: one
 * tag byte, then the payload, deflated when the tag says so.
 *
 * Deflated with the platform's own `CompressionStream`, not with a hand-rolled
 * fixture — a frame this test invented could be one the server would never
 * send, and the decoder would then be proved against nothing.
 */
async function taggedFrame(tag: number, payload: Uint8Array): Promise<ArrayBuffer> {
	const deflated = tag === FRAME_TAG.binaryDeflate || tag === FRAME_TAG.textDeflate;
	const body = deflated
		? new Uint8Array(
				await new Response(
					new Blob([payload as BlobPart]).stream().pipeThrough(new CompressionStream("deflate-raw")),
				).arrayBuffer(),
			)
		: payload;
	const frame = new Uint8Array(body.length + 1);
	frame[0] = tag;
	frame.set(body, 1);
	return frame.buffer;
}

describe("canvasTerminalTransport", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe("createTransport", () => {
		it("returns TauriTransport when isTauri() is true", () => {
			(isTauri as ReturnType<typeof vi.fn>).mockReturnValue(true);
			const t = createTransport("session-1");
			expect(t).toBeInstanceOf(TauriTransport);
		});

		it("returns WsTransport when isTauri() is false", () => {
			(isTauri as ReturnType<typeof vi.fn>).mockReturnValue(false);
			const t = createTransport("session-1");
			expect(t).toBeInstanceOf(WsTransport);
		});
	});

	// Every binary payload the backend sends — grid frames on the channel, styled
	// row chunks from a command — arrives as an ArrayBuffer over the custom-protocol
	// IPC and as a plain number[] over the postMessage fallback Tauri drops to when
	// the custom protocol is blocked. One normalizer covers both callers.
	describe("toBinaryPayload", () => {
		it("passes an ArrayBuffer through untouched", () => {
			const buffer = new Uint8Array([1, 2, 3]).buffer;
			expect(toBinaryPayload(buffer)).toBe(buffer);
		});

		it("packs the postMessage number[] fallback into a buffer", () => {
			const result = toBinaryPayload([26, 0, 255]);
			expect(result).not.toBeNull();
			expect([...new Uint8Array(result!)]).toEqual([26, 0, 255]);
		});

		it("unwraps a Uint8Array view without copying its bytes", () => {
			const view = new Uint8Array([7, 8]);
			expect([...new Uint8Array(toBinaryPayload(view)!)]).toEqual([7, 8]);
		});

		it("rejects a shape that is not binary at all", () => {
			// A command that returns an error object or null must not reach the
			// decoder — `new Uint8Array({})` silently yields an empty buffer, which
			// would read as "an empty chunk" instead of "a broken response".
			expect(toBinaryPayload(undefined)).toBeNull();
			expect(toBinaryPayload(null)).toBeNull();
			expect(toBinaryPayload({ error: "nope" })).toBeNull();
			expect(toBinaryPayload("[1,2,3]")).toBeNull();
		});

		it("keeps an empty payload distinguishable from a broken one", () => {
			// A closed session answers with zero bytes; that is a valid empty chunk.
			expect(toBinaryPayload([])?.byteLength).toBe(0);
		});
	});

	describe("TauriTransport", () => {
		/**
		 * Make `subscribe_terminal_grid` answer with a subscription epoch, the way
		 * the real command does. Every other command keeps resolving undefined.
		 */
		async function mockEpochs(...epochs: number[]): Promise<void> {
			const { invoke } = await import("@tauri-apps/api/core");
			let next = 0;
			(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
				Promise.resolve(cmd === "subscribe_terminal_grid" ? epochs[Math.min(next++, epochs.length - 1)] : undefined),
			);
		}

		it("subscribes to terminal grid channel via invoke", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			const transport = new TauriTransport("session-1");
			const onFrame = vi.fn();
			await transport.subscribe(onFrame);

			expect(invoke).toHaveBeenCalledWith(
				"subscribe_terminal_grid",
				expect.objectContaining({
					sessionId: "session-1",
				}),
			);
		});

		it("requests initial frame after a visible subscribe", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			const transport = new TauriTransport("session-1");
			transport.setVisible(true);
			await transport.subscribe(vi.fn());

			expect(invoke).toHaveBeenCalledWith("terminal_request_frame", { sessionId: "session-1" });
		});

		// Catches hidden terminals requesting frames before their visibility is known.
		it("subscribes hidden without requesting grid bytes and publishes epoch-scoped show", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42);
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());
			expect(invoke).toHaveBeenCalledWith("subscribe_terminal_grid", expect.objectContaining({ visible: false }));
			expect(invoke).not.toHaveBeenCalledWith("terminal_request_frame", expect.anything());
			transport.setVisible(true);
			await Promise.resolve();
			expect(invoke).toHaveBeenLastCalledWith("ack_terminal_frame", {
				sessionId: "session-1",
				epoch: 42,
				received: 0,
				visible: true,
			});
		});

		// Catches rapid tab hide/show leaving Rust hidden when IPC completes out of order.
		it("keeps the newest visibility after a delayed hide completes", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42);
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());
			let backendVisible = false;
			let finishHide: (() => void) | undefined;
			(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
				if (cmd !== "ack_terminal_frame" || typeof args?.visible !== "boolean") return Promise.resolve();
				if (args.visible) {
					backendVisible = true;
					return Promise.resolve();
				}
				return new Promise<void>((resolve) => {
					finishHide = () => {
						backendVisible = false;
						resolve();
					};
				});
			});
			transport.setVisible(false);
			transport.setVisible(true);
			await Promise.resolve();
			finishHide?.();
			await vi.waitFor(() => {
				expect(backendVisible, "the visible tab must not remain excluded from grid delivery").toBe(true);
			});
		});

		it("delegates invoke calls to Tauri invoke", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			(invoke as ReturnType<typeof vi.fn>).mockResolvedValue("result");
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());

			const result = await transport.invoke("terminal_scroll", { sessionId: "session-1", delta: 5 });
			expect(invoke).toHaveBeenCalledWith("terminal_scroll", { sessionId: "session-1", delta: 5 });
			expect(result).toBe("result");
		});

		it("acks a frame with the receipt count the gate compares against", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42);
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());

			transport.ackFrame(7);
			expect(invoke).toHaveBeenCalledWith("ack_terminal_frame", {
				sessionId: "session-1",
				epoch: 42,
				received: 7,
			});
		});

		// A remount subscribes before the outgoing instance tears down, so the
		// backend sees the old instance's calls after the new gate is installed.
		// The epoch is what tells them apart: without it a late ack credits frames
		// the new terminal never received, and a late unsubscribe deletes the live
		// channel and leaves a mounted terminal blank.
		it("adopts the epoch of its newest subscription", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42, 43);
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());
			await transport.resubscribe();

			transport.ackFrame(3);
			expect(invoke).toHaveBeenCalledWith("ack_terminal_frame", {
				sessionId: "session-1",
				epoch: 43,
				received: 3,
			});
		});

		it("does not ack before it knows its epoch", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42);
			const transport = new TauriTransport("session-1");

			// A frame cannot arrive before subscribe resolves, but a paint-driven ack
			// racing the very first subscribe must not invent an epoch — epoch 0 would
			// silently match no gate at all, wedging the delivery gate shut forever.
			transport.ackFrame(1);
			expect(invoke).not.toHaveBeenCalledWith("ack_terminal_frame", expect.anything());
		});

		it("registers event listeners via Tauri listen", async () => {
			const { listen } = await import("@tauri-apps/api/event");
			const transport = new TauriTransport("session-1");
			const handler = vi.fn();
			await transport.subscribe(vi.fn());
			await transport.onEvent("cwd", handler);

			expect(listen).toHaveBeenCalledWith("pty-cwd-session-1", expect.any(Function));
		});

		it("calls unsubscribe_terminal_grid with the epoch it owns", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			await mockEpochs(42);
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());
			transport.unsubscribe();

			expect(invoke).toHaveBeenCalledWith("unsubscribe_terminal_grid", { sessionId: "session-1", epoch: 42 });
		});

		// The desktop backend keeps one grid channel per session: a second
		// subscription replaces the first. A split pane mounts a second Terminal for
		// the same session, so the copy left behind must learn it lost the channel.
		describe("lostGrid", () => {
			// catches: a replaced instance that still believes it receives frames, so
			// the terminal left on screen after a split closes stays frozen.
			it("reports the older instance once a newer one subscribes the same session", async () => {
				await mockEpochs(1, 2);
				const flat = new TauriTransport("grid-a");
				const pane = new TauriTransport("grid-a");
				await flat.subscribe(vi.fn());
				expect(flat.lostGrid()).toBe(false);
				await pane.subscribe(vi.fn());
				expect(flat.lostGrid()).toBe(true);
				expect(pane.lostGrid()).toBe(false);
			});

			// catches: resubscribing that does not take the channel back, so the
			// reclaim on show would repeat forever without restoring frames.
			it("clears after the replaced instance resubscribes", async () => {
				await mockEpochs(1, 2, 3);
				const flat = new TauriTransport("grid-b");
				const pane = new TauriTransport("grid-b");
				await flat.subscribe(vi.fn());
				await pane.subscribe(vi.fn());
				await flat.resubscribe();
				expect(flat.lostGrid()).toBe(false);
				expect(pane.lostGrid()).toBe(true);
			});

			// catches: the closing pane's teardown clearing ownership it no longer
			// has, which would hide the loss from the instance that must reclaim.
			it("keeps the loss visible when the newer instance unmounts", async () => {
				await mockEpochs(1, 2);
				const flat = new TauriTransport("grid-c");
				const pane = new TauriTransport("grid-c");
				await flat.subscribe(vi.fn());
				await pane.subscribe(vi.fn());
				pane.unsubscribe();
				expect(flat.lostGrid()).toBe(true);
			});

			// catches: a reclaim triggered by a subscription still in flight or by
			// another session, which would wipe and repaint a healthy terminal.
			it("is false before the first subscription and across sessions", async () => {
				await mockEpochs(1, 2);
				const fresh = new TauriTransport("grid-d");
				expect(fresh.lostGrid()).toBe(false);
				const other = new TauriTransport("grid-e");
				await fresh.subscribe(vi.fn());
				await other.subscribe(vi.fn());
				expect(fresh.lostGrid()).toBe(false);
			});

			// catches: an unsubscribed instance reported as having lost the channel.
			it("is false after the instance unsubscribes itself", async () => {
				await mockEpochs(1, 2);
				const a = new TauriTransport("grid-f");
				const b = new TauriTransport("grid-f");
				await a.subscribe(vi.fn());
				await b.subscribe(vi.fn());
				a.unsubscribe();
				expect(a.lostGrid()).toBe(false);
			});
		});

		it("does not unsubscribe a subscription it never made", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			const transport = new TauriTransport("session-1");
			transport.unsubscribe();

			expect(invoke).not.toHaveBeenCalledWith("unsubscribe_terminal_grid", expect.anything());
		});
	});

	describe("WsTransport", () => {
		let wsInstances: MockWebSocket[];

		/**
		 * A server's answer to the subprotocol offer, for the next socket opened.
		 *
		 * `"accept"` is a server that tags its frames, `"ignore"` one that predates
		 * the negotiation and sends the original framing. Nothing else is
		 * reachable: a browser fails a socket whose server names a subprotocol the
		 * client never offered.
		 */
		let serverAnswer: "accept" | "ignore" = "accept";

		class MockWebSocket {
			static lastUrl = "";
			static lastProtocols: string[] | undefined;
			binaryType = "";
			protocol = "";
			onmessage: ((e: { data: unknown }) => void) | null = null;
			onclose: (() => void) | null = null;
			onopen: (() => void) | null = null;
			onerror: ((e: unknown) => void) | null = null;
			close = vi.fn();
			constructor(url: string, protocols?: string[]) {
				MockWebSocket.lastUrl = url;
				MockWebSocket.lastProtocols = protocols;
				if (serverAnswer === "accept") this.protocol = protocols?.[0] ?? "";
				wsInstances.push(this);
			}
		}

		beforeEach(() => {
			vi.useFakeTimers();
			wsInstances = [];
			serverAnswer = "accept";
			MockWebSocket.lastProtocols = undefined;
			(globalThis as Record<string, unknown>).WebSocket = MockWebSocket as unknown as typeof WebSocket;
		});

		afterEach(() => {
			vi.useRealTimers();
		});

		it("connects to /sessions/{id}/stream?format=grid", async () => {
			const transport = new WsTransport("sess-42");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			expect(MockWebSocket.lastUrl).toContain("/sessions/sess-42/stream?format=grid");
			expect(wsInstances[0].binaryType).toBe("arraybuffer");
			// A local terminal has no link to save, so it neither asks nor offers —
			// and the server has nothing to acknowledge.
			expect(MockWebSocket.lastProtocols).toBeUndefined();
		});

		// `ack_terminal_frame` is desktop-only (INTENTIONALLY_UNMAPPED): calling it
		// over HTTP throws "native/host-only" — once per frame, 30-60 times a second,
		// for a browser client that recovers from dropped frames by sequence number
		// instead. The ack belongs to the transport that has one.
		it("does not ack over HTTP", async () => {
			const { rpc } = await import("../transport");
			const transport = new WsTransport("sess-42");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;
			(rpc as ReturnType<typeof vi.fn>).mockClear();

			transport.ackFrame(7);
			expect(rpc).not.toHaveBeenCalled();
		});

		// resubscribe() closes the old socket while the transport is deliberately NOT
		// in the closed state, so that socket's onclose reads as an unexpected drop
		// and schedules a reconnect — a third socket, on top of the one resubscribe
		// just opened. Both stay live and both feed the same onFrame, so an old delta
		// can land after a newer full frame and paint stale rows over it.
		it("does not let the socket it replaced reconnect behind it", async () => {
			const transport = new WsTransport("sess-42");
			const first = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await first;

			const again = transport.resubscribe();
			wsInstances[1].onopen!();
			await again;
			expect(wsInstances).toHaveLength(2);

			// The close resubscribe asked for finally lands.
			wsInstances[0].onclose!();
			vi.advanceTimersByTime(10_000);
			expect(wsInstances).toHaveLength(2);
		});

		it("ignores frames from a socket it has already replaced", async () => {
			const onFrame = vi.fn();
			const transport = new WsTransport("sess-42");
			const first = transport.subscribe(onFrame);
			wsInstances[0].onopen!();
			await first;

			const again = transport.resubscribe();
			wsInstances[1].onopen!();
			await again;

			wsInstances[0].onmessage!({ data: new Uint8Array([1]).buffer });
			expect(onFrame).not.toHaveBeenCalled();

			wsInstances[1].onmessage!({ data: new Uint8Array([2]).buffer });
			expect(onFrame).toHaveBeenCalledTimes(1);
		});

		it("dispatches binary frames to onFrame handler", async () => {
			const transport = new WsTransport("sess-1");
			const onFrame = vi.fn();
			const subscribePromise = transport.subscribe(onFrame);
			wsInstances[0].onopen!();
			await subscribePromise;

			const buffer = new ArrayBuffer(8);
			wsInstances[0].onmessage!({ data: buffer });
			expect(onFrame).toHaveBeenCalledWith(buffer);
		});

		it("dispatches JSON text messages to event handlers", async () => {
			const transport = new WsTransport("sess-1");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			const handler = vi.fn();
			await transport.onEvent("parsed", handler);

			wsInstances[0].onmessage!({ data: JSON.stringify({ type: "parsed", event: { kind: "cwd" } }) });
			expect(handler).toHaveBeenCalledWith({ event: { kind: "cwd" } });
		});

		// The frames below are byte-for-byte what `grid_ws_frame` serialises in
		// src-tauri/src/mcp_http/session.rs — snake_case `exit_code` included,
		// because the desktop side sends `Osc133Event` and the two must agree.
		// Asserting against a hand-built object would prove nothing about the wire.
		it("delivers an osc133 frame with the field names the Rust event carries", async () => {
			const transport = new WsTransport("sess-1");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			const handler = vi.fn();
			await transport.onEvent("osc133", handler);

			wsInstances[0].onmessage!({
				data: JSON.stringify({ type: "osc133", marker: "D", line: 42, exit_code: 1 }),
			});
			expect(handler).toHaveBeenCalledWith({ marker: "D", line: 42, exit_code: 1 });
		});

		it("delivers a cwd frame as the same { cwd } object the desktop event carries", async () => {
			const transport = new WsTransport("sess-1");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			const handler = vi.fn();
			await transport.onEvent("cwd", handler);

			wsInstances[0].onmessage!({ data: JSON.stringify({ type: "cwd", cwd: "/tmp/work" }) });
			expect(handler).toHaveBeenCalledWith({ cwd: "/tmp/work" });
		});

		// Catches: socket-open clearing the banner before replay, or retry ten appearing as failure before it fails.
		it("reports actual retries, replay recovery, and exhaustion only after the tenth failure", async () => {
			const transport = new WsTransport("sess-42");
			const reconnecting = vi.fn();
			const recovered = vi.fn();
			const exhausted = vi.fn();
			transport.onStreamReconnecting(reconnecting);
			transport.onStreamRecovered(recovered);
			transport.onStreamExhausted(exhausted);
			const initial = transport.subscribe(vi.fn());
			wsInstances[0].onopen?.();
			await initial;
			wsInstances[0].onclose?.();
			expect(reconnecting).toHaveBeenLastCalledWith(1, 10);
			await vi.advanceTimersByTimeAsync(1000);
			wsInstances[1].onopen?.();
			await Promise.resolve();
			expect(recovered).not.toHaveBeenCalled();
			wsInstances[1].onmessage?.({ data: JSON.stringify({ type: "grid-replay-empty" }) });
			expect(recovered).toHaveBeenCalledTimes(1);
			wsInstances[1].onclose?.();
			for (let attempt = 1; attempt <= 10; attempt++) {
				expect(reconnecting).toHaveBeenLastCalledWith(attempt, 10);
				expect(exhausted).not.toHaveBeenCalled();
				await vi.advanceTimersByTimeAsync(1000 * 2 ** Math.min(attempt - 1, 5));
				wsInstances.at(-1)?.onclose?.();
				await Promise.resolve();
			}
			expect(exhausted).toHaveBeenCalledExactlyOnceWith(10);
			const socketCount = wsInstances.length;
			await vi.advanceTimersByTimeAsync(60_000);
			expect(wsInstances).toHaveLength(socketCount);
			transport.unsubscribe();
		});

		it("reconnects on unexpected close", async () => {
			const transport = new WsTransport("sess-1");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			// Simulate unexpected close
			wsInstances[0].onclose!();
			expect(wsInstances).toHaveLength(1);

			// After 1s reconnect timer fires
			vi.advanceTimersByTime(1000);
			expect(wsInstances).toHaveLength(2);

			// Settle the reconnect connect promise to avoid leak
			wsInstances[1].onopen!();
			transport.unsubscribe();
		});

		it("does not reconnect after explicit unsubscribe", async () => {
			const transport = new WsTransport("sess-1");
			const subscribePromise = transport.subscribe(vi.fn());
			wsInstances[0].onopen!();
			await subscribePromise;

			transport.unsubscribe();
			expect(wsInstances[0].close).toHaveBeenCalled();

			vi.advanceTimersByTime(2000);
			expect(wsInstances).toHaveLength(1); // no new instance
		});

		it("delegates invoke to rpc()", async () => {
			const { rpc } = await import("../transport");
			(rpc as ReturnType<typeof vi.fn>).mockResolvedValue("ws-result");
			const transport = new WsTransport("session-1");
			const result = await transport.invoke("resize_pty", { sessionId: "session-1", rows: 24, cols: 80 });

			// No connection id: a browser-mode local terminal, which is what `rpc`
			// already assumes when the third argument is absent.
			expect(rpc).toHaveBeenCalledWith("resize_pty", { sessionId: "session-1", rows: 24, cols: 80 }, undefined);
			expect(result).toBe("ws-result");
		});

		// A terminal owned by a remote machine: the socket, and every call about
		// that session, must reach the daemon that owns the PTY — with a credential
		// the upgrade request cannot put in a header.
		describe("against a remote connection", () => {
			beforeEach(() => {
				setRemoteBaseUrlLookup((id) => (id === "conn-1" ? "http://remote.test:9876" : undefined));
				setRemoteTokenLookup((id) => (id === "conn-1" ? "tok-abc" : undefined));
			});
			afterEach(() => {
				setRemoteBaseUrlLookup(() => undefined);
				setRemoteTokenLookup(() => undefined);
			});

			it("opens the stream on the remote daemon with the session token in the URL", async () => {
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(vi.fn());
				wsInstances[0].onopen!();
				await subscribed;

				expect(MockWebSocket.lastUrl).toBe(
					"ws://remote.test:9876/sessions/sess-9/stream?format=grid&compress=deflate&token=tok-abc",
				);
			});

			// The whole point of the feature: this is the socket with a link on
			// it. A local socket asks for nothing (asserted above), so the query
			// parameter is the one thing that tells the two apart on the wire.
			it("asks the remote daemon to compress the stream", async () => {
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(vi.fn());
				wsInstances[0].onopen!();
				await subscribed;

				expect(MockWebSocket.lastUrl).toContain("compress=deflate");
				// And offers the subprotocol, which is the half the server answers.
				// Without the offer a server that tags its frames cannot say so, and
				// RFC 6455 forbids it selecting one that was not offered.
				expect(MockWebSocket.lastProtocols).toEqual([DEFLATE_SUBPROTOCOL]);
			});

			// The failure this negotiation exists for. An older daemon ignores the
			// query parameter and sends the frames untouched; a client that assumed
			// its own request was granted reads the first byte of a grid row as a
			// tag — 0x01 fails to inflate, 0x00 hands the renderer a frame one byte
			// short, and a JSON frame arrives as a string the decoder throws on.
			it("falls back to untagged framing against a server that does not compress", async () => {
				serverAnswer = "ignore";
				const onFrame = vi.fn();
				const handler = vi.fn();
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(onFrame);
				wsInstances[0].onopen!();
				await subscribed;
				await transport.onEvent("cwd", handler);

				// Bytes as the old server sends them: no tag, and a leading 0x01 that
				// a tag reader would have taken for "deflated".
				const grid = new Uint8Array([1, 2, 3]).buffer;
				wsInstances[0].onmessage!({ data: grid });
				wsInstances[0].onmessage!({ data: JSON.stringify({ type: "cwd", cwd: "/tmp/work" }) });

				// Synchronous, because the untagged path has nothing to inflate.
				expect(onFrame).toHaveBeenCalledWith(grid);
				expect(handler).toHaveBeenCalledWith({ cwd: "/tmp/work" });
			});

			it("reads a deflated grid frame back to the bytes the server serialised", async () => {
				const onFrame = vi.fn();
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(onFrame);
				wsInstances[0].onopen!();
				await subscribed;

				const grid = new Uint8Array(4096).fill(0x41);
				wsInstances[0].onmessage!({ data: await taggedFrame(FRAME_TAG.binaryDeflate, grid) });
				await vi.waitFor(() => expect(onFrame).toHaveBeenCalledTimes(1));

				expect(new Uint8Array(onFrame.mock.calls[0][0] as ArrayBuffer)).toEqual(grid);
			});

			// Criterion 4 on the client: a frame the server decided not to
			// compress still arrives tagged, and the tag byte must not reach the
			// grid decoder as if it were a row.
			it("strips the tag from a frame the server chose not to compress", async () => {
				const onFrame = vi.fn();
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(onFrame);
				wsInstances[0].onopen!();
				await subscribed;

				const grid = new Uint8Array([7, 8, 9]);
				wsInstances[0].onmessage!({ data: await taggedFrame(FRAME_TAG.binary, grid) });
				await vi.waitFor(() => expect(onFrame).toHaveBeenCalledTimes(1));

				expect(new Uint8Array(onFrame.mock.calls[0][0] as ArrayBuffer)).toEqual(grid);
			});

			it("delivers a deflated JSON frame to the handler for its type", async () => {
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe(vi.fn());
				wsInstances[0].onopen!();
				await subscribed;

				const handler = vi.fn();
				await transport.onEvent("cwd", handler);

				const json = new TextEncoder().encode(JSON.stringify({ type: "cwd", cwd: "/tmp/work" }));
				wsInstances[0].onmessage!({ data: await taggedFrame(FRAME_TAG.textDeflate, json) });

				await vi.waitFor(() => expect(handler).toHaveBeenCalledWith({ cwd: "/tmp/work" }));
			});

			// Grid frames are deltas. Inflating is asynchronous, so two frames
			// racing would be applied in whichever order finished first, and the
			// rows the loser carried would be painted over stale content.
			it("applies frames in the order the server sent them, not the order they inflate", async () => {
				const seen: number[] = [];
				const transport = new WsTransport("sess-9", "conn-1");
				const subscribed = transport.subscribe((data) => seen.push(new Uint8Array(data)[0]));
				wsInstances[0].onopen!();
				await subscribed;

				// The first is big and slow to inflate, the second is one byte and
				// instant. Without the chain the second would land first.
				const big = await taggedFrame(FRAME_TAG.binaryDeflate, new Uint8Array(200_000).fill(1));
				const small = await taggedFrame(FRAME_TAG.binary, new Uint8Array([2]));
				wsInstances[0].onmessage!({ data: big });
				wsInstances[0].onmessage!({ data: small });

				await vi.waitFor(() => expect(seen).toHaveLength(2));
				expect(seen).toEqual([1, 2]);
			});

			it("routes invoke to the machine that owns the session", async () => {
				const { rpc } = await import("../transport");
				(rpc as ReturnType<typeof vi.fn>).mockResolvedValue("ok");
				const transport = new WsTransport("sess-9", "conn-1");
				await transport.invoke("write_to_pty", { sessionId: "sess-9", data: "ls\r" });

				expect(rpc).toHaveBeenCalledWith("write_to_pty", { sessionId: "sess-9", data: "ls\r" }, "conn-1");
			});

			// Not connected, or connected but unauthenticated: there is no URL to
			// open, and retrying would spin against a daemon answering 401.
			it("refuses to open a socket for a connection that is not connected", async () => {
				const transport = new WsTransport("sess-9", "conn-gone");
				await expect(transport.subscribe(vi.fn())).rejects.toThrow("not connected");
				expect(wsInstances).toHaveLength(0);
			});

			it("createTransport picks the WS even under Tauri when a connection owns the session", () => {
				(isTauri as ReturnType<typeof vi.fn>).mockReturnValue(true);
				expect(createTransport("sess-9", "conn-1")).toBeInstanceOf(WsTransport);
			});
		});
	});
});
