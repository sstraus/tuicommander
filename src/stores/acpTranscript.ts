import { createStore, produce, unwrap } from "solid-js/store";
import type {
	AcpPlanEntry,
	AcpSessionId,
	AcpSessionUpdate,
	AcpStreamFrame,
	AcpToolCall,
	AcpToolCallContent,
	AcpToolCallLocation,
} from "../types/acp";

/**
 * The conversation, as a person reads it.
 *
 * Kept apart from `acpStore` on purpose. That store holds protocol state — what
 * the connection is, where the cursor sits, which questions are open — and is
 * pinned field-for-field to the wire. This one holds a projection built for
 * rendering: chunks joined into a message, a tool call and its later updates
 * folded into one card, a plan replaced rather than appended. The two change at
 * different rates and for different reasons, and folding them together would
 * make every streamed character touch the object the connection list reads.
 *
 * Keyed by session rather than by connection: a session outlives the connection
 * that opened it — ego owns it, and reconnecting attaches to the same id — so a
 * transcript filed under the connection would be thrown away by a reconnect
 * that changed nothing a person can see.
 */

/**
 * What a card's button does, as ego's notice carries it in `_meta.ego.action`.
 *
 * Mirrors `NoticeAction` in ego-acp `project.rs`: the tag is `kind`, the payload
 * keys are snake_case on the wire. An action this client does not know is
 * dropped, leaving a card with text and no button.
 */
export type AcpNoticeAction =
	| { kind: "open_result"; path: string }
	| { kind: "answer"; questionId: string }
	| { kind: "approve"; requestId: string };

export type AcpTranscriptEntry = { messageId?: string; inherited?: boolean } & (
	| { id: string; kind: "user"; text: string; pending?: boolean }
	| { id: string; kind: "agent"; text: string }
	| { id: string; kind: "thought"; text: string }
	| { id: string; kind: "tool"; call: AcpToolCall }
	| { id: string; kind: "plan"; entries: AcpPlanEntry[] }
	/** An ego notice (`_meta.ego.salience = "card"`), shown apart from the agent's prose. */
	| { id: string; kind: "notice"; text: string; action?: AcpNoticeAction }
	/** A turn that ended as something other than a finished answer. */
	| { id: string; kind: "settled"; stopReason: string }
	| { id: string; kind: "failed"; message: string }
);

export interface AcpProviderRetry {
	text: string;
	/** Presentation deadline measured when the update arrived, never from panel mount. */
	retryAt?: number;
}

interface TranscriptState {
	sessions: Record<AcpSessionId, AcpTranscriptEntry[]>;
	titles: Record<AcpSessionId, string>;
	usage: Record<AcpSessionId, { used: number; size: number; cost?: { amount: number; currency: string } }>;
	turnHasReply: Record<AcpSessionId, boolean>;
	pendingUserEcho: Record<AcpSessionId, { entryId: string; received: string }>;
	/** ego's provider-retry status while a turn waits to retry (`_meta.ego.providerRetry`). */
	retries: Record<AcpSessionId, AcpProviderRetry>;
	/** Next entry id. Monotonic across sessions; only distinctness matters. */
	nextId: number;
}

const [state, setState] = createStore<TranscriptState>({
	sessions: {},
	titles: {},
	usage: {},
	turnHasReply: {},
	pendingUserEcho: {},
	retries: {},
	nextId: 1,
});

/**
 * The text inside a content block, or "" for a block that carries none.
 *
 * An image or an embedded resource is not rendered as a message today, and
 * rendering its JSON instead would put a wall of base64 in the conversation.
 */
function textOf(content: unknown): string {
	if (content && typeof content === "object" && (content as { type?: string }).type === "text") {
		return String((content as { text?: unknown }).text ?? "");
	}
	return "";
}

function noticeAction(raw: unknown): AcpNoticeAction | undefined {
	const action = raw as { kind?: unknown; path?: unknown; question_id?: unknown; request_id?: unknown } | null;
	if (!action || typeof action !== "object") return undefined;
	if (action.kind === "open_result" && typeof action.path === "string" && action.path)
		return { kind: "open_result", path: action.path };
	if (action.kind === "answer" && typeof action.question_id === "string" && action.question_id)
		return { kind: "answer", questionId: action.question_id };
	if (action.kind === "approve" && typeof action.request_id === "string" && action.request_id)
		return { kind: "approve", requestId: action.request_id };
	return undefined;
}

/** The `_meta.ego` object of an update, when it has one. */
function egoMeta(
	record: Record<string, unknown>,
): { salience?: unknown; action?: unknown; inherited?: unknown } | undefined {
	const ego = (record._meta as { ego?: unknown } | null | undefined)?.ego;
	return ego && typeof ego === "object"
		? (ego as { salience?: unknown; action?: unknown; inherited?: unknown })
		: undefined;
}

/**
 * Append text to the last entry when it is the same kind, or start a new one.
 *
 * Chunks are how a turn streams: one word per frame is normal. Kept as separate
 * entries they render as one word per bubble, and joining them at render time
 * would mean re-deriving the grouping on every frame.
 */
function appendChunk(
	draft: TranscriptState,
	entries: AcpTranscriptEntry[],
	kind: "user" | "agent" | "thought",
	text: string,
	messageId?: string,
	inherited?: boolean,
): void {
	if (!text) return;
	const last = entries.at(-1);
	if (
		last?.kind === kind &&
		last.inherited === inherited &&
		(!messageId || !last.messageId || last.messageId === messageId)
	) {
		last.text += text;
		if (messageId) last.messageId = messageId;
		return;
	}
	entries.push({
		id: `e${draft.nextId}`,
		kind,
		text,
		...(messageId ? { messageId } : {}),
		...(inherited ? { inherited } : {}),
	});
	draft.nextId += 1;
}

/** Reconcile ego's streamed echo with the prompt already shown by promptSent. */
function appendUserChunk(
	draft: TranscriptState,
	sessionId: AcpSessionId,
	entries: AcpTranscriptEntry[],
	text: string,
	messageId?: string,
	inherited?: boolean,
): void {
	if (!text) return;
	const pending = draft.pendingUserEcho[sessionId];
	if (pending) {
		const entry = entries.find((item) => item.id === pending.entryId);
		if (entry?.kind === "user") {
			const received = pending.received + text;
			if (entry.text.startsWith(received)) {
				if (received === entry.text) delete draft.pendingUserEcho[sessionId];
				else pending.received = received;
				return;
			}
			entry.text = received;
			delete draft.pendingUserEcho[sessionId];
			return;
		}
		delete draft.pendingUserEcho[sessionId];
	}
	appendChunk(draft, entries, "user", text, messageId, inherited);
}

/** Fold a tool call, or an update to one, into the single card that shows it. */
function foldToolCall(draft: TranscriptState, entries: AcpTranscriptEntry[], update: Record<string, unknown>): void {
	const toolCallId = String(update.toolCallId ?? "");
	if (!toolCallId) return;
	const existing = entries.find((entry) => entry.kind === "tool" && entry.call.toolCallId === toolCallId);
	const fields: Partial<AcpToolCall> = {
		title: update.title as string | undefined,
		kind: update.kind as AcpToolCall["kind"],
		status: update.status as AcpToolCall["status"],
		content: update.content as AcpToolCallContent[] | undefined,
		locations: update.locations as AcpToolCallLocation[] | undefined,
		rawInput: update.rawInput,
		rawOutput: update.rawOutput,
	};
	if (existing?.kind === "tool") {
		// An update names only what changed. Copying the undefined ones over
		// would erase a title and a location list the first frame carried.
		for (const [key, value] of Object.entries(fields)) {
			if (value !== undefined) (existing.call as unknown as Record<string, unknown>)[key] = value;
		}
		return;
	}
	entries.push({
		id: `e${draft.nextId}`,
		kind: "tool",
		// A title is required by the protocol on the first frame, but an update
		// for a call this client never saw — a session loaded mid-flight — has
		// none, and an empty card is better than a dropped one.
		call: { toolCallId, title: (fields.title as string) ?? toolCallId, ...stripUndefined(fields) },
	});
	draft.nextId += 1;
}

function stripUndefined(fields: Partial<AcpToolCall>): Partial<AcpToolCall> {
	return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
}

/** A terminal turn cannot leave its tool indicators showing work in progress. */
function settleToolCalls(entries: AcpTranscriptEntry[], status: "completed" | "failed"): void {
	for (const entry of entries) {
		if (
			entry.kind === "tool" &&
			(!entry.call.status || entry.call.status === "pending" || entry.call.status === "in_progress")
		) {
			entry.call.status = status;
		}
	}
}

function reduceUpdate(
	draft: TranscriptState,
	sessionId: AcpSessionId,
	entries: AcpTranscriptEntry[],
	update: AcpSessionUpdate,
): void {
	const record = update as unknown as Record<string, unknown>;
	switch (update.sessionUpdate) {
		case "session_info_update": {
			if (typeof record.title === "string" && record.title.trim()) draft.titles[sessionId] = record.title;
			// A patch: only a present key changes the status, and null clears it.
			const ego = egoMeta(record) as { providerRetry?: { text?: unknown; delayMs?: unknown } | null } | undefined;
			if (ego && "providerRetry" in ego) {
				const text = ego.providerRetry?.text;
				if (typeof text === "string" && text) {
					const delay = ego.providerRetry?.delayMs;
					draft.retries[sessionId] = {
						text,
						...(typeof delay === "number" && Number.isFinite(delay) && delay >= 0
							? { retryAt: Date.now() + delay }
							: {}),
					};
				} else delete draft.retries[sessionId];
			}
			break;
		}
		case "usage_update": {
			if (
				typeof record.used !== "number" ||
				!Number.isFinite(record.used) ||
				record.used < 0 ||
				typeof record.size !== "number" ||
				!Number.isFinite(record.size) ||
				record.size <= 0
			)
				break;
			const cost = record.cost;
			draft.usage[sessionId] = {
				used: record.used,
				size: record.size,
				...(cost &&
				typeof cost === "object" &&
				typeof (cost as { amount?: unknown }).amount === "number" &&
				Number.isFinite((cost as { amount: number }).amount) &&
				typeof (cost as { currency?: unknown }).currency === "string"
					? { cost: cost as { amount: number; currency: string } }
					: {}),
			};
			break;
		}
		case "user_message_chunk":
			appendUserChunk(
				draft,
				sessionId,
				entries,
				textOf(record.content),
				typeof record.messageId === "string" ? record.messageId : undefined,
				egoMeta(record)?.inherited === true ? true : undefined,
			);
			break;
		case "agent_message_chunk": {
			const ego = egoMeta(record);
			if (ego?.salience === "card") {
				// A card is one whole message. Chunk-joining it would glue it
				// onto the agent's last reply, and the next reply onto it.
				const cardText = textOf(record.content);
				if (cardText) {
					entries.push({
						id: `e${draft.nextId}`,
						kind: "notice",
						text: cardText,
						action: noticeAction(ego.action),
						...(ego.inherited === true ? { inherited: true } : {}),
					});
					draft.nextId += 1;
				}
				break;
			}
			delete draft.pendingUserEcho[sessionId];
			appendChunk(
				draft,
				entries,
				"agent",
				textOf(record.content),
				typeof record.messageId === "string" ? record.messageId : undefined,
				egoMeta(record)?.inherited === true ? true : undefined,
			);
			if (textOf(record.content)) draft.turnHasReply[sessionId] = true;
			break;
		}
		case "agent_thought_chunk":
			delete draft.pendingUserEcho[sessionId];
			appendChunk(
				draft,
				entries,
				"thought",
				textOf(record.content),
				typeof record.messageId === "string" ? record.messageId : undefined,
				egoMeta(record)?.inherited === true ? true : undefined,
			);
			break;
		case "tool_call":
		case "tool_call_update":
			delete draft.pendingUserEcho[sessionId];
			foldToolCall(draft, entries, record);
			if (egoMeta(record)?.inherited === true) {
				const tool = entries.find((entry) => entry.kind === "tool" && entry.call.toolCallId === record.toolCallId);
				if (tool) tool.inherited = true;
			}
			break;
		case "plan": {
			// The agent sends the whole plan every time and the client replaces
			// its copy — an appended plan would show every intermediate list.
			const planEntries = (record.entries ?? []) as AcpPlanEntry[];
			const plan = entries.find((entry) => entry.kind === "plan");
			if (plan?.kind === "plan") {
				plan.entries = planEntries;
				break;
			}
			entries.push({ id: `e${draft.nextId}`, kind: "plan", entries: planEntries });
			draft.nextId += 1;
			break;
		}
		default:
			// `config_option_update` and anything a later protocol
			// version adds are not rendered by the transcript projection.
			break;
	}
}

export const acpTranscript = {
	state,

	/** Forget everything. Tests only. */
	reset(): void {
		setState({ sessions: {}, titles: {}, usage: {}, turnHasReply: {}, pendingUserEcho: {}, retries: {}, nextId: 1 });
	},

	/**
	 * Drop one session's transcript and hand back what was dropped.
	 *
	 * Called before a `session/load`, which replays the whole history: without
	 * it the replay lands under what is already there and every message appears
	 * twice. The return value is what makes that safe to do *before* the load —
	 * a load that is refused leaves the panel live on a session whose
	 * conversation has been erased, and only the caller knows the request failed.
	 */
	clear(sessionId: AcpSessionId): AcpTranscriptEntry[] {
		// Unwrapped and copied: what comes back has to outlive the store node it
		// came from, and handing back a live proxy to an array this call is about
		// to delete is how a restore puts back an empty transcript.
		const removed = [...(unwrap(state.sessions[sessionId]) ?? [])];
		setState(
			produce((s: TranscriptState) => {
				delete s.sessions[sessionId];
				delete s.turnHasReply[sessionId];
				delete s.pendingUserEcho[sessionId];
				delete s.retries[sessionId];
			}),
		);
		return removed;
	},

	/**
	 * Put back a transcript `clear` removed, for a load that never happened.
	 *
	 * Replaces rather than merges, and does nothing for an empty list: the only
	 * caller is undoing its own `clear`, so anything now under that session id
	 * arrived after the failure and is fresher than what is being restored.
	 */
	restore(sessionId: AcpSessionId, entries: AcpTranscriptEntry[]): void {
		if (entries.length === 0) return;
		setState(
			produce((s: TranscriptState) => {
				s.sessions[sessionId] = entries;
			}),
		);
	},

	/**
	 * Read one frame for the conversation it belongs to.
	 *
	 * A frame with no session — a connection-level state change, a gap — is not
	 * part of any conversation and is dropped here; `acpStore` is where those
	 * are held.
	 */
	applyFrame(frame: AcpStreamFrame): void {
		if (frame.kind !== "event" || !frame.sessionId) return;
		const sessionId = frame.sessionId;
		const event = frame.event;
		setState(
			produce((s: TranscriptState) => {
				const entries = (s.sessions[sessionId] ??= []);
				if (event.kind === "turnStarted") {
					s.turnHasReply[sessionId] = false;
					return;
				}
				if (event.kind === "promptSent") {
					s.turnHasReply[sessionId] = false;
					const id = `e${s.nextId}`;
					entries.push({ id, kind: "user", text: event.text });
					if (event.text) s.pendingUserEcho[sessionId] = { entryId: id, received: "" };
					s.nextId += 1;
					return;
				}
				if (event.kind === "sessionUpdate") {
					reduceUpdate(s, sessionId, entries, event.update);
					return;
				}
				if (event.kind === "turnFailed") {
					delete s.pendingUserEcho[sessionId];
					delete s.retries[sessionId];
					settleToolCalls(entries, "failed");
					entries.push({ id: `e${s.nextId}`, kind: "failed", message: event.message });
					s.nextId += 1;
					return;
				}
				if (event.kind === "turnSettled") {
					delete s.pendingUserEcho[sessionId];
					delete s.retries[sessionId];
					settleToolCalls(entries, event.stopReason === "end_turn" ? "completed" : "failed");
				}
				if (event.kind === "turnSettled" && (event.stopReason !== "end_turn" || !s.turnHasReply[sessionId])) {
					// A turn that ended because it was cancelled, refused or ran
					// out of room ended without answering, and a transcript that
					// just stops there reads as the agent falling silent.
					entries.push({
						id: `e${s.nextId}`,
						kind: "settled",
						stopReason: s.turnHasReply[sessionId]
							? event.stopReason
							: event.stopReason === "end_turn"
								? "empty"
								: event.stopReason,
					});
					s.nextId += 1;
				}
			}),
		);
	},

	/**
	 * Settle the calls a replayed history left unfinished.
	 *
	 * A session that ended mid-call — a permission never answered, a crashed
	 * process — has `tool_call` and `in_progress` in its journal and nothing
	 * after, so no `turnSettled` ever arrives to close them. `failed` because
	 * ACP has no "unknown" status and the call did not complete; it is the status
	 * `settleToolCalls` already gives a turn that did not end. ego does not say
	 * on `session/load` whether a turn is still running, so this is a reading of
	 * the journal, not of the session: a live call's next update overwrites it.
	 */
	settleReplayed(sessionId: AcpSessionId): void {
		setState(
			produce((s: TranscriptState) => {
				const entries = s.sessions[sessionId];
				if (entries) settleToolCalls(entries, "failed");
			}),
		);
	},

	/**
	 * What the user typed, shown before the agent has echoed it back.
	 *
	 * Returns the entry's id so the caller can take it back. The message is put
	 * on screen before the prompt is sent — that is the point of it — so a prompt
	 * the backend refuses would otherwise leave a turn the agent never received
	 * sitting in the conversation, indistinguishable from one it ignored.
	 */
	noteUserMessage(sessionId: AcpSessionId, text: string): string {
		const id = `e${state.nextId}`;
		setState(
			produce((s: TranscriptState) => {
				const entries = (s.sessions[sessionId] ??= []);
				entries.push({ id, kind: "user", text });
				s.nextId += 1;
			}),
		);
		return id;
	},

	/**
	 * Take back an entry that turned out not to have happened.
	 *
	 * By id rather than by position: frames keep arriving while a prompt is in
	 * flight, so "the last entry" is not reliably the one being withdrawn.
	 */
	dropEntry(sessionId: AcpSessionId, entryId: string): void {
		setState(
			produce((s: TranscriptState) => {
				const entries = s.sessions[sessionId];
				if (!entries) return;
				const index = entries.findIndex((entry) => entry.id === entryId);
				if (index >= 0) entries.splice(index, 1);
			}),
		);
	},

	entries(sessionId: AcpSessionId): AcpTranscriptEntry[] {
		return state.sessions[sessionId] ?? [];
	},

	title(sessionId: AcpSessionId): string | null {
		return state.titles[sessionId] ?? null;
	},

	/** ego's provider-retry line for a turn waiting to retry, or null. */
	retry(sessionId: AcpSessionId): AcpProviderRetry | null {
		return state.retries[sessionId] ?? null;
	},

	usage(sessionId: AcpSessionId): { used: number; size: number; cost?: { amount: number; currency: string } } | null {
		return state.usage[sessionId] ?? null;
	},
};
