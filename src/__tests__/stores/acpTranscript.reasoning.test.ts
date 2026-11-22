import { beforeEach, expect, it } from "vitest";
import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpClientEvent, AcpEventEnvelope, AcpSessionUpdate } from "../../types/acp";

const SESSION = "chat-a";
const CONNECTION = "connection-a";
let sequence = 0;

function event(event: AcpClientEvent, owner: Partial<Omit<AcpEventEnvelope, "event">> = {}) {
	acpTranscript.applyFrame({
		kind: "event",
		connectionId: CONNECTION,
		generation: 1,
		sequence: ++sequence,
		sessionId: SESSION,
		turnId: "turn-a",
		...owner,
		event,
	});
}
function chunk(text: string, owner: Partial<Omit<AcpEventEnvelope, "event">> = {}) {
	event(
		{ kind: "sessionUpdate", update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text } } },
		owner,
	);
}
const reasoning = () => acpTranscript.reasoning(SESSION, CONNECTION);

beforeEach(() => {
	sequence = 0;
	acpTranscript.reset();
});

// Catches: optimistic/rejected sends and queue receipts masquerade as a running turn.
it("starts only on dispatch, not on optimistic text, a queue receipt or turnStarted alone", () => {
	acpTranscript.noteUserMessage(SESSION, "not accepted");
	event({ kind: "promptQueueChanged", queuedPrompts: [{ turnId: "queued", summary: "next" }] });
	event({ kind: "turnStarted" });
	expect(reasoning()).toBeNull();
	event({ kind: "promptSent", text: "accepted" });
	expect(reasoning()).toBe("");
	chunk("public thought");
	event({ kind: "promptQueueChanged", queuedPrompts: [{ turnId: "queued", summary: "next" }] }, { turnId: "queued" });
	expect(reasoning()).toBe("public thought");
});

// Catches: final-answer cleanup erases evidence, or a late thought revives the transient row.
it("streams public thoughts and hides them on the answer without deleting evidence", () => {
	event({ kind: "promptSent", text: "accepted" });
	event({ kind: "turnStarted" });
	chunk("Inspect ");
	chunk("the request.");
	expect(reasoning()).toBe("Inspect the request.");
	event({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Answer" } },
	});
	expect(reasoning()).toBeNull();
	chunk("late evidence");
	expect(reasoning()).toBeNull();
	expect(
		acpTranscript
			.entries(SESSION)
			.filter((entry) => entry.kind === "thought")
			.map((entry) => entry.text),
	).toEqual(["Inspect the request.", "late evidence"]);
});

// Catches: opaque continuation data, replayed history or notices replace public reasoning.
it("does not interpret private blocks, non-string text, inherited text or notices as live reasoning", () => {
	event({ kind: "promptSent", text: "accepted" });
	for (const update of [
		{ sessionUpdate: "agent_thought_chunk", content: { type: "thinking", thinking: "private" } },
		{ sessionUpdate: "agent_thought_chunk", content: { type: "text", text: { opaque: "private" } } },
		{
			sessionUpdate: "agent_thought_chunk",
			content: { type: "text", text: "history" },
			_meta: { ego: { inherited: true } },
		},
		{
			sessionUpdate: "agent_message_chunk",
			content: { type: "text", text: "Notice" },
			_meta: { ego: { salience: "card" } },
		},
	])
		event({ kind: "sessionUpdate", update: update as AcpSessionUpdate });
	expect(reasoning()).toBe("");
	chunk("public");
	expect(reasoning()).toBe("public");
});

// Catches: cancellation, refusal, empty completion or failure leaves reasoning visible.
it.each<AcpClientEvent>([
	{ kind: "turnSettled", stopReason: "cancelled", usage: null },
	{ kind: "turnSettled", stopReason: "refusal", usage: null },
	{ kind: "turnSettled", stopReason: "end_turn", usage: null },
	{ kind: "turnFailed", message: "failed", state: "idle" },
])("seals reasoning after $kind $stopReason", (terminal) => {
	event({ kind: "promptSent", text: "accepted" });
	chunk("retained");
	event(terminal);
	chunk("late");
	event({ kind: "promptSent", text: "duplicate" });
	expect(reasoning()).toBeNull();
	expect(
		acpTranscript.entries(SESSION).some((entry) => entry.kind === "thought" && entry.text.includes("retained")),
	).toBe(true);
	event({ kind: "promptSent", text: "next" }, { turnId: "turn-b" });
	expect(reasoning()).toBe("");
});

// Catches: late/duplicate/wrong-owner chunks contaminate the current turn.
it("isolates session, connection, generation and turn identity and ignores repeated sequences", () => {
	event({ kind: "promptSent", text: "first" });
	chunk("first");
	event({ kind: "promptSent", text: "next" }, { turnId: "turn-b", generation: 2 });
	chunk("current", { turnId: "turn-b", generation: 2 });
	const acceptedSequence = sequence;
	for (const owner of [
		{ turnId: "turn-a", generation: 2 },
		{ turnId: "turn-b", generation: 1 },
		{ turnId: "turn-b", generation: 2, connectionId: "other" },
		{ turnId: "turn-b", generation: 2, sessionId: "other" },
		{ turnId: "turn-b", generation: 2, sequence: acceptedSequence },
	])
		chunk("wrong", owner);
	event({ kind: "promptSent", text: "old dispatch" }, { generation: 1 });
	expect(reasoning()).toBe("current");
	expect(acpTranscript.reasoning(SESSION, "other")).toBeNull();
});

// Catches: a replay or restored transcript revives a transient turn, or teardown leaks across connections.
it("clears only the disconnected owner and never rebuilds reasoning from retained history", () => {
	event({ kind: "promptSent", text: "accepted" });
	chunk("retained");
	event({ kind: "promptSent", text: "other" }, { sessionId: "other", connectionId: "other" });
	acpTranscript.stopReasoning(CONNECTION);
	chunk("late");
	expect(reasoning()).toBeNull();
	expect(acpTranscript.reasoning("other", "other")).toBe("");
	const entries = acpTranscript.clear(SESSION);
	acpTranscript.restore(SESSION, entries);
	chunk("replay", { turnId: null });
	expect(reasoning()).toBeNull();
	expect(acpTranscript.entries(SESSION).some((entry) => entry.kind === "thought")).toBe(true);
});
