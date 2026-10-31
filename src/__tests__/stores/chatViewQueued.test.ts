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

it.each(["/help", "dropped input", "rewritten input"])(
	"retires unmatched older sends when a later receipt arrives: %s",
	async (unmatched) => {
		chatViewStore.beginSubmission("queued", unmatched);
		chatViewStore.beginSubmission("queued", "next prompt");
		chatViewStore.beginSubmission("queued", "still waiting");
		await read([user("next", "next prompt")]);
		expect(chatViewStore.entries("queued").flatMap((e) => (e.kind === "user" && e.pending ? [e.text] : []))).toEqual([
			"still waiting",
		]);
	},
);

it("does not reuse a later receipt after retiring several unmatched sends", async () => {
	chatViewStore.beginSubmission("queued", "/help");
	chatViewStore.beginSubmission("queued", "dropped input");
	chatViewStore.beginSubmission("queued", "repeat");
	chatViewStore.beginSubmission("queued", "repeat");
	await read([user("next", "repeat")]);
	expect(chatViewStore.entries("queued").flatMap((e) => (e.kind === "user" && e.pending ? [e.text] : []))).toEqual([
		"repeat",
	]);
	await read([]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(1);
	await read([user("last", "repeat")]);
	expect(chatViewStore.entries("queued").filter((e) => e.kind === "user" && e.pending)).toHaveLength(0);
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

// Catches: assistant text falsely acknowledging a send without a new user receipt.
it("does not dismiss a pending prompt when the assistant repeats its text", async () => {
	chatViewStore.beginSubmission("queued", "same text");
	await read([
		{ sessionUpdate: "agent_message_chunk", messageId: "assistant", content: { type: "text", text: "same text" } },
	]);
	expect(
		chatViewStore.entries("queued").flatMap((entry) => (entry.kind === "user" && entry.pending ? [entry.text] : [])),
	).toEqual(["same text"]);
	await read([user("receipt", "same text")]);
	expect(chatViewStore.entries("queued").filter((entry) => entry.kind === "user" && entry.pending)).toHaveLength(0);
});

// Catches: treating the initial snapshot as a replacement and dropping an unacknowledged send.
it("retains a prompt submitted before the first snapshot establishes its epoch", async () => {
	await chatViewStore.watch("opening");
	chatViewStore.beginSubmission("opening", "sent before snapshot");
	await read([], true, 7, "opening");
	expect(
		chatViewStore.entries("opening").flatMap((entry) => (entry.kind === "user" && entry.pending ? [entry.text] : [])),
	).toEqual(["sent before snapshot"]);
});

// Catches: either receipt-side or submission-side whitespace preventing acknowledgement.
it.each([
	["prompt", " \tprompt\n"],
	[" \tprompt\n", "prompt"],
])("acknowledges whitespace-normalized receipt for %j and %j", async (submitted, received) => {
	chatViewStore.beginSubmission("queued", submitted);
	await read([user("receipt", received)]);
	expect(chatViewStore.entries("queued").filter((entry) => entry.kind === "user" && entry.pending)).toHaveLength(0);
	expect(chatViewStore.entries("queued").flatMap((entry) => (entry.kind === "user" ? [entry.text] : []))).toEqual([
		"repeat",
		received,
	]);
});

// Catches: truthy whitespace-only text creating an empty pending user bubble.
it("does not create a pending bubble for a whitespace-only submission", () => {
	chatViewStore.beginSubmission("queued", " \t\n");
	expect(chatViewStore.entries("queued").flatMap((entry) => (entry.kind === "user" ? [entry.text] : []))).toEqual([
		"repeat",
	]);
});

// Catches: reset merging an empty object and retaining pending bubbles into the next test/watch.
it("clears pending feedback when the store is reset and watched again", async () => {
	chatViewStore.beginSubmission("queued", "stale prompt");
	chatViewStore.reset();
	await chatViewStore.watch("queued");
	expect(chatViewStore.entries("queued").filter((entry) => entry.kind === "user" && entry.pending)).toHaveLength(0);
});

// Catches: terminal sends creating hidden pending bubbles when Chat is not open.
it("does not retain pending feedback for an unwatched terminal", () => {
	chatViewStore.beginSubmission("closed", "not in Chat");
	expect(chatViewStore.entries("closed")).toEqual([]);
});

// Catches: adapter truncation leaving the last submitted prompt pending forever.
it("retires a 20001-character prompt with an adapter-shaped truncated receipt", async () => {
	const submitted = "a".repeat(20001);
	const received = `${"a".repeat(19999)}…`;
	chatViewStore.beginSubmission("queued", submitted);
	invoke.mockResolvedValueOnce({
		epoch: 1,
		nextSeq: 11,
		reset: false,
		updates: [user("long", received)],
		unknownRows: 0,
		malformedRows: 0,
		pendingTexts: [received],
	});
	await chatViewStore.refresh("queued");
	expect(chatViewStore.entries("queued").filter((entry) => entry.kind === "user" && entry.pending)).toHaveLength(0);
});

// Catches: redaction mismatch and sharing a transformed receipt between repeated sends.
it("matches redacted receipts once while keeping original pending presentation", async () => {
	const submitted = "inspect AKIAIOSFODNN7EXAMPLE";
	chatViewStore.beginSubmission("queued", submitted);
	chatViewStore.beginSubmission("queued", submitted);
	invoke.mockResolvedValueOnce({
		epoch: 1,
		nextSeq: 11,
		reset: false,
		updates: [user("secret", "inspect [REDACTED]")],
		unknownRows: 0,
		malformedRows: 0,
		pendingTexts: ["inspect [REDACTED]", "inspect [REDACTED]"],
	});
	await chatViewStore.refresh("queued");
	expect(invoke).toHaveBeenLastCalledWith(
		"chat_view_snapshot",
		expect.objectContaining({ pendingTexts: [submitted, submitted] }),
	);
	expect(
		chatViewStore.entries("queued").flatMap((entry) => (entry.kind === "user" && entry.pending ? [entry.text] : [])),
	).toEqual([submitted]);
});

// Catches: an in-flight comparison being applied by position to a newer submission after rollback.
it("binds comparison results to the sends captured before the snapshot", async () => {
	const rollback = chatViewStore.beginSubmission("queued", "first secret");
	let resolve!: (value: unknown) => void;
	invoke.mockReturnValueOnce(
		new Promise((done) => {
			resolve = done;
		}),
	);
	const refresh = chatViewStore.refresh("queued");
	rollback();
	chatViewStore.beginSubmission("queued", "second secret");
	resolve({
		epoch: 1,
		nextSeq: 11,
		reset: false,
		updates: [user("first", "[REDACTED]")],
		unknownRows: 0,
		malformedRows: 0,
		pendingTexts: ["[REDACTED]"],
	});
	await refresh;
	expect(
		chatViewStore.entries("queued").flatMap((entry) => (entry.kind === "user" && entry.pending ? [entry.text] : [])),
	).toEqual(["second secret"]);
});
