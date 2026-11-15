import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the IPC boundary only. Everything this file tests — how a journal frame
// moves the cursor, which frames are terminal, when a gap becomes visible — is
// the store's own logic, and mocking it would test the mock.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import { acpStore } from "../../stores/acp";
import type {
	AcpClientEvent,
	AcpConnectionSnapshot,
	AcpPendingInteraction,
	AcpRequestPermissionRequest,
	AcpSessionConfigOption,
} from "../../types/acp";
import recordedCommands from "../fixtures/ego-available-commands-2393.json";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";

function snapshot(overrides: Partial<AcpConnectionSnapshot> = {}): AcpConnectionSnapshot {
	return {
		connectionId: CONNECTION,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.1.0" },
		capabilities: null,
		attachments: [],
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
		...overrides,
	};
}

function attachment(overrides: Record<string, unknown> = {}) {
	return {
		sessionId: SESSION,
		state: "idle" as const,
		cwd: "/repo",
		additionalDirectories: [],
		configOptions: [],
		usage: null,
		activeTurn: null,
		queuedPrompts: [],
		pendingPermissionIds: [],
		pendingElicitationIds: [],
		...overrides,
	};
}

/** One event frame at `sequence`, as it arrives off the stream. */
function frame(sequence: number, event: AcpClientEvent, sessionId: string | null = null) {
	return {
		kind: "event" as const,
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId,
		turnId: null,
		event,
	};
}

function permissionRequest(): AcpRequestPermissionRequest {
	return {
		sessionId: SESSION,
		toolCall: {},
		options: [
			{ optionId: "allow", name: "Allow", kind: "allow_once" },
			{ optionId: "reject", name: "Reject", kind: "reject_once" },
		],
	};
}

function permission(requestId: string): AcpPendingInteraction {
	return { kind: "permission", requestId, sessionId: SESSION, request: permissionRequest() };
}

beforeEach(() => {
	mockInvoke.mockReset();
	mockInvoke.mockResolvedValue(undefined);
	acpStore.reset();
});

// Catches: async replacement offers merge stale options or alter a sibling attachment.
describe("acpStore: async config offers", () => {
	const model: AcpSessionConfigOption = {
		id: "model",
		name: "Model",
		type: "select",
		currentValue: "opus",
		options: [{ value: "opus", name: "Opus" }],
	};

	it.each([{ options: [] }, { options: [model] }])(
		"replaces the complete option set with $options for only the matching session",
		({ options }) => {
			const sibling = "sibling-session";
			const old = { ...model, id: "old-model" };
			acpStore.applySnapshot(
				snapshot({
					attachments: [attachment({ configOptions: [old] }), attachment({ sessionId: sibling, configOptions: [old] })],
				}),
			);
			acpStore.applyFrame(
				CONNECTION,
				frame(
					2,
					{
						kind: "sessionUpdate",
						update: { sessionUpdate: "config_option_update", configOptions: options },
					},
					SESSION,
				),
			);
			expect(acpStore.attachment(CONNECTION, SESSION)?.configOptions).toEqual(options);
			expect(acpStore.attachment(CONNECTION, sibling)?.configOptions).toEqual([old]);
			expect(acpStore.cursor(CONNECTION)).toBe(2);
		},
	);

	// Catches: a late offer for a detached session creates a phantom attachment.
	it("does not create an attachment for an unknown session", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		acpStore.applyFrame(
			CONNECTION,
			frame(
				2,
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "config_option_update", configOptions: [model] },
				},
				"detached-session",
			),
		);
		expect(acpStore.attachments(CONNECTION)).toEqual([attachment()]);
	});
});

describe("acpStore: what it holds", () => {
	it("holds a connection and the attachments that came with it", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));

		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
		expect(acpStore.attachments(CONNECTION).map((a) => a.sessionId)).toEqual([SESSION]);
		expect(acpStore.attachment(CONNECTION, SESSION)?.state).toBe("idle");
	});

	// The snapshot is the whole picture for one connection, so a later one
	// replaces it rather than merging into it. Merging would keep an attachment
	// the agent has since dropped, and nothing would ever remove it.
	it("replaces a connection's attachments rather than accumulating them", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		acpStore.applySnapshot(snapshot({ attachments: [] }));

		expect(acpStore.attachments(CONNECTION)).toEqual([]);
	});

	it("keeps connections apart", () => {
		const other = "01932d5e-0000-7000-8000-0000000000c2";
		acpStore.applySnapshot(snapshot());
		acpStore.applySnapshot(snapshot({ connectionId: other, attachments: [attachment()] }));

		expect(acpStore.attachments(CONNECTION)).toEqual([]);
		expect(acpStore.attachments(other)).toHaveLength(1);
	});
});

describe("acpStore: the journal cursor", () => {
	// The cursor is what a resume is computed from, so it has to move on every
	// frame the store accepted — not on the frames it found interesting.
	it("advances to the sequence of the last frame handled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(4, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(CONNECTION, frame(5, { kind: "turnSettled", stopReason: "end_turn", usage: null }, SESSION));

		expect(acpStore.cursor(CONNECTION)).toBe(5);
	});

	// `subscribe` delivers `>= from`, so resuming at the last sequence handled
	// re-delivers it. One past it is the only value that neither repeats a frame
	// nor skips one.
	it("resumes one past the last sequence handled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(7, { kind: "turnStarted" }, SESSION));

		expect(acpStore.resumeFrom(CONNECTION)).toBe(8);
	});

	// Nothing handled yet means everything the journal still holds, which is
	// what 0 asks for. Asking for 1 would skip the first event on a connection
	// whose journal has not yet rolled.
	it("resumes from the beginning when nothing has been handled", () => {
		acpStore.applySnapshot(snapshot());

		expect(acpStore.resumeFrom(CONNECTION)).toBe(0);
	});

	// The journal is ordered and the transport delivers in order, so an
	// out-of-order sequence is not a late frame to be slotted in — it is a bug
	// somewhere upstream, and letting it drag the cursor backwards would replay
	// everything after it on the next resume.
	it("never moves the cursor backwards", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(9, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(CONNECTION, frame(3, { kind: "turnStarted" }, SESSION));

		expect(acpStore.cursor(CONNECTION)).toBe(9);
	});
});

describe("acpStore: state events", () => {
	it("a failed prompt releases its own turn and preserves the paused state", () => {
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "turn-1", state: "running", stopReason: null, usage: null },
					}),
				],
			}),
		);
		acpStore.applyFrame(CONNECTION, {
			...frame(2, { kind: "turnFailed", message: "held", state: "paused" }, SESSION),
			turnId: "turn-1",
		});
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({ state: "paused", activeTurn: null });
	});

	it("shows a turn started and cancelled by the other view as busy, cancelling, then idle", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		const started = { ...frame(2, { kind: "turnStarted" }, SESSION), turnId: "01932d5e-0000-7000-8000-0000000000a1" };
		acpStore.applyFrame(CONNECTION, started);
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({
			state: "prompting",
			activeTurn: { turnId: started.turnId, state: "running" },
		});

		acpStore.applyFrame(CONNECTION, frame(3, { kind: "attachmentState", state: "cancelling" }, SESSION));
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({
			state: "cancelling",
			activeTurn: { state: "cancelling" },
		});

		acpStore.applyFrame(CONNECTION, {
			...frame(4, { kind: "turnSettled", stopReason: "cancelled", usage: null }, SESSION),
			turnId: started.turnId,
		});
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({
			state: "idle",
			activeTurn: { state: "settled", stopReason: "cancelled" },
		});
	});
	it("shows a queued phone prompt from the server stream and removes it after another client cancels it", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ queuedPrompts: [] })] }));
		const queued = { turnId: "01932d5e-0000-7000-8000-0000000000b1", summary: "from phone" };
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "promptQueueChanged", queuedPrompts: [queued] }, SESSION));
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({ queuedPrompts: [queued] });

		acpStore.applyFrame(CONNECTION, frame(3, { kind: "promptQueueChanged", queuedPrompts: [] }, SESSION));
		expect(acpStore.attachment(CONNECTION, SESSION)).toMatchObject({ queuedPrompts: [] });
	});
	it("moves the connection to the state a connectionState frame reports", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "connectionState", state: "failed" }));

		expect(acpStore.connection(CONNECTION)?.state).toBe("failed");
	});

	// The event names no session; the envelope does. A store that read the
	// session off the event would move every attachment at once.
	it("moves only the attachment the envelope names", () => {
		const other = "01932d5e-0000-7000-8000-0000000000ab";
		acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: other })] }));
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "attachmentState", state: "prompting" }, SESSION));

		expect(acpStore.attachment(CONNECTION, SESSION)?.state).toBe("prompting");
		expect(acpStore.attachment(CONNECTION, other)?.state).toBe("idle");
	});
});

describe("acpStore: pending interactions", () => {
	it("holds a question until it is settled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyInteractions(CONNECTION, [permission("req-1")]);

		expect(acpStore.interactions(CONNECTION)).toHaveLength(1);

		acpStore.applyFrame(
			CONNECTION,
			frame(3, { kind: "permissionSettled", requestId: "req-1", outcome: { outcome: "cancelled" } }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION)).toEqual([]);
	});

	// A question may be answered by another client, or by a phone. The settle
	// frame is the only thing that retracts it, so it must retract it whether or
	// not this window is the one that answered.
	it("drops a question settled by somebody else", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(
			CONNECTION,
			frame(2, { kind: "permissionRequested", requestId: "req-2", request: permissionRequest() }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION).map((i) => i.requestId)).toEqual(["req-2"]);

		acpStore.applyFrame(
			CONNECTION,
			frame(
				3,
				{ kind: "permissionSettled", requestId: "req-2", outcome: { outcome: "selected", optionId: "allow" } },
				SESSION,
			),
		);

		expect(acpStore.interactions(CONNECTION)).toEqual([]);
	});

	it("does not add the same question twice", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyInteractions(CONNECTION, [permission("req-3")]);
		acpStore.applyFrame(
			CONNECTION,
			frame(4, { kind: "permissionRequested", requestId: "req-3", request: permissionRequest() }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION)).toHaveLength(1);
	});

	// acp_settled_question_does_not_survive_reconnect — catches: a re-adopt
	// (openConversation on a held connection) applies the snapshot and pending
	// list it fetched before the old stream delivered the settlement, so the
	// answered question and its awaiting state come back and nothing retracts
	// them again: the new stream resumes after the settlement frame.
	it("acp_settled_question_does_not_survive_reconnect", () => {
		const awaiting = {
			state: "awaiting" as const,
			humanWaits: [{ requestId: "req-9", kind: "permission" as const }],
			tools: [],
			children: [],
			interrupted: false,
			revision: 2,
		};
		const stale = snapshot({ attachments: [attachment({ state: "prompting", activity: awaiting })] });
		acpStore.applySnapshot(stale);
		acpStore.applyInteractions(CONNECTION, [permission("req-9")]);

		acpStore.applyFrame(
			CONNECTION,
			frame(
				10,
				{ kind: "permissionSettled", requestId: "req-9", outcome: { outcome: "selected", optionId: "allow" } },
				SESSION,
			),
		);
		acpStore.applyFrame(
			CONNECTION,
			frame(
				11,
				{ kind: "activityChanged", activity: { ...awaiting, state: "busy", humanWaits: [], revision: 3 } },
				SESSION,
			),
		);

		// The reconnect commits what it fetched before the settlement arrived.
		acpStore.applySnapshot(stale);
		acpStore.applyInteractions(CONNECTION, [permission("req-9")]);

		expect(acpStore.interactions(CONNECTION)).toEqual([]);
		expect(acpStore.attachment(CONNECTION, SESSION)?.activity).toMatchObject({
			state: "busy",
			humanWaits: [],
			revision: 3,
		});
	});
});

describe("acpStore: activity across a reattach", () => {
	// catches: a client that never refreshes keeping the old attachment's
	// revision, so every frame of the reattached session (revision from 0)
	// is refused and the chat shows stale activity for good.
	it("takes the reattached session's activity after a detach", () => {
		const busy = { state: "busy" as const, humanWaits: [], tools: [], children: [], interrupted: false, revision: 5 };
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting", activity: busy })] }));

		acpStore.applyFrame(CONNECTION, frame(2, { kind: "attachmentState", state: "detached" }, SESSION));
		acpStore.applyFrame(CONNECTION, frame(3, { kind: "attachmentState", state: "idle" }, SESSION));
		acpStore.applyFrame(CONNECTION, frame(4, { kind: "activityChanged", activity: { ...busy, revision: 1 } }, SESSION));

		expect(acpStore.attachment(CONNECTION, SESSION)?.activity?.revision).toBe(1);
	});
});

describe("acpStore: a gap is a state, not a dropped frame", () => {
	// A gap says the journal no longer holds the sequence asked for. Skipping
	// ahead and carrying on would leave the panel showing a conversation with a
	// hole in it and no way to know, which is the whole reason criterion 6 asks
	// for it to be surfaced.
	it("records the gap and stops treating the stream as live", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "sequence 3 is no longer held; the earliest is 40",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});

		expect(acpStore.gap(CONNECTION)?.code).toBe("stream_gap");
		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
	});

	// A gap is terminal for the subscription, and the producer sends nothing
	// after it. Advancing the cursor past a gap would make the next resume ask
	// for events that were never delivered.
	it("leaves the cursor where the last real frame left it", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "gone",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});

		expect(acpStore.cursor(CONNECTION)).toBe(2);
	});

	it("clears the gap when the connection is subscribed again", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "gone",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		acpStore.markStreaming(CONNECTION);

		expect(acpStore.gap(CONNECTION)).toBeNull();
		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});

	// Giving up on the stream is a third ending, and the one a person can undo.
	// Recording it as a gap would send them to recover a conversation that is
	// intact; leaving it as streaming would show a live panel reading nothing.
	it("stops the stream without recording a gap when the reader gives up", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.markStreaming(CONNECTION);
		acpStore.markStopped(CONNECTION);

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.gap(CONNECTION)).toBeNull();
		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
	});

	// `end` is the other terminal frame and means the opposite of a gap: the
	// stream finished with nothing missing. Reporting it as a gap would send a
	// person to recover a conversation that is intact.
	it("ends the stream without recording a gap", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.markStreaming(CONNECTION);
		acpStore.applyFrame(CONNECTION, { kind: "end" });

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.gap(CONNECTION)).toBeNull();
	});

	// `end` is a unit variant on the wire and names nobody, so the reader's own
	// id is the only thing that says which connection finished. Connections are
	// per repo root and coexist: ego exiting on root A used to put root B's
	// panel on "Not receiving updates" with a live stream behind it.
	it("ends only the stream that reported it", () => {
		const other = "01932d5e-0000-7000-8000-0000000000c2";
		acpStore.applySnapshot(snapshot());
		acpStore.applySnapshot(snapshot({ connectionId: other }));
		acpStore.markStreaming(CONNECTION);
		acpStore.markStreaming(other);

		acpStore.applyFrame(CONNECTION, { kind: "end" });

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.isStreaming(other)).toBe(true);
	});
});

describe("acpStore: frames for a connection it does not know", () => {
	// A frame naming a connection the store dropped is not an error to raise at
	// a person — a disconnect races the last frames in flight. It is an error to
	// resurrect the connection from, though: a half-built entry with no snapshot
	// behind it renders as a connection that does not exist.
	it("is ignored rather than creating a connection", () => {
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "turnStarted" }, SESSION));

		expect(acpStore.connection(CONNECTION)).toBeNull();
		expect(acpStore.cursor(CONNECTION)).toBe(0);
	});
});

describe("acpStore: ACP slash commands", () => {
	it("replaces the command offer only for its owner — catches dropped offers and stale commands after revocation", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: "sibling" })] }));
		acpStore.applyFrame(CONNECTION, frame(2, { kind: "sessionUpdate", update: recordedCommands }, SESSION));
		expect(acpStore.attachment(CONNECTION, SESSION)?.availableCommands).toEqual(recordedCommands.availableCommands);
		expect(acpStore.attachment(CONNECTION, "sibling")?.availableCommands ?? []).toEqual([]);
		acpStore.applyFrame(
			CONNECTION,
			frame(
				3,
				{ kind: "sessionUpdate", update: { sessionUpdate: "available_commands_update", availableCommands: [] } },
				SESSION,
			),
		);
		expect(acpStore.attachment(CONNECTION, SESSION)?.availableCommands).toEqual([]);
	});
	it("recovers command offers from snapshots — catches late subscribers losing the current menu", () => {
		acpStore.applySnapshot(
			snapshot({ attachments: [attachment({ availableCommands: recordedCommands.availableCommands })] }),
		);
		expect(acpStore.attachment(CONNECTION, SESSION)?.availableCommands).toEqual(recordedCommands.availableCommands);
	});
});

// Catches: v1 failure loses turn identity after activeTurn is cleared, or a replay overwrites a newer completion.
it("retains the latest completion identity without accepting a replayed older failure", () => {
	acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
	acpStore.applyFrame(CONNECTION, { ...frame(1, { kind: "turnStarted" }, SESSION), turnId: "turn-1" });
	const failure = {
		...frame(2, { kind: "turnFailed", message: "overflow", state: "idle" }, SESSION),
		turnId: "turn-1",
	};
	acpStore.applyFrame(CONNECTION, failure);
	expect(acpStore.attachment(CONNECTION, SESSION)?.activeTurn).toBeNull();
	expect(acpStore.turnOutcome(CONNECTION, SESSION)).toEqual({ turnId: "turn-1", event: failure.event });
	const settled = {
		...frame(4, { kind: "turnSettled", stopReason: "end_turn", usage: null }, SESSION),
		turnId: "turn-2",
	};
	acpStore.applyFrame(CONNECTION, settled);
	acpStore.applyFrame(CONNECTION, failure);
	expect(acpStore.turnOutcome(CONNECTION, SESSION)).toEqual({ turnId: "turn-2", event: settled.event });
	acpStore.applySnapshot(snapshot());
	expect(acpStore.turnOutcome(CONNECTION, SESSION)).toBeNull();
});
