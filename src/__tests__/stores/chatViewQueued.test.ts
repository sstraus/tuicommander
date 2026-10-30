import { beforeEach, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen: async () => () => {} }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

import { acpTranscript } from "../../stores/acpTranscript";
import { chatViewStore } from "../../stores/chatView";

const user = (messageId: string, text: string) => ({
	sessionUpdate: "user_message_chunk",
	messageId,
	content: { type: "text", text },
});
async function read(updates: unknown[], reset = false, epoch = 1, sessionId = "queued") {
	invoke.mockResolvedValueOnce({ epoch, nextSeq: 10, reset, updates, unknownRows: 0, malformedRows: 0 });
	await chatViewStore.refresh(sessionId);
}
beforeEach(async () => {
	invoke.mockReset();
	chatViewStore.reset();
	acpTranscript.reset();
	await chatViewStore.watch("queued");
	await read([user("old", "repeat")], true);
});

it("keeps pending prompts through activity and consumes repeated echoes only once", async () => {
	chatViewStore.beginSubmission("queued", "repeat");
	chatViewStore.beginSubmission("queued", "repeat");
	expect(chatViewStore.entries("queued").flatMap((e) => (e.kind === "user" && e.pending ? [e.text] : []))).toEqual([
		"repeat",
		"repeat",
	]);
	chatViewStore.clearSubmission("queued");
	await read([]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(2);
	await read([user("new-1", "repeat")]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(1);
	await read([]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(1);
	await read([user("new-2", "repeat")]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(0);
	expect(
		chatViewStore
			.entries("queued")
			.filter((e) => e.kind === "user")
			.map((e) => e.text),
	).toEqual(["repeat", "repeat", "repeat"]);
});

it("rolls back only its own failed send and never consumes another PTY's prompt", async () => {
	const rollback = chatViewStore.beginSubmission("queued", "first");
	chatViewStore.beginSubmission("queued", "second");
	rollback();
	await chatViewStore.watch("other");
	await read([user("other", "second")], true, 1, "other");
	expect(chatViewStore.entries("queued").flatMap((e) => (e.kind === "user" && e.pending ? [e.text] : []))).toEqual([
		"second",
	]);
	await read([user("own", "second")]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(0);
});

it("does not match replayed history and drops pending feedback on transcript replacement", async () => {
	chatViewStore.beginSubmission("queued", "repeat");
	await read([user("old", "repeat")], true);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(1);
	await read([user("replacement", "unrelated")], true, 2);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(0);
});
