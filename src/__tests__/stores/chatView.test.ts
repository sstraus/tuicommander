import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const listeners = new Map<string, (event: { payload: unknown }) => void>();
vi.mock("../../invoke", () => ({
	invoke: (...args: unknown[]) => invokeMock(...args),
	listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
		listeners.set(name, handler);
		return () => listeners.delete(name);
	},
}));

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

import { acpTranscript } from "../../stores/acpTranscript";
import { chatViewAvailability, chatViewKey, chatViewStore } from "../../stores/chatView";

const SID = "sess-1";
const text = (t: string) => ({ type: "text", text: t });

function snapshot(partial: Record<string, unknown>) {
	return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0, ...partial };
}

beforeEach(() => {
	invokeMock.mockReset();
	listeners.clear();
	chatViewStore.reset();
	acpTranscript.reset();
});

describe("chatViewAvailability", () => {
	const bound = { agentType: "claude", agentSessionId: "uuid", sessionId: "s", isRemote: false } as const;

	it("is available for a Claude agent bound to a session", () => {
		expect(chatViewAvailability({ ...bound })).toEqual({ available: true });
	});

	// Catches: another tab's conversation shown for an agent TUIC has not bound (issue #119 class).
	it("toggle_disabled_when_agent_unbound_with_reason", () => {
		const result = chatViewAvailability({ ...bound, agentSessionId: null });
		expect(result).toEqual({ available: false, reason: expect.stringContaining("not bound") });
	});

	it("names a reason for a shell tab, a non-Claude agent and a remote terminal", () => {
		for (const term of [
			undefined,
			{ ...bound, agentType: null },
			{ ...bound, agentType: "codex" },
			{ ...bound, isRemote: true },
			{ ...bound, sessionId: null },
		] as const) {
			const result = chatViewAvailability(term as never);
			expect(result.available).toBe(false);
			expect("reason" in result && result.reason.length).toBeGreaterThan(0);
		}
	});
});

describe("chatViewStore.refresh", () => {
	it("folds ACP updates into the terminal's conversation through the AI Chat reducer", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockResolvedValueOnce(
			snapshot({
				nextSeq: 5,
				updates: [
					{ sessionUpdate: "user_message_chunk", messageId: "u1", content: text("question") },
					{ sessionUpdate: "agent_message_chunk", messageId: "m1", content: text("first") },
					{ sessionUpdate: "agent_message_chunk", messageId: "m1", content: text("\n\nsecond") },
					{ sessionUpdate: "tool_call", toolCallId: "t1", title: "Bash: ls", kind: "execute", status: "in_progress" },
					{ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "failed" },
				],
			}),
		);
		await chatViewStore.refresh(SID);
		const entries = acpTranscript.entries(chatViewKey(SID));
		expect(entries.map((e) => e.kind)).toEqual(["user", "agent", "tool"]);
		const agent = entries[1];
		expect(agent.kind === "agent" && agent.text).toBe("first\n\nsecond");
		const tool = entries[2];
		expect(tool.kind === "tool" && tool.call.status).toBe("failed");
		expect(tool.kind === "tool" && tool.call.title).toBe("Bash: ls");
	});

	it("asks for the next sequence of the epoch it last saw", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockResolvedValueOnce(snapshot({ epoch: 3, nextSeq: 9 }));
		await chatViewStore.refresh(SID);
		invokeMock.mockResolvedValueOnce(snapshot({ epoch: 3, nextSeq: 9, reset: false }));
		await chatViewStore.refresh(SID);
		expect(invokeMock).toHaveBeenLastCalledWith("chat_view_snapshot", { sessionId: SID, epoch: 3, fromSeq: 9 });
	});

	// Catches: after /clear the new conversation is drawn under the old one.
	it("a reset drops what was held before applying the snapshot", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockResolvedValueOnce(
			snapshot({ updates: [{ sessionUpdate: "agent_message_chunk", messageId: "a", content: text("old") }] }),
		);
		await chatViewStore.refresh(SID);
		invokeMock.mockResolvedValueOnce(
			snapshot({
				epoch: 1,
				updates: [{ sessionUpdate: "agent_message_chunk", messageId: "b", content: text("new") }],
			}),
		);
		await chatViewStore.refresh(SID);
		const entries = acpTranscript.entries(chatViewKey(SID));
		expect(entries.map((e) => (e.kind === "agent" ? e.text : ""))).toEqual(["new"]);
	});

	it("records the reason when the backend says the terminal is not bound", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockRejectedValueOnce(new Error("not_bound: no Claude agent is bound to this terminal"));
		await chatViewStore.refresh(SID);
		expect(chatViewStore.unavailableReason(SID)).toBe("no Claude agent is bound to this terminal");
	});

	it("rethrows any other failure", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockRejectedValueOnce(new Error("boom"));
		await expect(chatViewStore.refresh(SID)).rejects.toThrow("boom");
	});

	// Catches: a late reply for a closed view repopulating a conversation nobody watches.
	it("ignores a reply that lands after the view was closed", async () => {
		const dispose = await chatViewStore.watch(SID);
		let resolve: (value: unknown) => void = () => {};
		invokeMock.mockReturnValueOnce(new Promise((r) => (resolve = r)));
		const pending = chatViewStore.refresh(SID);
		dispose();
		resolve(snapshot({ updates: [{ sessionUpdate: "agent_message_chunk", content: text("late") }] }));
		await pending;
		expect(acpTranscript.entries(chatViewKey(SID))).toEqual([]);
	});

	it("wakes on chat-view-changed for its own terminal only", async () => {
		await chatViewStore.watch(SID);
		invokeMock.mockResolvedValue(snapshot({ reset: false }));
		listeners.get("chat-view-changed")?.({ payload: { session_id: "other" } });
		expect(invokeMock).not.toHaveBeenCalled();
		listeners.get("chat-view-changed")?.({ payload: { session_id: SID } });
		expect(invokeMock).toHaveBeenCalledTimes(1);
	});
});
