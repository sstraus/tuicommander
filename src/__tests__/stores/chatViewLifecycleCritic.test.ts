import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("../../invoke", () => transport);
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

import { acpTranscript } from "../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../stores/chatView";

describe("Chat view listener registration across CLI/Chat remount", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		chatViewStore.reset();
		acpTranscript.clear(chatViewKey("terminal"));
	});

	it("late cleanup from the previous mount must not stop the new mount following", async () => {
		let finishOldRegistration!: (dispose: () => void) => void;
		const callbacks: Array<(event: { payload: { session_id: string } }) => void> = [];
		const unlistenOld = vi.fn();
		transport.listen.mockImplementation((_name, callback) => {
			callbacks.push(callback);
			if (callbacks.length === 1) {
				return new Promise<() => void>((resolve) => {
					finishOldRegistration = resolve;
				});
			}
			return Promise.resolve(() => {});
		});
		transport.invoke.mockResolvedValue({
			epoch: 1,
			nextSeq: 1,
			reset: true,
			updates: [
				{ sessionUpdate: "agent_message_chunk", messageId: "reply", content: { type: "text", text: "latest reply" } },
			],
			unknownRows: 0,
			malformedRows: 0,
		});

		// The first component is removed while native listen registration is pending.
		const oldMount = chatViewStore.watch("terminal");
		// A new Chat component mounts before the old registration settles.
		const closeNew = await chatViewStore.watch("terminal");
		// TerminalChatView's disposed branch immediately runs this late disposer.
		finishOldRegistration(unlistenOld);
		(await oldMount)();
		expect(unlistenOld).toHaveBeenCalledTimes(1);
		callbacks[1]({ payload: { session_id: "terminal" } });
		await chatViewStore.refresh("terminal");

		expect(acpTranscript.entries(chatViewKey("terminal"))).toHaveLength(1);
		closeNew();
	});

	// Catches: a snapshot started by a closed mount overwriting the remount's transcript/cursor.
	it("discards the previous mount's in-flight snapshot before reading for the new mount", async () => {
		transport.listen.mockResolvedValue(() => {});
		let finishOldRead!: (snapshot: unknown) => void;
		transport.invoke.mockReturnValueOnce(
			new Promise((resolve) => {
				finishOldRead = resolve;
			}),
		);
		const closeOld = await chatViewStore.watch("terminal");
		const oldRead = chatViewStore.refresh("terminal");
		closeOld();
		const closeNew = await chatViewStore.watch("terminal");
		await chatViewStore.refresh("terminal");
		transport.invoke.mockResolvedValueOnce({
			epoch: 2,
			nextSeq: 0,
			reset: true,
			updates: [],
			unknownRows: 0,
			malformedRows: 0,
		});
		finishOldRead({
			epoch: 1,
			nextSeq: 99,
			reset: true,
			updates: [{ sessionUpdate: "agent_message_chunk", messageId: "stale", content: { type: "text", text: "stale" } }],
			unknownRows: 0,
			malformedRows: 0,
		});
		await oldRead;
		expect(transport.invoke).toHaveBeenLastCalledWith("chat_view_snapshot", {
			sessionId: "terminal",
			epoch: null,
			fromSeq: 0,
		});
		expect(acpTranscript.entries(chatViewKey("terminal"))).toEqual([]);
		closeNew();
	});
});
