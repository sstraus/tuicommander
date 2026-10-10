import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
type Wake = (event: { payload: { session_id: string } }) => void;
const wakes = new Set<Wake>();
const warn = vi.fn();
vi.mock("../../invoke", () => ({
	invoke: (...args: unknown[]) => invokeMock(...args),
	listen: async (_name: string, handler: Wake) => {
		wakes.add(handler);
		return () => wakes.delete(handler);
	},
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: (...args: unknown[]) => warn(...args) } }));

import { acpTranscript } from "../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../stores/chatView";

function snapshot(text: string, nextSeq: number, reset = false, epoch = 0) {
	return {
		epoch,
		nextSeq,
		reset,
		unknownRows: 0,
		malformedRows: 0,
		updates: text
			? [{ sessionUpdate: "agent_message_chunk", messageId: `m${nextSeq}`, content: { type: "text", text } }]
			: [],
	};
}
function wake(sessionId: string) {
	for (const listener of wakes) listener({ payload: { session_id: sessionId } });
}
function texts(sessionId: string) {
	return acpTranscript
		.entries(chatViewKey(sessionId))
		.map((entry) => (entry.kind === "agent" ? entry.text : entry.kind));
}

beforeEach(() => {
	invokeMock.mockReset();
	warn.mockReset();
	wakes.clear();
	chatViewStore.reset();
	acpTranscript.reset();
});

describe("chat view follow boundaries", () => {
	// Catches: wakes arriving during a read being lost, or the stale cursor duplicating a chunk.
	it("drains a burst of wakes during an in-flight read from the advanced cursor", async () => {
		const dispose = await chatViewStore.watch("one");
		let finish: (value: ReturnType<typeof snapshot>) => void = () => {};
		invokeMock.mockReturnValueOnce(
			new Promise((resolve) => {
				finish = resolve;
			}),
		);
		invokeMock.mockResolvedValueOnce(snapshot("second", 2));
		const pending = chatViewStore.refresh("one");
		for (let i = 0; i < 5; i++) wake("one");
		finish(snapshot("first", 1, true));
		await pending;
		expect(texts("one")).toEqual(["first", "second"]);
		expect(invokeMock).toHaveBeenCalledTimes(2);
		expect(invokeMock).toHaveBeenLastCalledWith("chat_view_snapshot", { sessionId: "one", epoch: 0, fromSeq: 1 });
		dispose();
	});

	// Catches: closing Chat unregistering the listener permanently or reusing the old cursor on return.
	it("rearms after switching to CLI and back to Chat", async () => {
		const close = await chatViewStore.watch("one");
		invokeMock.mockResolvedValueOnce(snapshot("before", 1, true));
		await chatViewStore.refresh("one");
		close();
		wake("one");
		expect(invokeMock).toHaveBeenCalledTimes(1);
		const dispose = await chatViewStore.watch("one");
		invokeMock.mockResolvedValueOnce(snapshot("while in CLI", 2, true));
		wake("one");
		await vi.waitFor(() => expect(texts("one")).toEqual(["while in CLI"]));
		expect(invokeMock).toHaveBeenLastCalledWith("chat_view_snapshot", { sessionId: "one", epoch: null, fromSeq: 0 });
		dispose();
	});

	// Catches: a single global listener/cursor allowing one Chat terminal to steal the other's wakes.
	it("keeps two watched terminals independent when one closes", async () => {
		const closeOne = await chatViewStore.watch("one");
		const closeTwo = await chatViewStore.watch("two");
		invokeMock.mockImplementation(async (_command, args: { sessionId: string }) => snapshot(args.sessionId, 1, true));
		wake("one");
		wake("two");
		await vi.waitFor(() => {
			expect(texts("one")).toEqual(["one"]);
			expect(texts("two")).toEqual(["two"]);
		});
		closeOne();
		invokeMock.mockResolvedValueOnce(snapshot("still following", 2));
		wake("two");
		await vi.waitFor(() => expect(texts("two")).toEqual(["two", "still following"]));
		closeTwo();
	});

	// Catches: event-driven reads failing silently and leaving no diagnostic for a frozen chat.
	it("warns on a failed wake refresh and accepts the next wake", async () => {
		const dispose = await chatViewStore.watch("one");
		invokeMock.mockRejectedValueOnce(new Error("read failed"));
		wake("one");
		await vi.waitFor(() =>
			expect(warn).toHaveBeenCalledWith("terminal", "Chat view refresh failed", {
				sessionId: "one",
				error: "read failed",
			}),
		);
		invokeMock.mockResolvedValueOnce(snapshot("recovered", 1, true));
		wake("one");
		await vi.waitFor(() => expect(texts("one")).toEqual(["recovered"]));
		dispose();
	});
	// Catches: a malformed snapshot throwing after invoke resolves and bypassing refresh diagnostics.
	it("warns when applying a malformed snapshot fails after the request succeeds", async () => {
		const dispose = await chatViewStore.watch("one");
		invokeMock.mockResolvedValueOnce({ ...snapshot("", 1), updates: null });
		await expect(chatViewStore.refresh("one")).rejects.toThrow();
		expect(warn).toHaveBeenCalledWith("terminal", "Chat view refresh failed", {
			sessionId: "one",
			error: expect.any(String),
		});
		dispose();
	});
});
