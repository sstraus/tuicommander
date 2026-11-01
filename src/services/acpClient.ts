/**
 * Everything the panel does to an ACP connection, in one place.
 *
 * The store holds what is true; this holds what to do about it. Both halves of
 * every decision that needs the journal cursor live here — when to resume a
 * stream, when to stop resuming, what sequence to resume from — because the
 * cursor has one owner and a second opinion about it would replay or skip a
 * turn.
 *
 * Nothing here interprets a frame. The store reduces them and the panel renders
 * them; this file only decides which streams exist.
 */

import { invoke } from "../invoke";
import { acpStore } from "../stores/acp";
import { acpTranscript } from "../stores/acpTranscript";
import { appLogger } from "../stores/appLogger";
import type {
	AcpConnectionId,
	AcpConnectionSettlement,
	AcpConnectionSnapshot,
	AcpContentBlock,
	AcpElicitationAction,
	AcpHostRequestId,
	AcpPendingInteraction,
	AcpSessionConfigOption,
	AcpSessionConfigOptionValue,
	AcpSessionId,
	AcpStreamFrame,
	EgoCompactResponse,
} from "../types/acp";
import { randomUuid } from "../utils/randomId";
import { type AcpStreamHandle, type AcpStreamOpener, openAcpStream } from "./acpStream";

export interface ChatLaunch {
	masterPrompt: string;
	executable: string;
	profile: string;
	workspace: string;
	peerId: string;
}
export interface ChatOpenRequest {
	sessionId?: string;
	executable?: string;
	profile?: string;
	workspace?: string;
}
export interface ChatOpened {
	connection: AcpConnectionSnapshot;
	sessionId: AcpSessionId;
	launch: ChatLaunch;
	replayed: boolean;
}

export interface AcpListedSession {
	sessionId: AcpSessionId;
	cwd: string;
	title?: string | null;
	updatedAt?: string | null;
	_meta?: {
		ego?: {
			lineage?: {
				kind?: string;
				sourceSessionId: string;
				rootSessionId?: string;
				atMessageId?: string;
				sourceDeleted?: boolean;
			};
		};
		tuicommander?: { lineageDepth?: number; deleted?: boolean };
	};
}

export interface AcpSessionList {
	sessions: AcpListedSession[];
	nextCursor?: string | null;
}

/**
 * How many times a dropped stream may be reopened before the client stops.
 *
 * A backend that is gone answers every attempt identically, so an unbounded
 * retry is a spin rather than a recovery. Stopping leaves `isStreaming` false
 * with the snapshot intact, which is a state the panel can render and a person
 * can act on. The budget is per unbroken run of failures: one delivered frame
 * is proof the stream works and returns it.
 */
const RESUME_BUDGET = 3;

interface Live {
	handle: AcpStreamHandle | null;
	/** Resumes spent since the last frame arrived. */
	spent: number;
	/** Set by `disconnect`, so the drop its own teardown causes resumes nothing. */
	abandoned: boolean;
}

export function createAcpClient(open: AcpStreamOpener = openAcpStream) {
	const live = new Map<AcpConnectionId, Live>();
	/** Sessions whose `session/load` replay has not yet ended, with the connection that carries it. */
	const replaying = new Map<AcpSessionId, AcpConnectionId>();

	async function subscribe(connectionId: AcpConnectionId, entry: Live): Promise<void> {
		const handle = await open({
			connectionId,
			afterSequence: acpStore.resumeFrom(connectionId),
			onFrame: (frame) => receive(connectionId, entry, frame),
			onDropped: () => {
				void resume(connectionId, entry);
			},
		});
		if (entry.abandoned) {
			// The connection went away while the socket was opening. Closing it
			// here rather than leaving it live is the difference between a stream
			// nobody reads and a stream that keeps a settled connection's entry
			// alive in the store.
			handle.close();
			return;
		}
		entry.handle = handle;
		acpStore.markStreaming(connectionId);
	}

	/**
	 * Hand one frame to the stores, on behalf of the connection it came from.
	 *
	 * The id is passed rather than read off the frame because `end` carries none
	 * — see `acpStore.applyFrame`. This function is the only place that still
	 * knows which stream a frame arrived on, so losing it here is losing it for
	 * good.
	 */
	function receive(connectionId: AcpConnectionId, entry: Live, frame: AcpStreamFrame): void {
		entry.spent = 0;
		acpStore.applyFrame(connectionId, frame);
		acpTranscript.applyFrame(frame);
		if (
			frame.kind === "event" &&
			frame.sessionId &&
			replaying.get(frame.sessionId) === connectionId &&
			frame.event.kind === "sessionUpdate" &&
			frame.event.update.sessionUpdate === "session_info_update"
		) {
			replaying.delete(frame.sessionId);
			acpTranscript.settleReplayed(frame.sessionId);
		}
		if (frame.kind !== "event") {
			// `gap` and `end` are both terminal and neither is recoverable by
			// reading again: a gap means the journal no longer holds what the
			// cursor asks for, an end means there is nothing left to hold.
			entry.abandoned = true;
		}
	}

	async function resume(connectionId: AcpConnectionId, entry: Live): Promise<void> {
		if (entry.abandoned) return;
		if (entry.spent >= RESUME_BUDGET) {
			appLogger.warn("ai-chat", "stream stopped after repeated failures to resume", { connectionId });
			entry.abandoned = true;
			acpStore.markStopped(connectionId);
			return;
		}
		entry.spent += 1;
		try {
			await subscribe(connectionId, entry);
		} catch (error) {
			appLogger.debug("ai-chat", "resuming the stream failed", { connectionId, error });
			void resume(connectionId, entry);
		}
	}

	function forget(connectionId: AcpConnectionId): void {
		const entry = live.get(connectionId);
		if (!entry) return;
		entry.abandoned = true;
		entry.handle?.close();
		live.delete(connectionId);
		for (const [sessionId, owner] of replaying) if (owner === connectionId) replaying.delete(sessionId);
	}

	/**
	 * Take a connection as current: hold its snapshot, its open questions, and a
	 * stream over its journal.
	 *
	 * The questions are fetched rather than waited for. One raised before this
	 * window subscribed is still unanswered and nothing will announce it again,
	 * so a panel that only listened would show an idle session that is in fact
	 * blocked.
	 */
	async function adopt(snapshot: AcpConnectionSnapshot): Promise<AcpConnectionSnapshot> {
		const connectionId = snapshot.connectionId;
		// Before anything is committed. This is the fetch most likely to fail —
		// it is a round trip to a process that has only just started — and a
		// failure here used to leave the store holding a connection with a null
		// handle and no stream, which renders as present and reads as idle.
		const pending = await invoke<AcpPendingInteraction[]>("acp_pending_interactions", { connectionId });

		forget(connectionId);
		// The store is written before the stream opens, not after: `subscribe`
		// reads the cursor and marks the connection streaming, and a frame can
		// arrive the instant the socket is up. A commit ordered after the open
		// would drop those frames as belonging to an unknown connection. The
		// rollback below is what makes the early commit safe.
		const held = acpStore.connection(connectionId) !== null;
		acpStore.applySnapshot(snapshot);
		acpStore.applyInteractions(connectionId, pending);
		const entry: Live = { handle: null, spent: 0, abandoned: false };
		live.set(connectionId, entry);
		try {
			await subscribe(connectionId, entry);
		} catch (error) {
			live.delete(connectionId);
			// Only what this call added. `adopt` always runs against a
			// freshly-minted id today, but a re-adopt of a connection the panel
			// already holds must not erase the cursor it was reading from.
			if (!held) acpStore.forget(connectionId);
			throw error;
		}
		return snapshot;
	}

	/** The backend snapshot owns the complete attachment option set. */
	async function refresh(connectionId: AcpConnectionId): Promise<void> {
		acpStore.applySnapshot(await invoke<AcpConnectionSnapshot>("acp_connection_snapshot", { connectionId }));
	}

	return {
		async openConversation(request: ChatOpenRequest): Promise<ChatOpened> {
			const opened = await invoke<ChatOpened>("acp_chat_open", { request });
			// An attached conversation keeps receiving live output while open is pending.
			// Replace its projection only when the backend actually loaded history,
			// before subscribing to the journal that carries that replay.
			if (opened.replayed) {
				acpTranscript.clear(opened.sessionId);
				replaying.set(opened.sessionId, opened.connection.connectionId);
			}
			await adopt(opened.connection);
			return opened;
		},

		/** Launch ego on a repo root and read everything that connection holds. */
		async connect(root: string, env?: Record<string, string>): Promise<AcpConnectionSnapshot> {
			return adopt(await invoke<AcpConnectionSnapshot>("acp_connect", { root, ...(env !== undefined ? { env } : {}) }));
		},

		/**
		 * Replace a connection with a fresh process on the same root.
		 *
		 * The backend mints a **new** id for the replacement, so the old one has
		 * to be let go by name. Left behind, its entry stays in the connection
		 * list as a connection nobody can reach, and its stream is still open —
		 * which is how the dead connection's `end` frame used to arrive and stop
		 * the fresh one.
		 */
		async reconnect(connectionId: AcpConnectionId, root: string): Promise<AcpConnectionSnapshot> {
			const snapshot = await invoke<AcpConnectionSnapshot>("acp_reconnect", { connectionId, root });
			if (snapshot.connectionId !== connectionId) {
				forget(connectionId);
				acpStore.forget(connectionId);
			}
			return adopt(snapshot);
		},

		/** Read the connection's current picture without touching its stream. */
		refresh,

		async disconnect(connectionId: AcpConnectionId): Promise<AcpConnectionSettlement> {
			forget(connectionId);
			try {
				return await invoke<AcpConnectionSettlement>("acp_disconnect", { connectionId });
			} finally {
				acpStore.forget(connectionId);
			}
		},

		/**
		 * Open a session on the root this connection runs on.
		 *
		 * The authority names a directory and nothing else. What the session may
		 * reach beyond it is decided in Rust: a body naming an MCP server is
		 * refused, not stripped, so sending one would fail the whole request.
		 */
		async newSession(connectionId: AcpConnectionId, cwd: string): Promise<AcpSessionId> {
			const attachment = await invoke<{ sessionId: AcpSessionId }>("acp_session_new", {
				connectionId,
				authority: { cwd, additionalDirectories: [] },
			});
			await this.refresh(connectionId);
			return attachment.sessionId;
		},

		/** Attach to a session ego already owns and replay its history. */
		async loadSession(connectionId: AcpConnectionId, sessionId: AcpSessionId, cwd: string): Promise<void> {
			// `session/load` replays the whole history as updates. Without this the
			// replay lands under what is already shown and every message doubles —
			// so the clear has to come first, and a load that is refused has to put
			// back what it took. The panel stays on this session either way; an
			// erased conversation under a live session reads as history that is
			// gone rather than as a request that failed.
			const cleared = acpTranscript.clear(sessionId);
			// ego ends a replay with a `session_info_update` and sends it after
			// the response, so the frame can beat this call's own return.
			replaying.set(sessionId, connectionId);
			try {
				await invoke("acp_session_load", {
					connectionId,
					sessionId,
					authority: { cwd, additionalDirectories: [] },
				});
			} catch (error) {
				replaying.delete(sessionId);
				acpTranscript.restore(sessionId, cleared);
				throw error;
			}
			await this.refresh(connectionId);
		},

		/**
		 * Branch a session at its tip and return the child's id.
		 *
		 * The child is attached by this call, so it needs no `session/load`. ego
		 * replays history on load, not fork, so the parent's visible prefix is copied across:
		 * the child tab would otherwise open empty on a conversation it carries in
		 * full. A later load clears before it replays, so the copy cannot double.
		 */
		async forkSession(
			connectionId: AcpConnectionId,
			sessionId: AcpSessionId,
			cwd: string,
			atMessageId?: string,
		): Promise<AcpSessionId> {
			const attachment = await invoke<{ sessionId: AcpSessionId }>("acp_session_fork", {
				connectionId,
				sessionId,
				...(atMessageId ? { atMessageId } : {}),
				authority: { cwd, additionalDirectories: [] },
			});
			const entries = acpTranscript.entries(sessionId);
			const selected = atMessageId ? entries.findIndex((entry) => entry.messageId === atMessageId) : -1;
			// ego forks after the completed containing turn, including its later replies
			// and tools. The next user prompt starts the next turn in live and replayed history.
			const nextTurn = entries.findIndex((entry, index) => index > selected && entry.kind === "user");
			const cutoff = atMessageId ? (selected < 0 ? 0 : nextTurn < 0 ? entries.length : nextTurn) : entries.length;
			acpTranscript.restore(
				attachment.sessionId,
				entries.slice(0, cutoff).map((entry) => ({ ...entry, inherited: true })),
			);
			await this.refresh(connectionId);
			return attachment.sessionId;
		},

		async deleteSession(connectionId: AcpConnectionId, sessionId: AcpSessionId): Promise<void> {
			await invoke("acp_session_delete", { connectionId, sessionId });
			// Deletion is already durable. A lost snapshot must not report a refusal
			// and leave an open tab pointing at history the agent has removed.
			try {
				await this.refresh(connectionId);
			} catch (error) {
				appLogger.warn("ai-chat", "refresh after conversation deletion failed", error);
			}
		},

		async closeSession(connectionId: AcpConnectionId, sessionId: AcpSessionId): Promise<void> {
			await invoke("acp_session_close", { connectionId, sessionId });
			await this.refresh(connectionId);
		},

		async listSessions(connectionId: AcpConnectionId, cwd?: string, cursor?: string): Promise<AcpSessionList> {
			return invoke<AcpSessionList>("acp_session_list", {
				connectionId,
				cwd,
				cursor,
			});
		},

		/** Send one turn as ACP content blocks, shared by desktop and remote chat.
		 *  `viewedRepo` is the repository on screen, sent as context for this turn
		 *  only — the session's cwd stays the workspace. */
		async prompt(
			connectionId: AcpConnectionId,
			sessionId: AcpSessionId,
			text: string,
			images: Extract<AcpContentBlock, { type: "image" }>[] = [],
			viewedRepo: string | null = null,
			files: { name: string; path: string }[] = [],
		): Promise<string> {
			if (images.length && !acpStore.connection(connectionId)?.capabilities?.promptImage) {
				throw new Error("This agent does not support images.");
			}
			// Resource links are baseline ACP content blocks. Include readable paths as
			// text too, so agents that ignore links can still open the files.
			const filePaths = files.map((file) => `@${file.path}`).join("\n");
			const promptText = [text, filePaths].filter(Boolean).join("\n\n");
			const prompt: AcpContentBlock[] = [
				...(promptText.trim() ? [{ type: "text" as const, text: promptText }] : []),
				...images,
				...files.map((file) => {
					const path = file.path.replaceAll("\\", "/");
					const windowsDrive = /^[A-Za-z]:\//.test(path);
					const parts = path
						.split("/")
						.map((part, index) => (windowsDrive && index === 0 ? part : encodeURIComponent(part)));
					return {
						type: "resource_link" as const,
						uri: `file://${windowsDrive ? "/" : ""}${parts.join("/")}`,
						name: file.name,
					};
				}),
			];
			return invoke<string>("acp_session_prompt", { connectionId, sessionId, prompt, viewedRepo });
		},

		async cancel(connectionId: AcpConnectionId, sessionId: AcpSessionId): Promise<void> {
			await invoke("acp_session_cancel", { connectionId, sessionId });
		},

		async cancelQueued(connectionId: AcpConnectionId, sessionId: AcpSessionId, turnId: string): Promise<void> {
			await invoke("acp_queued_prompt_cancel", { connectionId, sessionId, turnId });
		},

		async recallQueued(connectionId: AcpConnectionId, sessionId: AcpSessionId, turnId: string): Promise<string> {
			return invoke<string>("acp_queued_prompt_recall", { connectionId, sessionId, turnId });
		},

		/** Answer with one of the option ids the agent published, never another. */
		async answerPermission(
			connectionId: AcpConnectionId,
			requestId: AcpHostRequestId,
			optionId: string,
		): Promise<void> {
			await invoke("acp_respond_permission", {
				connectionId,
				requestId,
				outcome: { outcome: "selected", optionId },
			});
		},

		async cancelPermission(connectionId: AcpConnectionId, requestId: AcpHostRequestId): Promise<void> {
			await invoke("acp_respond_permission", { connectionId, requestId, outcome: { outcome: "cancelled" } });
		},

		async answerElicitation(
			connectionId: AcpConnectionId,
			requestId: AcpHostRequestId,
			action: AcpElicitationAction,
		): Promise<void> {
			await invoke("acp_respond_elicitation", { connectionId, requestId, action });
		},

		/** Set one of the knobs the session published, and take back the new list. */
		async setConfigOption(
			connectionId: AcpConnectionId,
			sessionId: AcpSessionId,
			configId: string,
			value: AcpSessionConfigOptionValue,
		): Promise<AcpSessionConfigOption[]> {
			const options = await invoke<AcpSessionConfigOption[]>("acp_session_set_config_option", {
				connectionId,
				sessionId,
				configId,
				value,
			});
			await this.refresh(connectionId);
			return options;
		},

		async pause(connectionId: AcpConnectionId, sessionId: AcpSessionId) {
			return invoke("acp_turn_pause", { connectionId, sessionId, requestId: randomUuid() });
		},

		async resumeTurn(connectionId: AcpConnectionId, sessionId: AcpSessionId) {
			return invoke("acp_turn_resume", { connectionId, sessionId, requestId: randomUuid() });
		},

		async compact(connectionId: AcpConnectionId, sessionId: AcpSessionId) {
			return invoke<EgoCompactResponse>("acp_session_compact", { connectionId, sessionId, requestId: randomUuid() });
		},
	};
}

export const acpClient = createAcpClient();
