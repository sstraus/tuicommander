import { createStore, produce } from "solid-js/store";
import type {
	AcpAttachmentSnapshot,
	AcpAvailableCommand,
	AcpClientError,
	AcpClientEvent,
	AcpConnectionId,
	AcpConnectionSnapshot,
	AcpHostRequestId,
	AcpPendingInteraction,
	AcpSessionConfigOption,
	AcpSessionId,
	AcpStreamFrame,
} from "../types/acp";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

export interface AcpTurnOutcome {
	turnId: string;
	event: Extract<AcpClientEvent, { kind: "turnSettled" | "turnFailed" }>;
}

/**
 * Everything this window knows about one ACP connection.
 *
 * The three sources are deliberately kept apart rather than folded together,
 * because they answer different questions and arrive by different routes. The
 * snapshot is the current picture and is refetched; the cursor is a position in
 * an ordered journal and is only ever advanced; the interactions are a set the
 * agent adds to and anybody — another window, a phone — may remove from.
 */
interface AcpConnectionEntry {
	/** Latest streamed completion per attachment, including v1 failures with no activeTurn. */
	outcomes: Record<AcpSessionId, AcpTurnOutcome>;
	/** Automatic compaction successors announced by ego (`_meta.ego.compacted`), by source session. */
	successors: Record<AcpSessionId, AcpSessionId>;
	snapshot: AcpConnectionSnapshot;
	/**
	 * The last sequence this store handled, or 0 for none.
	 *
	 * Not the same as the snapshot's `latestSequence`: that is what the journal
	 * holds, this is what was read.
	 */
	cursor: number;
	interactions: AcpPendingInteraction[];
	/**
	 * Requests this window saw settled, newest last.
	 *
	 * A re-adopt applies a pending list it fetched before the old stream
	 * delivered a settlement, and the new stream resumes after that frame. So
	 * nothing would ever take the answered question away again: a settled id is
	 * never admitted back.
	 */
	settled: AcpHostRequestId[];
	/**
	 * The journal no longer holds the sequence asked for.
	 *
	 * Sticky on purpose. A gap is the end of that subscription, and the only
	 * recovery on record is a fresh connection replaying history — so it stays
	 * visible until something does that, rather than being cleared by the next
	 * frame that happens to arrive.
	 */
	gap: AcpClientError | null;
	streaming: boolean;
}

interface AcpState {
	connections: Record<AcpConnectionId, AcpConnectionEntry>;
}

const [state, setState] = createStore<AcpState>({ connections: {} });

// ---------------------------------------------------------------------------
// Reducing the stream
// ---------------------------------------------------------------------------

/**
 * Apply one event to a connection already in the store.
 *
 * Takes the entry rather than the id because every caller has already resolved
 * it and proved it exists — a frame for an unknown connection is dropped one
 * level up, where the reason for dropping it can be stated once.
 */
/** Settled request ids remembered per connection; far more than ever open at once. */
const SETTLED_RETAINED = 256;

function admissible(entry: AcpConnectionEntry, requestId: AcpHostRequestId): boolean {
	return !entry.settled.includes(requestId) && !entry.interactions.some((i) => i.requestId === requestId);
}

function reduceEvent(entry: AcpConnectionEntry, frame: Extract<AcpStreamFrame, { kind: "event" }>): void {
	const event = frame.event;
	switch (event.kind) {
		case "connectionState":
			entry.snapshot.state = event.state;
			break;
		case "attachmentState": {
			// The event names no session; the envelope does. Reading it off the
			// event would move every attachment on the connection at once.
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment) {
				attachment.state = event.state;
				if (event.state === "cancelling" && attachment.activeTurn) attachment.activeTurn.state = "cancelling";
				// A reattach restarts the revision at 0; the old one must not outrank it.
				if (event.state === "detached") attachment.activity = undefined;
			}
			break;
		}
		case "turnStarted": {
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment && frame.turnId) {
				attachment.state = "prompting";
				attachment.activeTurn = { turnId: frame.turnId, state: "running", stopReason: null, usage: null };
			}
			break;
		}
		case "turnSettled": {
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment && attachment.activeTurn?.turnId === frame.turnId) {
				attachment.state = "idle";
				attachment.activeTurn = {
					turnId: frame.turnId,
					state: "settled",
					stopReason: event.stopReason,
					usage: event.usage,
				};
			}
			break;
		}
		case "turnFailed": {
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment && attachment.activeTurn?.turnId === frame.turnId) {
				attachment.state = event.state;
				attachment.activeTurn = null;
			}
			break;
		}
		case "promptQueueChanged": {
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment) attachment.queuedPrompts = event.queuedPrompts;
			break;
		}
		case "sessionUpdate": {
			const update = event.update;
			if (update.sessionUpdate === "available_commands_update" && Array.isArray(update.availableCommands)) {
				const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
				if (attachment) attachment.availableCommands = update.availableCommands as AcpAvailableCommand[];
			}
			const compacted = (
				(update as { _meta?: { ego?: { compacted?: unknown } } })._meta?.ego as { compacted?: unknown } | undefined
			)?.compacted as { version?: unknown; sourceSessionId?: unknown; targetSessionId?: unknown } | undefined;
			if (
				update.sessionUpdate === "session_info_update" &&
				compacted?.version === 1 &&
				typeof compacted.sourceSessionId === "string" &&
				typeof compacted.targetSessionId === "string" &&
				compacted.targetSessionId !== compacted.sourceSessionId &&
				compacted.sourceSessionId === frame.sessionId
			) {
				entry.successors[compacted.sourceSessionId] = compacted.targetSessionId;
			}
			if (update.sessionUpdate === "config_option_update" && Array.isArray(update.configOptions)) {
				const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
				if (attachment) attachment.configOptions = update.configOptions as AcpSessionConfigOption[];
			}
			break;
		}
		case "permissionRequested":
			if (admissible(entry, event.requestId)) {
				entry.interactions.push({
					kind: "permission",
					requestId: event.requestId,
					sessionId: frame.sessionId ?? "",
					request: event.request,
				});
			}
			break;
		case "elicitationRequested":
			if (admissible(entry, event.requestId)) {
				entry.interactions.push({
					kind: "elicitation",
					requestId: event.requestId,
					sessionId: frame.sessionId ?? "",
					request: event.request,
				});
			}
			break;
		case "permissionSettled":
		case "elicitationSettled":
			// Whoever answered it, the question is gone. This window is not
			// necessarily the one that answered.
			entry.interactions = entry.interactions.filter((i) => i.requestId !== event.requestId);
			if (!entry.settled.includes(event.requestId)) {
				entry.settled.push(event.requestId);
				if (entry.settled.length > SETTLED_RETAINED) entry.settled.shift();
			}
			break;
		case "activityChanged": {
			const attachment = entry.snapshot.attachments.find((a) => a.sessionId === frame.sessionId);
			if (attachment && (attachment.activity?.revision ?? -1) <= event.activity.revision)
				attachment.activity = event.activity;
			break;
		}
		default:
			// Unprojected events still move the cursor.
			break;
	}
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const acpStore = {
	state,

	/** Forget everything. Tests only — nothing in the app drops every connection at once. */
	reset(): void {
		setState({ connections: {} });
	},

	/**
	 * Take a connection snapshot as the whole truth about that connection.
	 *
	 * It replaces rather than merges: a snapshot is the complete list of
	 * attachments, so merging would keep one the agent has since dropped and
	 * nothing would ever remove it. The cursor, the interactions and any gap
	 * survive, because none of them is part of the picture a snapshot paints.
	 */
	applySnapshot(snapshot: AcpConnectionSnapshot): void {
		setState(
			produce((s: AcpState) => {
				const existing = s.connections[snapshot.connectionId];
				if (existing) {
					// A snapshot fetched before a newer stream frame must not
					// roll the activity back: keep whichever revision is higher.
					const attachments = snapshot.attachments.map((attachment) => {
						const held = existing.snapshot.attachments.find((a) => a.sessionId === attachment.sessionId);
						return held?.activity && held.activity.revision > (attachment.activity?.revision ?? -1)
							? { ...attachment, activity: held.activity }
							: attachment;
					});
					existing.snapshot = { ...snapshot, attachments };
					for (const session of Object.keys(existing.outcomes)) {
						if (!snapshot.attachments.some((attachment) => attachment.sessionId === session))
							delete existing.outcomes[session];
					}
					return;
				}
				s.connections[snapshot.connectionId] = {
					snapshot,
					outcomes: {},
					successors: {},
					cursor: 0,
					interactions: [],
					settled: [],
					gap: null,
					streaming: false,
				};
			}),
		);
	},

	/** Take the pending-interaction list as the complete set for that connection. */
	applyInteractions(connectionId: AcpConnectionId, interactions: AcpPendingInteraction[]): void {
		setState(
			produce((s: AcpState) => {
				const entry = s.connections[connectionId];
				if (entry) entry.interactions = interactions.filter((i) => !entry.settled.includes(i.requestId));
			}),
		);
	},

	/**
	 * Apply one frame off the stream that `streamId` belongs to.
	 *
	 * A frame naming a connection this store does not hold is dropped in
	 * silence: a disconnect races the frames already in flight behind it, and
	 * building an entry from a frame would produce a connection with no snapshot
	 * behind it — which renders as a connection that does not exist.
	 *
	 * `streamId` is passed because `end` is a **unit variant on the wire** and so
	 * names nobody. Connections are per repo root and coexist, so reading it as
	 * "every connection stopped" froze the panel on root B when ego on root A
	 * exited — and, after a reconnect, the dead stream's own `end` froze the
	 * fresh connection that had just replaced it. Every stream belongs to exactly
	 * one connection, so the reader's own id is the answer the frame lacks.
	 */
	applyFrame(streamId: AcpConnectionId, frame: AcpStreamFrame): void {
		if (frame.kind === "end") {
			setState(
				produce((s: AcpState) => {
					const entry = s.connections[streamId];
					if (entry) entry.streaming = false;
				}),
			);
			return;
		}

		if (frame.kind === "gap") {
			const connectionId = frame.connectionId;
			if (!connectionId) return;
			setState(
				produce((s: AcpState) => {
					const entry = s.connections[connectionId];
					if (!entry) return;
					// The cursor stays where the last real frame left it. Moving it
					// past a gap would make the next resume ask for events nothing
					// ever delivered.
					entry.gap = frame;
					entry.streaming = false;
				}),
			);
			return;
		}

		setState(
			produce((s: AcpState) => {
				const entry = s.connections[frame.connectionId];
				if (!entry) return;
				reduceEvent(entry, frame);
				if (
					frame.sequence > entry.cursor &&
					frame.sessionId &&
					frame.turnId &&
					entry.snapshot.attachments.some((attachment) => attachment.sessionId === frame.sessionId) &&
					(frame.event.kind === "turnSettled" || frame.event.kind === "turnFailed")
				) {
					entry.outcomes[frame.sessionId] = { turnId: frame.turnId, event: frame.event };
				}
				// The journal is ordered and the transport delivers in order, so a
				// lower sequence is a defect upstream, not a late frame. Letting it
				// drag the cursor backwards would replay everything after it.
				if (frame.sequence > entry.cursor) entry.cursor = frame.sequence;
			}),
		);
	},

	/** Note that a subscription is live again, which is what discharges a gap. */
	markStreaming(connectionId: AcpConnectionId): void {
		setState(
			produce((s: AcpState) => {
				const entry = s.connections[connectionId];
				if (!entry) return;
				entry.gap = null;
				entry.streaming = true;
			}),
		);
	},

	/**
	 * Note that nothing is reading this connection's journal any more.
	 *
	 * Distinct from a gap, and from the `end` frame: no sequence was lost and
	 * the journal did not finish — the reader gave up. The snapshot stays, so a
	 * panel renders the connection as it last was and offers to read again,
	 * rather than showing a stream that is silently dead.
	 */
	markStopped(connectionId: AcpConnectionId): void {
		setState(
			produce((s: AcpState) => {
				const entry = s.connections[connectionId];
				if (entry) entry.streaming = false;
			}),
		);
	},

	/** Drop a connection and everything held for it. */
	forget(connectionId: AcpConnectionId): void {
		setState(
			produce((s: AcpState) => {
				delete s.connections[connectionId];
			}),
		);
	},

	// -------------------------------------------------------------------------
	// Reading
	// -------------------------------------------------------------------------

	connectionIds(): AcpConnectionId[] {
		return Object.keys(state.connections);
	},

	connection(connectionId: AcpConnectionId): AcpConnectionSnapshot | null {
		return state.connections[connectionId]?.snapshot ?? null;
	},

	attachments(connectionId: AcpConnectionId): AcpAttachmentSnapshot[] {
		return state.connections[connectionId]?.snapshot.attachments ?? [];
	},

	attachment(connectionId: AcpConnectionId, sessionId: AcpSessionId): AcpAttachmentSnapshot | null {
		return this.attachments(connectionId).find((a) => a.sessionId === sessionId) ?? null;
	},

	/** The session ego continued this one in after an automatic compaction, if it announced one. */
	successor(connectionId: AcpConnectionId, sessionId: AcpSessionId): AcpSessionId | null {
		return state.connections[connectionId]?.successors[sessionId] ?? null;
	},

	turnOutcome(connectionId: AcpConnectionId, sessionId: AcpSessionId): AcpTurnOutcome | null {
		return state.connections[connectionId]?.outcomes[sessionId] ?? null;
	},

	/** The last sequence handled, or 0 if none has been. */
	cursor(connectionId: AcpConnectionId): number {
		return state.connections[connectionId]?.cursor ?? 0;
	},

	/**
	 * The sequence to subscribe from.
	 *
	 * `subscribe` delivers `>= from`, so one past the last sequence handled is
	 * the only value that neither repeats a frame nor skips one. Nothing handled
	 * yet asks for 0, which means everything the journal still holds — asking for
	 * 1 would skip the first event on a journal that has not yet rolled.
	 */
	resumeFrom(connectionId: AcpConnectionId): number {
		const cursor = this.cursor(connectionId);
		return cursor === 0 ? 0 : cursor + 1;
	},

	interactions(connectionId: AcpConnectionId): AcpPendingInteraction[] {
		return state.connections[connectionId]?.interactions ?? [];
	},

	interaction(connectionId: AcpConnectionId, requestId: string): AcpPendingInteraction | null {
		return this.interactions(connectionId).find((i) => i.requestId === requestId) ?? null;
	},

	gap(connectionId: AcpConnectionId): AcpClientError | null {
		return state.connections[connectionId]?.gap ?? null;
	},

	isStreaming(connectionId: AcpConnectionId): boolean {
		return state.connections[connectionId]?.streaming ?? false;
	},
};
