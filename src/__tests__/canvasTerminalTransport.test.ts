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

import { createTransport, TauriTransport, WsTransport } from "../components/Terminal/canvasTerminalTransport";
import { isTauri } from "../transport";

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

	describe("TauriTransport", () => {
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

		it("requests initial frame after subscribe", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());

			expect(invoke).toHaveBeenCalledWith("terminal_request_frame", { sessionId: "session-1" });
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

		it("registers event listeners via Tauri listen", async () => {
			const { listen } = await import("@tauri-apps/api/event");
			const transport = new TauriTransport("session-1");
			const handler = vi.fn();
			await transport.subscribe(vi.fn());
			await transport.onEvent("cwd", handler);

			expect(listen).toHaveBeenCalledWith("pty-cwd-session-1", expect.any(Function));
		});

		it("calls unsubscribe_terminal_grid on unsubscribe", async () => {
			const { invoke } = await import("@tauri-apps/api/core");
			const transport = new TauriTransport("session-1");
			await transport.subscribe(vi.fn());
			transport.unsubscribe();

			expect(invoke).toHaveBeenCalledWith("unsubscribe_terminal_grid", { sessionId: "session-1" });
		});
	});

	describe("WsTransport", () => {
		let wsInstances: MockWebSocket[];

		class MockWebSocket {
			static lastUrl = "";
			binaryType = "";
			onmessage: ((e: { data: unknown }) => void) | null = null;
			onclose: (() => void) | null = null;
			onopen: (() => void) | null = null;
			onerror: ((e: unknown) => void) | null = null;
			close = vi.fn();
			constructor(url: string) {
				MockWebSocket.lastUrl = url;
				wsInstances.push(this);
			}
		}

		beforeEach(() => {
			vi.useFakeTimers();
			wsInstances = [];
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

			expect(rpc).toHaveBeenCalledWith("resize_pty", { sessionId: "session-1", rows: 24, cols: 80 });
			expect(result).toBe("ws-result");
		});
	});
});
