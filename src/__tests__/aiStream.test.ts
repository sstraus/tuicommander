import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { aiWsUrl, openChatStream, openConversationStream } from "../utils/aiStream";

class MockWebSocket {
	static instances: MockWebSocket[] = [];
	url: string;
	onopen: (() => void) | null = null;
	onmessage: ((e: { data: string }) => void) | null = null;
	onerror: (() => void) | null = null;
	onclose: ((e: { code: number }) => void) | null = null;
	closed = false;
	sent: string[] = [];
	constructor(url: string) {
		this.url = url;
		MockWebSocket.instances.push(this);
	}
	send(data: string): void {
		this.sent.push(data);
	}
	close(): void {
		this.closed = true;
	}
	triggerOpen(): void {
		this.onopen?.();
	}
	triggerMessage(obj: unknown): void {
		this.onmessage?.({ data: JSON.stringify(obj) });
	}
	triggerRaw(data: string): void {
		this.onmessage?.({ data });
	}
	triggerError(): void {
		this.onerror?.();
	}
	triggerClose(code: number): void {
		this.onclose?.({ code });
	}
}

describe("aiStream", () => {
	let originalWs: typeof WebSocket;

	beforeEach(() => {
		MockWebSocket.instances = [];
		originalWs = globalThis.WebSocket;
		(globalThis as Record<string, unknown>).WebSocket = MockWebSocket as unknown as typeof WebSocket;
	});

	afterEach(() => {
		(globalThis as Record<string, unknown>).WebSocket = originalWs;
		vi.unstubAllGlobals();
	});

	describe("aiWsUrl()", () => {
		it("uses ws:// for an http origin", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "localhost:9877" } });
			expect(aiWsUrl("/ai/chat/abc/stream")).toBe("ws://localhost:9877/ai/chat/abc/stream");
		});

		it("uses wss:// for an https origin", () => {
			vi.stubGlobal("window", { location: { protocol: "https:", host: "example.com" } });
			expect(aiWsUrl("/x")).toBe("wss://example.com/x");
		});
	});

	describe("openConversationStream()", () => {
		it("opens the session WS, sends params on open, dispatches frames, disposer closes", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const events: unknown[] = [];
			const dispose = openConversationStream("sess 1", { message: "hi", autonomy: "assisted" }, (e) => events.push(e));
			const ws = MockWebSocket.instances[0];
			expect(ws.url).toBe("ws://h/ai/conversation/sess%201/stream");

			ws.triggerOpen();
			expect(JSON.parse(ws.sent[0])).toEqual({ message: "hi", autonomy: "assisted" });

			ws.triggerMessage({ type: "text_chunk", text: "yo" });
			expect(events).toEqual([{ type: "text_chunk", text: "yo" }]);

			dispose();
			expect(ws.closed).toBe(true);
		});

		it("ignores malformed frames", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const events: unknown[] = [];
			openConversationStream("s", { message: "x" }, (e) => events.push(e));
			MockWebSocket.instances[0].triggerRaw("not json");
			expect(events).toEqual([]);
		});

		it("fires onClose(false) on a socket error", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const onClose = vi.fn();
			openConversationStream("s", { message: "x" }, () => {}, onClose);
			MockWebSocket.instances[0].triggerError();
			expect(onClose).toHaveBeenCalledWith(false);
		});

		it("fires onClose(true) only for a clean (1000) close, once", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const onClose = vi.fn();
			openConversationStream("s", { message: "x" }, () => {}, onClose);
			const ws = MockWebSocket.instances[0];
			ws.triggerClose(1000);
			ws.triggerClose(1006); // a second close must not re-fire
			expect(onClose).toHaveBeenCalledTimes(1);
			expect(onClose).toHaveBeenCalledWith(true);
		});

		it("does not fire onClose when the caller disposes the stream", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const onClose = vi.fn();
			const dispose = openConversationStream("s", { message: "x" }, () => {}, onClose);
			dispose();
			MockWebSocket.instances[0].triggerClose(1006);
			expect(onClose).not.toHaveBeenCalled();
		});
	});

	describe("openChatStream()", () => {
		it("opens the chat WS, dispatches snapshot + deltas, disposer closes", () => {
			vi.stubGlobal("window", { location: { protocol: "http:", host: "h" } });
			const events: unknown[] = [];
			const dispose = openChatStream("chat-1", (e) => events.push(e));
			const ws = MockWebSocket.instances[0];
			expect(ws.url).toBe("ws://h/ai/chat/chat-1/stream");

			ws.triggerMessage({ kind: "snapshot", messages: [] });
			ws.triggerMessage({ kind: "chunk", delta: "a" });
			expect(events).toEqual([
				{ kind: "snapshot", messages: [] },
				{ kind: "chunk", delta: "a" },
			]);

			dispose();
			expect(ws.closed).toBe(true);
		});
	});
});
