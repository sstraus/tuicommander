import { beforeEach, describe, expect, it, vi } from "vitest";

// Critic tests for the transient reasoning lifecycle. They drive the client the
// way the AI Chat panel does (desktop and mobile share `createAcpChat`, which
// reads `acpTranscript.reasoning`) and replace only the IPC and frame transport.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import { createAcpClient } from "../../services/acpClient";
import type { AcpStreamHandle, AcpStreamOptions } from "../../services/acpStream";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpClientEvent, AcpConnectionSnapshot, AcpStreamFrame } from "../../types/acp";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const OTHER_SESSION = "01932d5e-0000-7000-8000-0000000000bb";
const TURN = "01932d5e-0000-7000-8000-0000000000t1";
const NEXT_TURN = "01932d5e-0000-7000-8000-0000000000t2";
const ROOT = "/repo";
const LAUNCH = { executable: "/bin/ego", profile: "coordinator", masterPrompt: "", workspace: ROOT, peerId: "peer" };

function snapshot(): AcpConnectionSnapshot {
	return {
		connectionId: CONNECTION,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.6.0" },
		capabilities: null,
		attachments: [],
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
	};
}

let opened: AcpStreamOptions[];
let client: ReturnType<typeof createAcpClient>;
let sequence: number;

function open(options: AcpStreamOptions): Promise<AcpStreamHandle> {
	opened.push(options);
	return Promise.resolve({ close: () => undefined });
}

function deliver(
	event: AcpClientEvent,
	route: { sessionId?: string | null; turnId?: string | null; sequence?: number } = {},
): void {
	const at = route.sequence ?? ++sequence;
	const frame: AcpStreamFrame = {
		kind: "event",
		connectionId: CONNECTION,
		generation: 1,
		sequence: at,
		sessionId: route.sessionId === undefined ? SESSION : route.sessionId,
		turnId: route.turnId === undefined ? TURN : route.turnId,
		event,
	};
	const stream = opened.at(-1);
	if (!stream) throw new Error("no stream was opened");
	stream.onFrame(frame);
}

function thought(text: string): AcpClientEvent {
	return {
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text } },
	} as unknown as AcpClientEvent;
}

function answer(text: string): AcpClientEvent {
	return {
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text } },
	} as unknown as AcpClientEvent;
}

function dispatch(turnId = TURN, sessionId = SESSION): void {
	deliver({ kind: "promptSent", text: "question" } as AcpClientEvent, { turnId, sessionId });
	deliver({ kind: "turnStarted" } as AcpClientEvent, { turnId, sessionId });
}

beforeEach(async () => {
	mockInvoke.mockReset();
	mockInvoke.mockImplementation((command: string) => {
		switch (command) {
			case "acp_connect":
				return Promise.resolve(snapshot());
			case "acp_chat_open":
				return Promise.resolve({ connection: snapshot(), sessionId: SESSION, launch: LAUNCH, replayed: false });
			case "acp_pending_interactions":
				return Promise.resolve([]);
			default:
				return Promise.resolve(undefined);
		}
	});
	acpStore.reset();
	acpTranscript.reset();
	opened = [];
	sequence = 0;
	client = createAcpClient(open);
	await client.connect(ROOT);
});

describe("transient reasoning across connection and turn boundaries", () => {
	// Catches: ego exiting mid-turn ends the turn only through a connection-level
	// `connectionState` frame (no session, no turn, no `turnFailed`, no `end`),
	// so the panel keeps showing "Processing…" or a stale thought for a turn
	// that can never finish.
	it("stops showing a dead connection's reasoning when ego exits mid-turn", () => {
		dispatch();
		deliver(thought("Reading the parser"));
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("Reading the parser");

		deliver({ kind: "connectionState", state: "failed" } as AcpClientEvent, { sessionId: null, turnId: null });

		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBeNull();
		expect(acpTranscript.entries(SESSION).some((entry) => entry.kind === "thought")).toBe(true);
	});

	// Catches: re-adopting the connection a running conversation already uses
	// (reopening its tab: the host hands back the same peer connection) is
	// treated as teardown and seals the live turn, so its later thoughts never show.
	it("keeps a running turn's reasoning when its conversation is reopened on the same connection", async () => {
		dispatch();
		deliver(thought("First step. "));

		await client.openConversation({ sessionId: SESSION });
		deliver(thought("Second step."));

		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("First step. Second step.");
	});

	// Catches: a prompt queued behind a running turn shows "Processing…" before
	// the host dispatches it, or a late thought of the previous turn leaks into it.
	it("shows the placeholder only on dispatch and never mixes the previous turn's late thoughts", () => {
		dispatch();
		deliver(answer("Done."));
		deliver({ kind: "promptQueueChanged", queuedPrompts: [{ turnId: NEXT_TURN, summary: "next" }] } as AcpClientEvent, {
			turnId: NEXT_TURN,
		});
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBeNull();

		deliver({ kind: "turnSettled", stopReason: "end_turn", usage: null } as AcpClientEvent);
		dispatch(NEXT_TURN);
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("");

		deliver(thought("stale"), { turnId: TURN });
		deliver(thought("fresh"), { turnId: NEXT_TURN });
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("fresh");
	});

	// Catches: a duplicated earlier frame after a resume re-dispatches the turn and
	// wipes the reasoning already shown, or a settled turn is revived by it.
	it("ignores re-delivered frames of a turn already shown or settled", () => {
		dispatch();
		deliver(thought("kept"));
		deliver({ kind: "promptSent", text: "question" } as AcpClientEvent, { sequence: 1 });
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("kept");

		deliver({ kind: "turnSettled", stopReason: "cancelled", usage: null } as AcpClientEvent);
		deliver({ kind: "promptSent", text: "question" } as AcpClientEvent);
		deliver(thought("revived"));
		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBeNull();
	});

	// Catches: reasoning keyed by connection or by the last turn alone, so a
	// session switch shows another conversation's thoughts.
	it("never shows one session's reasoning in another session on the same connection", () => {
		dispatch(TURN, SESSION);
		dispatch(NEXT_TURN, OTHER_SESSION);
		deliver(thought("about A"), { turnId: TURN, sessionId: SESSION });
		deliver(thought("about B"), { turnId: NEXT_TURN, sessionId: OTHER_SESSION });

		expect(acpTranscript.reasoning(SESSION, CONNECTION)).toBe("about A");
		expect(acpTranscript.reasoning(OTHER_SESSION, CONNECTION)).toBe("about B");
	});
});
