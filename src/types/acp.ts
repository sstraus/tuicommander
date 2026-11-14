/**
 * The ACP wire, as TUICommander's Rust client actually writes it.
 *
 * These are not the ACP protocol's own types. They are the shapes
 * `src-tauri/src/acp/mod.rs` serializes, which is a deliberately narrower
 * surface: the client refuses stdio MCP servers and any elicitation mode but
 * `form` before they reach a host, so a type here that admitted them would
 * describe a frame that cannot arrive.
 *
 * A boolean config option is **not** one of those, and the difference matters
 * because the two failures are opposite. `client_boolean_config: false` is a
 * capability this client does not advertise, and `AcpCapabilitySnapshot`
 * excludes it for that reason — but Rust passes `Vec<SessionConfigOption>`
 * through unfiltered, so an agent that publishes a boolean option anyway
 * produces a frame that arrives and is simply never rendered. Deleting the
 * `{ type: "boolean" }` member here to match the capability would break
 * parsing of a frame that can and does turn up (#810-4986).
 *
 * Every field name below is pinned by `src-tauri/tests/story785_event_wire_shapes.rs`.
 * That test exists because both defects it caught were silent: serde renames
 * variants and fields under separate attributes, and an internally tagged enum
 * wrapping a bare value invents a key from the value rather than failing. Neither
 * threw. Both would have produced a store that parsed nothing and reported no
 * error, so do not widen a type here to make a frame fit — check the test first.
 */

/** A connection id, turn id or host request id. All are UUIDs on the wire. */
export type AcpConnectionId = string;
export type AcpTurnId = string;
export type AcpHostRequestId = string;
export type AcpSessionId = string;

/**
 * What a registered connection is.
 *
 * There is no `connecting`: the id is minted inside `connect` and does not
 * escape it until initialization succeeded, so a connection that is still
 * coming up cannot be named, listed or subscribed to.
 */
export type AcpConnectionState = "ready" | "closing" | "closed" | "failed" | "killed";

export type AcpAttachmentState =
	| "attaching"
	| "idle"
	| "prompting"
	| "cancelling"
	| "pause_pending"
	| "paused"
	| "closing"
	| "closed"
	| "detached";

export type AcpTurnState = "running" | "cancelling" | "settled";

export type AcpHoldState = "running" | "pending" | "paused";

export type AcpConnectionSettlementReason =
	| "disconnected"
	| "eof"
	| "transport_error"
	| "protocol_violation"
	| "killed";

export type AcpClientErrorCode =
	| "invalid_input"
	| "initialization_failed"
	| "not_found"
	| "unsupported_protocol"
	| "capability_unavailable"
	| "agent_error"
	| "protocol_violation"
	| "transport_closed"
	| "stream_gap";

/**
 * A refusal, carried identically by a failed command, an HTTP error body and a
 * `gap` stream frame.
 *
 * One struct for all three on purpose: a host that renders an error has one
 * shape to render, whichever way the failure reached it.
 */
export interface AcpClientError {
	code: AcpClientErrorCode;
	message: string;
	connectionId: AcpConnectionId | null;
	sessionId: AcpSessionId | null;
	operation: string | null;
	retryable: boolean;
}

export interface AcpConnectionSettlement {
	connectionId: AcpConnectionId;
	generation: number;
	reason: AcpConnectionSettlementReason;
}

/**
 * What the agent said it can do, reduced to the operations this client offers.
 *
 * `mcpStdio` is always `false` and `mcpAcp` always `true`: the client carries
 * one MCP server, `tuicommander`, on the ACP transport, by contract rather than
 * by omission. `clientBooleanConfig` is excluded the same way, so a boolean
 * config option is never rendered even though the wire type allows one.
 */
export interface AcpCapabilitySnapshot {
	protocol: unknown;
	load: boolean;
	list: boolean;
	resume: boolean;
	fork: boolean;
	forkAtMessage?: boolean;
	delete: boolean;
	close: boolean;
	additionalDirectories: boolean;
	promptImage: boolean;
	promptAudio: boolean;
	promptEmbeddedContext: boolean;
	mcpStdio: boolean;
	mcpHttp: boolean;
	mcpSse: boolean;
	mcpAcp: boolean;
	clientFormElicitation: boolean;
	clientBooleanConfig: boolean;
	/** Present only when ego advertised `_ego/pause` and `_ego/resume` as one pair. */
	egoHoldVersion: number | null;
	egoSteerVersion?: number | null;
	egoChildActivityVersion?: number | null;
	/** Present only when ego advertised `_ego/compact`. */
	egoCompactVersion: number | null;
	/** Present only when ego announces automatic compaction with `_meta.ego.compacted`. */
	egoCompactedVersion?: number | null;
}

/** ego's answer to `_ego/compact`: the compacted continuation is a new session. */
export interface EgoCompactResponse {
	sourceSessionId: AcpSessionId;
	targetSessionId: AcpSessionId;
	publication:
		| { kind: "not_published"; diagnostic: string }
		| { kind: "published_durably"; diagnostic: string | null }
		| { kind: "published_durability_uncertain"; diagnostic: string };
}

export interface AcpTurnSnapshot {
	turnId: AcpTurnId;
	state: AcpTurnState;
	stopReason: string | null;
	usage: unknown | null;
}

export interface AcpQueuedPrompt {
	turnId: AcpTurnId;
	summary: string;
}

export interface AcpUsageSnapshot {
	context: unknown | null;
	endTurn: unknown | null;
}

/** One session this connection is attached to. */
export type AcpChatActivityState = "idle" | "busy" | "awaiting";

export interface AcpHumanWait {
	requestId: AcpHostRequestId;
	kind: "permission" | "question";
}

export interface AcpActiveTool {
	toolCallId: string;
	title: string;
}

export interface AcpChildActivity {
	turnId: string;
	itemId: string;
	child: unknown;
}

/** What one chat is doing, as the backend derived it. A higher revision is newer. */
export interface AcpChatActivity {
	state: AcpChatActivityState;
	humanWaits: AcpHumanWait[];
	tools: AcpActiveTool[];
	children: AcpChildActivity[];
	interrupted: boolean;
	revision: number;
}

export interface AcpAttachmentSnapshot {
	sessionId: AcpSessionId;
	state: AcpAttachmentState;
	cwd: string;
	additionalDirectories: string[];
	configOptions: AcpSessionConfigOption[];
	/** Ego's reason when its latest config update reported a failed session setup. */
	configFailure?: string;
	/** Latest complete ACP command offer, retained across snapshot refreshes. */
	availableCommands?: AcpAvailableCommand[];
	/** Authoritative ego clamp notices returned when a repo selects a profile. */
	profileWarnings?: string[];
	usage: AcpUsageSnapshot | null;
	activeTurn: AcpTurnSnapshot | null;
	queuedPrompts: AcpQueuedPrompt[];
	pendingPermissionIds: AcpHostRequestId[];
	pendingElicitationIds: AcpHostRequestId[];
	/** Derived in Rust; absent only from a backend that predates it. */
	activity?: AcpChatActivity;
}

/**
 * Everything true about one connection right now.
 *
 * `earliestSequence`/`latestSequence` bound the journal a subscriber may still
 * resume from, so a host knows whether its cursor is recoverable before it opens
 * a stream and finds out by being handed a gap.
 */
export interface AcpConnectionSnapshot {
	connectionId: AcpConnectionId;
	generation: number;
	state: AcpConnectionState;
	agentInfo: { name?: string; version?: string } | null;
	capabilities: AcpCapabilitySnapshot | null;
	attachments: AcpAttachmentSnapshot[];
	earliestSequence: number;
	latestSequence: number;
	settlement: AcpConnectionSettlement | null;
}

// ---------------------------------------------------------------------------
// Session configuration
// ---------------------------------------------------------------------------

export interface AcpAvailableCommand {
	name: string;
	description: string;
	input?: { hint: string } | null;
}

export interface AcpSessionConfigSelectOption {
	value: string;
	name: string;
	description?: string;
}

export interface AcpSessionConfigSelectGroup {
	group: string;
	name: string;
	options: AcpSessionConfigSelectOption[];
}

/**
 * One knob the session publishes — model, mode, reasoning effort.
 *
 * The panel renders these and nothing else: the vocabulary is the agent's, so a
 * hardcoded model list here would be a second, wrong answer to a question the
 * session already answers.
 */
export type AcpSessionConfigOption = {
	id: string;
	name: string;
	description?: string;
	category?: string;
} & (
	| {
			type: "select";
			currentValue: string;
			options: AcpSessionConfigSelectOption[] | AcpSessionConfigSelectGroup[];
	  }
	| { type: "boolean"; currentValue: boolean }
);

/** The value sent back to `set_config_option`. */
export type AcpSessionConfigOptionValue = { type: "boolean"; value: boolean } | { value: string };

// ---------------------------------------------------------------------------
// Interactions
// ---------------------------------------------------------------------------

export type AcpPermissionOptionKind = "allow_once" | "allow_always" | "reject_once" | "reject_always";

export interface AcpPermissionOption {
	optionId: string;
	name: string;
	kind: AcpPermissionOptionKind;
}

/**
 * What the agent wants permission for, and the answers it will accept.
 *
 * `options` is the agent's list. A host renders those ids and answers with one
 * of them; inventing an Allow/Deny pair of its own would answer a question
 * nobody asked.
 */
export interface AcpRequestPermissionRequest {
	sessionId: AcpSessionId;
	toolCall: unknown;
	options: AcpPermissionOption[];
}

/**
 * An elicitation, always in `form` mode.
 *
 * The Rust client answers `cancel` to any other mode before it reaches a host
 * (`Interaction::advertised`), so a `mode` other than `"form"` cannot arrive
 * here and must never be drawn.
 */
export interface AcpCreateElicitationRequest {
	mode: "form";
	sessionId?: AcpSessionId;
	message: string;
	requestedSchema: unknown;
}

export type AcpPendingInteraction =
	| {
			kind: "permission";
			requestId: AcpHostRequestId;
			sessionId: AcpSessionId;
			request: AcpRequestPermissionRequest;
	  }
	| {
			kind: "elicitation";
			requestId: AcpHostRequestId;
			sessionId: AcpSessionId;
			request: AcpCreateElicitationRequest;
	  };

export type AcpRequestPermissionOutcome = { outcome: "cancelled" } | { outcome: "selected"; optionId: string };

export type AcpElicitationAction =
	| { action: "accept"; content?: Record<string, unknown> }
	| { action: "decline" }
	| { action: "cancel" };

export interface AcpInteractionSettlement {
	requestId: AcpHostRequestId;
	sessionId: AcpSessionId;
}

// ---------------------------------------------------------------------------
// The stream
// ---------------------------------------------------------------------------

/**
 * One thing that happened, tagged by `kind`.
 *
 * `connectionState` and `attachmentState` carry their payload under `state`
 * rather than as the object's own shape — see the note in `acp/mod.rs`; the
 * earlier bare-newtype spelling produced a different object per state and could
 * not be typed at all.
 */
export type AcpClientEvent =
	| { kind: "connectionState"; state: AcpConnectionState }
	| { kind: "attachmentState"; state: AcpAttachmentState }
	| { kind: "turnStarted" }
	| { kind: "promptSent"; text: string }
	| { kind: "promptQueueChanged"; queuedPrompts: AcpQueuedPrompt[] }
	| { kind: "sessionUpdate"; update: AcpSessionUpdate }
	| { kind: "turnSettled"; stopReason: string; usage: unknown | null }
	| { kind: "turnFailed"; message: string; state: AcpAttachmentState }
	| { kind: "permissionRequested"; requestId: AcpHostRequestId; request: AcpRequestPermissionRequest }
	| { kind: "permissionSettled"; requestId: AcpHostRequestId; outcome: AcpRequestPermissionOutcome }
	| { kind: "elicitationRequested"; requestId: AcpHostRequestId; request: AcpCreateElicitationRequest }
	| { kind: "elicitationSettled"; requestId: AcpHostRequestId; action: AcpElicitationAction }
	| { kind: "activityChanged"; activity: AcpChatActivity };

/**
 * Ego's own update, forwarded whole.
 *
 * Carried under `update` rather than flattened beside the event's `kind`, and
 * that is not cosmetic: `tool_call` has a field called `kind` too, so a
 * flattened update would put that key in the object twice and every JSON reader
 * keeps the last — a `read` tool call would arrive tagged `read` and match no
 * event. Only the members the panel renders are spelled out; the rest stay open
 * rather than being narrowed to a guess.
 */
export type AcpSessionUpdate =
	| { sessionUpdate: "user_message_chunk"; content: unknown }
	| { sessionUpdate: "agent_message_chunk"; content: unknown }
	| { sessionUpdate: "agent_thought_chunk"; content: unknown }
	| { sessionUpdate: "tool_call"; [key: string]: unknown }
	| { sessionUpdate: "tool_call_update"; [key: string]: unknown }
	| { sessionUpdate: "plan"; [key: string]: unknown }
	| { sessionUpdate: "available_commands_update"; availableCommands: AcpAvailableCommand[] }
	| { sessionUpdate: "config_option_update"; [key: string]: unknown }
	| { sessionUpdate: "usage_update"; used: number; size: number; [key: string]: unknown }
	| { sessionUpdate: string; [key: string]: unknown };

/**
 * One journal entry: what happened, and where it sits in the order.
 *
 * `sequence` is the cursor. A subscriber resumes with the sequence after the
 * last one it handled, because `subscribe` delivers `>= from`.
 */
export interface AcpEventEnvelope {
	connectionId: AcpConnectionId;
	generation: number;
	sequence: number;
	sessionId: AcpSessionId | null;
	turnId: AcpTurnId | null;
	event: AcpClientEvent;
}

/**
 * A frame off the stream.
 *
 * `gap` and `end` are both terminal — the producer stops after either, and a
 * `gap` is never followed by an `end`. A gap is not a transport failure: it says
 * the journal no longer holds the sequence asked for, and the only recovery on
 * record is a fresh connection replaying history through `session/load`.
 */
export type AcpStreamFrame =
	| ({ kind: "event" } & AcpEventEnvelope)
	| ({ kind: "gap" } & AcpClientError)
	| { kind: "end" };

export type AcpNoticeKind = "ready" | "settled" | "interaction_pending" | "interaction_settled" | "card";

/**
 * A wake signal, carrying no ordered payload of its own.
 *
 * It says come and look, and where. The ordered payload stays on the stream and
 * the current picture stays in the snapshot, so a host that reacts by fetching
 * one of those reads the same truth as a host that never missed a frame.
 */
export interface AcpNotice {
	connectionId: AcpConnectionId;
	generation: number;
	sessionId: AcpSessionId | null;
	requestId: AcpHostRequestId | null;
	sequence: number;
	kind: AcpNoticeKind;
}

/**
 * What a session may reach, supplied fresh on every attach and never persisted.
 *
 * There is no `mcpServers` here on purpose, and sending one is refused rather
 * than ignored: the session routes are reachable from a browser and take no
 * spawn guard, so a body that could name a server would let whoever sends one
 * point ego at any endpoint it liked. The list is synthesised in Rust from
 * configuration. Pinned by `a_session_authority_refuses_a_body_that_names_an_mcp_server`.
 */
export interface AcpSessionAuthority {
	cwd: string;
	additionalDirectories: string[];
}

// ---------------------------------------------------------------------------
// Content, tool calls and plans
//
// The shapes a session update carries, spelled only as far as the panel reads
// them. They are `snake_case`-tagged where the protocol is: `ContentBlock`,
// `ToolCallContent`, `ToolKind` and the two status enums all carry
// `rename_all = "snake_case"` in the schema crate, while their *fields* are
// camelCase. Mixing those up produces a union that never matches and a panel
// that renders nothing, with no error anywhere.
// ---------------------------------------------------------------------------

export type AcpContentBlock =
	| { type: "text"; text: string }
	| { type: "image"; mimeType: string; data: string; uri?: string }
	| { type: "audio"; mimeType: string; data: string }
	| { type: "resource_link"; uri: string; name: string; title?: string; description?: string }
	| { type: "resource"; resource: unknown };

export type AcpToolKind =
	| "read"
	| "edit"
	| "delete"
	| "move"
	| "search"
	| "execute"
	| "think"
	| "fetch"
	| "switch_mode"
	| "other";

export type AcpToolCallStatus = "pending" | "in_progress" | "completed" | "failed";

export interface AcpDiff {
	path: string;
	oldText?: string | null;
	newText: string;
}

export type AcpToolCallContent =
	| { type: "content"; content: AcpContentBlock }
	| { type: "diff"; path: string; oldText?: string | null; newText: string }
	| { type: "terminal"; terminalId: string };

export interface AcpToolCallLocation {
	path: string;
	line?: number | null;
}

/** A tool call as first announced. An update carries the same fields, all optional. */
export interface AcpToolCall {
	toolCallId: string;
	title: string;
	kind?: AcpToolKind;
	status?: AcpToolCallStatus;
	content?: AcpToolCallContent[];
	locations?: AcpToolCallLocation[];
	rawInput?: unknown;
	rawOutput?: unknown;
}

/** What one unattended ego turn produced — the whole answer of `acp_one_shot_prompt`.
 *
 * There is no stream and no transcript: the turn is folded in Rust and arrives
 * finished. `declined` is the count of questions refused because nobody was
 * there to answer, and it is the only way to tell "ego had nothing to say" from
 * "ego wanted a tool this mode cannot grant". */
export interface EgoTurn {
	text: string;
	stopReason: string;
	declined: number;
}

export type AcpPlanEntryStatus = "pending" | "in_progress" | "completed";

export interface AcpPlanEntry {
	content: string;
	priority: "high" | "medium" | "low";
	status: AcpPlanEntryStatus;
}
