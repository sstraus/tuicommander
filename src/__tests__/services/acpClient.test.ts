import { beforeEach, describe, expect, it, vi } from "vitest";

// The IPC boundary and the frame transport are the two edges of this module.
// Both are replaced here because what is under test is neither of them: it is
// what the client does with a cursor, a gap and a dropped socket.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import { createAcpClient } from "../../services/acpClient";
import type { AcpStreamHandle, AcpStreamOptions } from "../../services/acpStream";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpConnectionSnapshot, AcpStreamFrame } from "../../types/acp";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const ROOT = "/repo";

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

/** Every stream the client opened, in order, with the frames it was given. */
class FakeStreams {
	opened: AcpStreamOptions[] = [];
	closed = 0;
	private failNext = 0;

	readonly open = async (options: AcpStreamOptions): Promise<AcpStreamHandle> => {
		this.opened.push(options);
		if (this.failNext > 0) {
			this.failNext -= 1;
			throw new Error("the stream refused to open");
		}
		return { close: () => void (this.closed += 1) };
	};

	failOpens(times: number): void {
		this.failNext = times;
	}

	get last(): AcpStreamOptions {
		const last = this.opened.at(-1);
		if (!last) throw new Error("no stream was opened");
		return last;
	}

	/** The newest stream opened for one connection, for tests with several. */
	for(connectionId: string): AcpStreamOptions {
		const found = [...this.opened].reverse().find((options: AcpStreamOptions) => options.connectionId === connectionId);
		if (!found) throw new Error(`no stream was opened for ${connectionId}`);
		return found;
	}

	deliver(frame: AcpStreamFrame): void {
		this.last.onFrame(frame);
	}

	drop(): void {
		this.last.onDropped?.();
	}
}

function frame(sequence: number): Extract<AcpStreamFrame, { kind: "event" }> {
	return {
		kind: "event",
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId: SESSION,
		turnId: null,
		event: { kind: "turnStarted" },
	};
}

const GAP: AcpStreamFrame = {
	kind: "gap",
	code: "stream_gap",
	message: "sequence 3 is no longer held",
	connectionId: CONNECTION,
	sessionId: null,
	operation: null,
	retryable: false,
};

/** Answer each command with what its Rust counterpart returns. */
function answering(overrides: Record<string, unknown> = {}) {
	return (command: string) => {
		if (command in overrides) return Promise.resolve(overrides[command]);
		switch (command) {
			case "acp_connect":
			case "acp_reconnect":
			case "acp_connection_snapshot":
				return Promise.resolve(snapshot());
			case "acp_pending_interactions":
				return Promise.resolve([]);
			case "acp_session_new":
			case "acp_session_load":
				return Promise.resolve({ sessionId: SESSION, state: "idle", cwd: ROOT });
			default:
				return Promise.resolve(undefined);
		}
	};
}

let streams: FakeStreams;
let client: ReturnType<typeof createAcpClient>;

beforeEach(() => {
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(answering());
	acpStore.reset();
	acpTranscript.reset();
	streams = new FakeStreams();
	client = createAcpClient(streams.open);
});

describe("acpClient: opening a connection", () => {
	// Catches a desktop client dropping launch-only credential overrides.
	it("passes a supplied environment only on the requested connect", async () => {
		await client.connect(ROOT, { FOO: "bar" });
		expect(mockInvoke).toHaveBeenCalledWith("acp_connect", { root: ROOT, env: { FOO: "bar" } });
		mockInvoke.mockClear();
		await client.connect(ROOT);
		expect(mockInvoke).toHaveBeenCalledWith("acp_connect", { root: ROOT });
	});

	it("holds the snapshot and starts reading the journal from its beginning", async () => {
		await client.connect(ROOT);

		expect(mockInvoke).toHaveBeenCalledWith("acp_connect", { root: ROOT });
		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
		expect(streams.opened).toHaveLength(1);
		expect(streams.last.afterSequence).toBe(0);
		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});

	// A question raised before this window subscribed is still waiting for an
	// answer, and no frame will announce it again. Without this the panel shows
	// an idle session that is in fact blocked on a permission nobody can see.
	it("takes the questions that were already pending", async () => {
		mockInvoke.mockImplementation(
			answering({
				acp_pending_interactions: [
					{
						kind: "permission",
						requestId: "req-1",
						sessionId: SESSION,
						request: { sessionId: SESSION, toolCall: {}, options: [] },
					},
				],
			}),
		);

		await client.connect(ROOT);

		expect(acpStore.interactions(CONNECTION).map((i) => i.requestId)).toEqual(["req-1"]);
	});
});

describe("acpClient: a stream that stops", () => {
	// The socket dropping says nothing about the journal — the events are still
	// there. Resuming at the cursor is what makes a dropped connection invisible
	// to the person reading the panel.
	it("resumes one past the last sequence it handled", async () => {
		await client.connect(ROOT);
		streams.deliver(frame(4));
		streams.drop();
		await vi.waitFor(() => expect(streams.opened).toHaveLength(2));

		expect(streams.last.afterSequence).toBe(5);
	});

	// A gap is terminal and not recoverable by reading again: the journal no
	// longer holds what the cursor asks for, so resuming would ask for it
	// forever. The store makes it visible instead, and a person decides.
	it("does not resume after a gap", async () => {
		await client.connect(ROOT);
		streams.deliver(frame(2));
		streams.deliver(GAP);
		streams.drop();
		await vi.waitFor(() => expect(acpStore.gap(CONNECTION)?.code).toBe("stream_gap"));

		expect(streams.opened).toHaveLength(1);
		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
	});

	// `end` means the journal finished with nothing missing — a settled
	// connection. Resuming would reopen a stream that immediately ends again.
	it("does not resume after the stream ended", async () => {
		await client.connect(ROOT);
		streams.deliver({ kind: "end" });
		streams.drop();

		expect(streams.opened).toHaveLength(1);
	});

	// A backend that is gone answers every open the same way. Retrying without
	// a bound turns one dead connection into a spin; stopping leaves a state the
	// panel can render and a person can act on.
	it("gives up after a bounded number of failed resumes", async () => {
		await client.connect(ROOT);
		streams.failOpens(10);
		streams.drop();
		await vi.waitFor(() => expect(acpStore.isStreaming(CONNECTION)).toBe(false));

		expect(streams.opened.length).toBeLessThanOrEqual(4);
	});

	// A frame is proof the stream works, so the budget it spent getting there is
	// not owed by the next drop. Without this, a connection that is merely
	// long-lived eventually exhausts a budget that was never about it.
	it("gives the budget back once a frame arrives", async () => {
		await client.connect(ROOT);
		streams.failOpens(2);
		streams.drop();
		await vi.waitFor(() => expect(streams.opened.length).toBeGreaterThan(2));
		streams.deliver(frame(9));
		streams.failOpens(2);
		streams.drop();
		await vi.waitFor(() => expect(streams.opened.length).toBeGreaterThan(5));

		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});
});

describe("acpClient: talking to a session", () => {
	it("shows a queued prompt only in the queue, then in both transcripts when the agent receives it", async () => {
		await client.connect(ROOT);
		mockInvoke.mockImplementation((command: string) =>
			command === "acp_session_prompt" ? Promise.resolve("phone-queued") : answering()(command),
		);
		await client.prompt(CONNECTION, SESSION, "from phone");
		expect(acpTranscript.entries(SESSION)).toEqual([]);
		streams.deliver({ ...frame(2), turnId: "phone-queued", event: { kind: "promptSent", text: "from phone" } });
		expect(acpTranscript.entries(SESSION)).toEqual([expect.objectContaining({ kind: "user", text: "from phone" })]);
	});
	// Catches: a caller bypasses the composer and sends an image to an incapable agent.
	it("refuses image blocks at the shared client boundary without image capability", async () => {
		await client.connect(ROOT);
		await expect(
			client.prompt(CONNECTION, SESSION, "look", [{ type: "image", mimeType: "image/png", data: "iVBORw==" }]),
		).rejects.toThrow("does not support images");
		expect(mockInvoke.mock.calls.some(([command]) => command === "acp_session_prompt")).toBe(false);
	});

	// Catches: the IPC adapter silently strips the image block from an image-only turn.
	it("sends a base64 image content block in an image-only prompt", async () => {
		const capable = snapshot({
			capabilities: {
				protocol: 1,
				load: true,
				list: true,
				resume: true,
				fork: false,
				delete: false,
				close: true,
				additionalDirectories: true,
				promptImage: true,
				promptAudio: false,
				promptEmbeddedContext: false,
				mcpStdio: false,
				mcpHttp: true,
				mcpSse: false,
				mcpAcp: true,
				clientFormElicitation: true,
				clientBooleanConfig: false,
				egoHoldVersion: null,
				egoCompactVersion: null,
			},
		});
		mockInvoke.mockImplementation(answering({ acp_connect: capable }));
		await client.connect(ROOT);
		await client.prompt(CONNECTION, SESSION, "", [{ type: "image", mimeType: "image/png", data: "iVBORw==" }]);

		expect(mockInvoke).toHaveBeenCalledWith("acp_session_prompt", {
			connectionId: CONNECTION,
			sessionId: SESSION,
			prompt: [{ type: "image", mimeType: "image/png", data: "iVBORw==" }],
			viewedRepo: null,
		});
		streams.deliver({ ...frame(2), event: { kind: "promptSent", text: "Image" } });
		expect(acpTranscript.entries(SESSION)).toEqual([expect.objectContaining({ kind: "user", text: "Image" })]);
	});

	it("sends a prompt as one text content block, with the viewed repository beside it", async () => {
		await client.connect(ROOT);
		await client.prompt(CONNECTION, SESSION, "hello", [], "/repo/viewed");

		expect(mockInvoke).toHaveBeenCalledWith("acp_session_prompt", {
			connectionId: CONNECTION,
			sessionId: SESSION,
			prompt: [{ type: "text", text: "hello" }],
			viewedRepo: "/repo/viewed",
		});
	});

	it("gives ego a file resource link and readable path for a mobile document", async () => {
		await client.connect(ROOT);
		await client.prompt(CONNECTION, SESSION, "Summarize this", [], null, [
			{ name: "report.pdf", path: "/repo/.tuic/attachments/1-report.pdf" },
		]);
		expect(mockInvoke).toHaveBeenCalledWith("acp_session_prompt", {
			connectionId: CONNECTION,
			sessionId: SESSION,
			prompt: [
				{ type: "text", text: "Summarize this\n\n@/repo/.tuic/attachments/1-report.pdf" },
				{ type: "resource_link", uri: "file:///repo/.tuic/attachments/1-report.pdf", name: "report.pdf" },
			],
			viewedRepo: null,
		});
	});

	it("encodes a Windows attachment path as a file resource URI", async () => {
		await client.connect(ROOT);
		await client.prompt(CONNECTION, SESSION, "", [], null, [
			{ name: "a #1.txt", path: "C:\\repo\\.tuic\\attachments\\1-a #1.txt" },
		]);
		expect(mockInvoke).toHaveBeenCalledWith(
			"acp_session_prompt",
			expect.objectContaining({
				prompt: expect.arrayContaining([
					{ type: "resource_link", uri: "file:///C:/repo/.tuic/attachments/1-a%20%231.txt", name: "a #1.txt" },
				]),
			}),
		);
	});

	// A refused request never emits promptSent, so no view can display it as sent.
	it("does not show a message when the prompt is refused", async () => {
		await client.connect(ROOT);
		mockInvoke.mockImplementation((command: string) =>
			command === "acp_session_prompt"
				? Promise.reject(new Error("the session is not accepting prompts"))
				: answering()(command),
		);

		await expect(client.prompt(CONNECTION, SESSION, "hello")).rejects.toThrow("not accepting prompts");

		expect(acpTranscript.entries(SESSION)).toEqual([]);
	});

	it("shows the message when the server reports it reached the agent", async () => {
		await client.connect(ROOT);

		await client.prompt(CONNECTION, SESSION, "hello");
		streams.deliver({ ...frame(2), event: { kind: "promptSent", text: "hello" } });

		expect(acpTranscript.entries(SESSION)).toEqual([expect.objectContaining({ kind: "user", text: "hello" })]);
	});

	// The clear has to come first — `session/load` replays the whole history, so
	// without it every message doubles — which means a load that is refused
	// leaves the panel live on a session whose conversation has been erased.
	it("puts the transcript back when a session load is refused", async () => {
		await client.connect(ROOT);
		acpTranscript.noteUserMessage(SESSION, "what was said before");
		mockInvoke.mockImplementation((command: string) =>
			command === "acp_session_load" ? Promise.reject(new Error("no such session")) : answering()(command),
		);

		await expect(client.loadSession(CONNECTION, SESSION, ROOT)).rejects.toThrow("no such session");

		expect(acpTranscript.entries(SESSION)).toEqual([
			expect.objectContaining({ kind: "user", text: "what was said before" }),
		]);
	});

	it("clears the transcript for a load that succeeds, so the replay does not double it", async () => {
		await client.connect(ROOT);
		acpTranscript.noteUserMessage(SESSION, "what was said before");

		await client.loadSession(CONNECTION, SESSION, ROOT);

		expect(acpTranscript.entries(SESSION)).toEqual([]);
	});

	// Shape recorded from ego session 01a0dc79 (journal seq 4-53, ~/.ego/sessions):
	// one finished glob, then an exec that asked for permission and was never
	// decided — ToolCallStarted + PermissionRequested, no ToolCallCompleted, no
	// TurnCompleted. ego's replay (serve.rs emit_attachment) projects it as
	// tool_call, in_progress and ends with a session_info_update.
	describe("replay of a session that ended mid-call", () => {
		const update = (sequence: number, body: Record<string, unknown>) =>
			streams.deliver({ ...frame(sequence), event: { kind: "sessionUpdate", update: body as never } });
		const statuses = () =>
			acpTranscript.entries(SESSION).flatMap((entry) => (entry.kind === "tool" ? [entry.call.status] : []));
		const replay = async () => {
			await client.connect(ROOT);
			await client.loadSession(CONNECTION, SESSION, ROOT);
			update(2, { sessionUpdate: "tool_call", toolCallId: "glob", title: "glob", status: "pending" });
			update(3, { sessionUpdate: "tool_call_update", toolCallId: "glob", status: "in_progress" });
			update(4, { sessionUpdate: "tool_call_update", toolCallId: "glob", status: "completed" });
			update(5, { sessionUpdate: "tool_call", toolCallId: "exec", title: "exec", status: "pending" });
			update(6, { sessionUpdate: "tool_call_update", toolCallId: "exec", status: "in_progress" });
		};

		// Catches: the replayed exec card keeps status in_progress forever, so its
		// dot pulses (candidate 1 of story 1153).
		it("settles the unfinished call when the replay ends, and only then", async () => {
			await replay();
			expect(statuses()).toEqual(["completed", "in_progress"]);

			update(7, { sessionUpdate: "session_info_update", title: null });

			expect(statuses()).toEqual(["completed", "failed"]);
		});

		// ego does not say on session/load whether a turn is still running, so the
		// settle can hit a live call. Catches: a settled status that sticks over
		// live data (failed instead of the call's real outcome).
		it("lets a later update of the same call win over the settled status", async () => {
			await replay();
			update(7, { sessionUpdate: "session_info_update", title: null });

			update(8, { sessionUpdate: "tool_call_update", toolCallId: "exec", status: "in_progress" });
			expect(statuses()).toEqual(["completed", "in_progress"]);
			update(9, { sessionUpdate: "tool_call_update", toolCallId: "exec", status: "completed" });
			expect(statuses()).toEqual(["completed", "completed"]);
		});

		// Catches: a title update of a running session, long after the load,
		// failing its live calls.
		it("settles once per load", async () => {
			await replay();
			update(7, { sessionUpdate: "session_info_update", title: null });
			update(8, { sessionUpdate: "tool_call_update", toolCallId: "exec", status: "in_progress" });

			update(9, { sessionUpdate: "session_info_update", title: "renamed" });

			expect(statuses()).toEqual(["completed", "in_progress"]);
		});
	});

	// ego does not replay inherited history for a fork child, so the child's
	// transcript is the parent's copy. The parent's own must stay untouched: it
	// is still a live tab.
	it("forks into a child that carries the parent's transcript and leaves the parent's intact", async () => {
		await client.connect(ROOT);
		acpTranscript.noteUserMessage(SESSION, "what was said before");
		const CHILD = "01932d5e-0000-7000-8000-0000000000dd";
		mockInvoke.mockImplementation(answering({ acp_session_fork: { sessionId: CHILD, state: "idle", cwd: ROOT } }));

		const forked = await client.forkSession(CONNECTION, SESSION, ROOT);

		expect(forked).toBe(CHILD);
		expect(mockInvoke).toHaveBeenCalledWith("acp_session_fork", {
			connectionId: CONNECTION,
			sessionId: SESSION,
			authority: { cwd: ROOT, additionalDirectories: [] },
		});
		const said = [expect.objectContaining({ kind: "user", text: "what was said before" })];
		expect(acpTranscript.entries(CHILD)).toEqual(said);
		expect(acpTranscript.entries(SESSION)).toEqual(said);
	});

	// The ids are the agent's. Answering with an Allow/Deny of this client's own
	// invention would answer a question nobody asked.
	it("answers a permission with one of the option ids the agent published", async () => {
		await client.connect(ROOT);
		await client.answerPermission(CONNECTION, "req-1", "allow_once_id");

		expect(mockInvoke).toHaveBeenCalledWith("acp_respond_permission", {
			connectionId: CONNECTION,
			requestId: "req-1",
			outcome: { outcome: "selected", optionId: "allow_once_id" },
		});
	});

	it("cancels a permission without naming an option", async () => {
		await client.connect(ROOT);
		await client.cancelPermission(CONNECTION, "req-1");

		expect(mockInvoke).toHaveBeenCalledWith("acp_respond_permission", {
			connectionId: CONNECTION,
			requestId: "req-1",
			outcome: { outcome: "cancelled" },
		});
	});

	// The session is opened against the root the connection runs on, and the
	// server decides what it may reach beyond that: a caller naming an MCP
	// server is refused by the backend, so the client never sends one.
	it("opens a session on the connection's own root and names nothing else", async () => {
		await client.connect(ROOT);
		await client.newSession(CONNECTION, ROOT);

		expect(mockInvoke).toHaveBeenCalledWith("acp_session_new", {
			connectionId: CONNECTION,
			authority: { cwd: ROOT, additionalDirectories: [] },
		});
	});

	// Pause, resume and compact are idempotency-keyed by a request id the client
	// mints. A backend that receives the same one twice answers once.
	it("mints a uuid request id for a hold", async () => {
		await client.connect(ROOT);
		await client.pause(CONNECTION, SESSION);

		const call = mockInvoke.mock.calls.find(([command]) => command === "acp_turn_pause");
		expect(call?.[1].requestId).toMatch(/^[0-9a-f-]{36}$/);
	});
});

describe("acpClient: connections that coexist", () => {
	/** A second connection, the way a second repo root gets one. */
	const OTHER = "01932d5e-0000-7000-8000-0000000000c2";
	const OTHER_ROOT = "/other-repo";

	/** Answer `acp_connect` with whichever root was asked for. */
	function answeringBothRoots() {
		return (command: string, args?: Record<string, unknown>) => {
			if (command === "acp_connect") {
				return Promise.resolve(args?.root === OTHER_ROOT ? snapshot({ connectionId: OTHER }) : snapshot());
			}
			return answering()(command);
		};
	}

	// `end` is a unit variant on the wire, so the frame names nobody. Reading it
	// as "everything stopped" put the other root's panel on "Not receiving
	// updates" while its stream was still live and still delivering.
	it("stops streaming only on the connection whose stream ended", async () => {
		mockInvoke.mockImplementation(answeringBothRoots());
		await client.connect(ROOT);
		await client.connect(OTHER_ROOT);
		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
		expect(acpStore.isStreaming(OTHER)).toBe(true);

		streams.for(CONNECTION).onFrame({ kind: "end" });

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.isStreaming(OTHER)).toBe(true);
	});
});

describe("acpClient: replacing a connection", () => {
	/** The id `acp_reconnect` mints for the replacement process. */
	const FRESH = "01932d5e-0000-7000-8000-0000000000c3";

	function answeringReconnect() {
		return (command: string) =>
			command === "acp_reconnect" ? Promise.resolve(snapshot({ connectionId: FRESH })) : answering()(command);
	}

	// The backend mints a new id, so the old entry is not overwritten — it is
	// orphaned. Left in the store it shows in the connection list as a
	// connection nobody can reach, and it leaks one per recover().
	it("leaves no entry behind for the connection it replaced", async () => {
		await client.connect(ROOT);
		mockInvoke.mockImplementation(answeringReconnect());

		await client.reconnect(CONNECTION, ROOT);

		expect(acpStore.connection(CONNECTION)).toBeNull();
		expect(acpStore.connectionIds()).toEqual([FRESH]);
		expect(acpStore.isStreaming(FRESH)).toBe(true);
	});

	// And the old stream is closed rather than left reading. This is the second
	// half of the same defect: a stream nobody forgot goes on to deliver the
	// dead connection's `end`, which used to freeze the fresh panel.
	it("closes the replaced connection's stream", async () => {
		await client.connect(ROOT);
		mockInvoke.mockImplementation(answeringReconnect());

		await client.reconnect(CONNECTION, ROOT);
		streams.for(CONNECTION).onFrame({ kind: "end" });

		expect(streams.closed).toBe(1);
		expect(acpStore.isStreaming(FRESH)).toBe(true);
	});
});

describe("acpClient: a connection that does not finish arriving", () => {
	// `acp_pending_interactions` is a round trip to a process that has only just
	// started. A failure used to leave the store holding a connection with a
	// null handle and no stream — present in the list, reading as idle, and
	// never going to receive anything.
	it("holds nothing when the pending fetch fails", async () => {
		mockInvoke.mockImplementation((command: string) =>
			command === "acp_pending_interactions"
				? Promise.reject(new Error("the connection went away"))
				: answering()(command),
		);

		await expect(client.connect(ROOT)).rejects.toThrow("the connection went away");

		expect(acpStore.connection(CONNECTION)).toBeNull();
		expect(streams.opened).toHaveLength(0);
	});

	// The same for the other fallible half. The store is written before the
	// stream opens on purpose — a frame can arrive the instant the socket is up
	// — so the rollback is what keeps that safe.
	it("holds nothing when the stream refuses to open", async () => {
		streams.failOpens(1);

		await expect(client.connect(ROOT)).rejects.toThrow("the stream refused to open");

		expect(acpStore.connection(CONNECTION)).toBeNull();
		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
	});
});

describe("acpClient: letting go", () => {
	it("closes the stream and forgets the connection", async () => {
		await client.connect(ROOT);
		await client.disconnect(CONNECTION);

		expect(mockInvoke).toHaveBeenCalledWith("acp_disconnect", { connectionId: CONNECTION });
		expect(streams.closed).toBe(1);
		expect(acpStore.connection(CONNECTION)).toBeNull();
	});

	// A connection that is gone must not be resumed by the drop its own
	// teardown causes, which arrives after the close.
	it("does not resume a stream it closed itself", async () => {
		await client.connect(ROOT);
		await client.disconnect(CONNECTION);
		streams.drop();

		expect(streams.opened).toHaveLength(1);
	});
});

// Catches: atMessageId dropped and the child showing all four replies after a mid-history fork.
it("forks from the second of four replies without changing the parent", async () => {
	const streams = new FakeStreams();
	const client = createAcpClient(streams.open);
	await client.connect(ROOT);
	for (let index = 1; index <= 4; index++) {
		acpTranscript.noteUserMessage(SESSION, `prompt ${index}`);
		acpTranscript.applyFrame({
			...frame(index),
			event: {
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "agent_message_chunk",
					messageId: `reply-${index}`,
					content: { type: "text", text: `reply ${index}` },
				},
			},
		});
	}
	const child = "child";
	mockInvoke.mockImplementation(answering({ acp_session_fork: { sessionId: child } }));
	await client.forkSession(CONNECTION, SESSION, ROOT, "reply-2");
	expect(mockInvoke).toHaveBeenCalledWith("acp_session_fork", {
		connectionId: CONNECTION,
		sessionId: SESSION,
		authority: { cwd: ROOT, additionalDirectories: [] },
		atMessageId: "reply-2",
	});
	expect(acpTranscript.entries(child).map((entry) => ("text" in entry ? entry.text : ""))).toEqual([
		"prompt 1",
		"reply 1",
		"prompt 2",
		"reply 2",
	]);
	expect(acpTranscript.entries(SESSION)).toHaveLength(8);
});

describe("configured conversation replay", () => {
	const launch = {
		executable: "/bin/ego",
		profile: "coordinator",
		masterPrompt: "",
		workspace: "/observer",
		peerId: "peer",
	};
	// Catches: reopening appends replayed history to the old projection instead of replacing it.
	it("clears the old projection for a replay but retains it when already attached", async () => {
		acpTranscript.noteUserMessage(SESSION, "old projection");
		mockInvoke.mockImplementation(
			answering({ acp_chat_open: { connection: snapshot(), sessionId: SESSION, launch, replayed: true } }),
		);
		await client.openConversation({ sessionId: SESSION });
		expect(acpTranscript.entries(SESSION)).toEqual([]);
		acpTranscript.noteUserMessage(SESSION, "current projection");
		mockInvoke.mockImplementation(
			answering({ acp_chat_open: { connection: snapshot(), sessionId: SESSION, launch, replayed: false } }),
		);
		await client.openConversation({ sessionId: SESSION });
		expect(acpTranscript.entries(SESSION).map((entry) => (entry.kind === "user" ? entry.text : ""))).toEqual([
			"current projection",
		]);
	});
	// Catches: a failed custom conversation reopen destroys the transcript a user was reading.
	it("restores the projection when the saved launch is refused", async () => {
		acpTranscript.noteUserMessage(SESSION, "keep this history");
		mockInvoke.mockRejectedValueOnce(new Error("launch refused"));
		await expect(client.openConversation({ sessionId: SESSION })).rejects.toThrow("launch refused");
		expect(acpTranscript.entries(SESSION).map((entry) => (entry.kind === "user" ? entry.text : ""))).toEqual([
			"keep this history",
		]);
	});
});

// Catches: switching back to an attached custom chat erases response chunks received during the open request.
it("retains live response chunks while reopening an already attached custom conversation", async () => {
	await client.connect(ROOT);
	acpTranscript.noteUserMessage(SESSION, "question before switching tabs");
	let finishOpen!: (value: unknown) => void;
	const pendingOpen = new Promise<unknown>((resolve) => {
		finishOpen = resolve;
	});
	mockInvoke.mockImplementation(answering({ acp_chat_open: pendingOpen }));
	const reopening = client.openConversation({ sessionId: SESSION });
	streams.deliver({
		...frame(2),
		event: {
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "response arriving during tab switch" },
			},
		},
	});
	finishOpen({
		connection: snapshot({ latestSequence: 2 }),
		sessionId: SESSION,
		launch: {
			executable: "/bin/ego",
			profile: "coordinator",
			masterPrompt: "",
			workspace: "/observer",
			peerId: "peer",
		},
		replayed: false,
	});
	await reopening;
	expect(acpTranscript.entries(SESSION).map((entry) => ("text" in entry ? entry.text : ""))).toEqual([
		"question before switching tabs",
		"response arriving during tab switch",
	]);
});

// Catches: a rejected reopen restores stale history over chunks received on the live stream.
it("retains live response chunks when custom conversation reopen is refused", async () => {
	await client.connect(ROOT);
	acpTranscript.noteUserMessage(SESSION, "question before switching tabs");
	let refuseOpen!: (error: Error) => void;
	const pendingOpen = new Promise<unknown>((_resolve, reject) => {
		refuseOpen = reject;
	});
	mockInvoke.mockImplementation(answering({ acp_chat_open: pendingOpen }));
	const reopening = client.openConversation({ sessionId: SESSION });
	const rejected = expect(reopening).rejects.toThrow("launch refused");
	streams.deliver({
		...frame(2),
		event: {
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "response arriving during refused reopen" },
			},
		},
	});
	refuseOpen(new Error("launch refused"));
	await rejected;
	expect(acpTranscript.entries(SESSION).map((entry) => ("text" in entry ? entry.text : ""))).toEqual([
		"question before switching tabs",
		"response arriving during refused reopen",
	]);
});

// Catches: the delete UI accidentally issuing session/close, which retains durable history.
it("deletes durable history through the ACP delete command and refreshes attachments", async () => {
	const deletedAttachment = {
		sessionId: SESSION,
		state: "idle" as const,
		cwd: ROOT,
		additionalDirectories: [],
		configOptions: [],
		usage: null,
		activeTurn: null,
		queuedPrompts: [],
		pendingPermissionIds: [],
		pendingElicitationIds: [],
	};
	const survivor = { ...deletedAttachment, sessionId: "01932d5e-0000-7000-8000-0000000000bb" };
	acpStore.applySnapshot(snapshot({ attachments: [deletedAttachment, survivor] }));
	mockInvoke.mockImplementation(answering({ acp_connection_snapshot: snapshot({ attachments: [survivor] }) }));
	await client.deleteSession(CONNECTION, SESSION);
	expect(mockInvoke).toHaveBeenCalledWith("acp_session_delete", { connectionId: CONNECTION, sessionId: SESSION });
	expect(mockInvoke).toHaveBeenCalledWith("acp_connection_snapshot", { connectionId: CONNECTION });
	expect(acpStore.attachments(CONNECTION)).toEqual([survivor]);
	expect(acpStore.attachment(CONNECTION, SESSION)).toBeNull();
});

// Catches: a failed snapshot after accepted deletion leaves the deleted session open.
it("keeps accepted deletion successful when its following snapshot fails", async () => {
	const failure = vi.spyOn((await import("../../stores/appLogger")).appLogger, "warn").mockImplementation(() => {});
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === "acp_connection_snapshot") throw new Error("snapshot disconnected");
		return undefined;
	});
	try {
		await expect(createAcpClient().deleteSession(CONNECTION, SESSION)).resolves.toBeUndefined();
		expect(failure).toHaveBeenCalledWith("ai-chat", "refresh after conversation deletion failed", expect.any(Error));
	} finally {
		failure.mockRestore();
	}
});
