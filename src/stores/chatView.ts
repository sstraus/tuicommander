import { createStore, produce } from "solid-js/store";
import { invoke, listen } from "../invoke";
import type { AcpSessionUpdate } from "../types/acp";
import { acpTranscript } from "./acpTranscript";
import { appLogger } from "./appLogger";
import type { TerminalData } from "./terminals";

/**
 * The transcript view of an agent terminal: what `chat_view_snapshot` returns,
 * folded by the same reducer the AI Chat panel uses.
 *
 * Rust owns the parsing, the bounds and the reset decisions. This file only
 * remembers where the client stopped (`epoch`, `nextSeq`), applies what comes
 * back, and tells a terminal that is not bound apart from one that is.
 */

interface ChatViewSnapshot {
	epoch: number;
	nextSeq: number;
	reset: boolean;
	updates: AcpSessionUpdate[];
	unknownRows: number;
	malformedRows: number;
}

interface Cursor {
	epoch: number | null;
	nextSeq: number;
}

interface ChatViewState {
	cursors: Record<string, Cursor>;
	/** Why the last read of a terminal found nothing to show. */
	unavailable: Record<string, string>;
}

const inFlight = new Map<string, { again: boolean }>();
const watchOwners = new Map<string, symbol>();

const [state, setState] = createStore<ChatViewState>({ cursors: {}, unavailable: {} });

/** The key a terminal's conversation is filed under in `acpTranscript`. */
export function chatViewKey(sessionId: string): string {
	return `pty:${sessionId}`;
}

/**
 * Whether a terminal can show a chat view, and if not, why.
 *
 * Only Claude has a transcript adapter, and only an agent TUIC has bound to a
 * session file is read: a Claude tab with no `agentSessionId` is one the
 * pid registry has not answered for, and showing it would risk another tab's
 * conversation (issue #119).
 */
export function chatViewAvailability(
	term: Pick<TerminalData, "agentType" | "agentSessionId" | "sessionId" | "isRemote"> | undefined,
): { available: true } | { available: false; reason: string } {
	if (!term?.agentType) return { available: false, reason: "No agent is running in this terminal" };
	if (term.agentType !== "claude") return { available: false, reason: "The chat view supports Claude only" };
	if (term.isRemote) return { available: false, reason: "The chat view is not available for remote terminals" };
	if (!term.sessionId) return { available: false, reason: "The terminal has no session yet" };
	if (!term.agentSessionId) return { available: false, reason: "The agent session is not bound to a transcript yet" };
	return { available: true };
}

function isNotBound(error: unknown): string | null {
	const text = error instanceof Error ? error.message : String(error);
	const match = /not_bound: ([^"}]*)/.exec(text);
	return match ? match[1].trim() : null;
}

function warnRefresh(sessionId: string, error: unknown): void {
	appLogger.warn("terminal", "Chat view refresh failed", {
		sessionId,
		error: error instanceof Error ? error.message : String(error),
	});
}

async function readOnce(sessionId: string): Promise<void> {
	const key = chatViewKey(sessionId);
	const cursor = state.cursors[sessionId];
	const owner = watchOwners.get(sessionId);
	// Not watched: nobody asked for this view, or it was closed.
	if (!cursor) return;
	let snapshot: ChatViewSnapshot;
	try {
		snapshot = await invoke<ChatViewSnapshot>("chat_view_snapshot", {
			sessionId,
			epoch: cursor.epoch,
			fromSeq: cursor.nextSeq,
		});
	} catch (error) {
		if (watchOwners.get(sessionId) !== owner) return;
		const reason = isNotBound(error);
		if (reason === null) throw error;
		warnRefresh(sessionId, error);
		setState("unavailable", sessionId, reason);
		return;
	}
	// A reply that arrives after the view was closed must not resurrect state.
	if (!state.cursors[sessionId] || watchOwners.get(sessionId) !== owner) return;
	if (snapshot.reset) acpTranscript.clear(key);
	for (const update of snapshot.updates) {
		acpTranscript.applyFrame({
			kind: "event",
			connectionId: "pty",
			generation: 0,
			sequence: 0,
			sessionId: key,
			turnId: null,
			event: { kind: "sessionUpdate", update },
		});
	}
	setState("cursors", sessionId, { epoch: snapshot.epoch, nextSeq: snapshot.nextSeq });
	chatViewStore.clearUnavailable(sessionId);
}

export const chatViewStore = {
	state,

	/** Why this terminal's chat view could not be read, or null. */
	unavailableReason(sessionId: string): string | null {
		return state.unavailable[sessionId] ?? null;
	},

	/** Record why a terminal's chat view closed itself, for the banner it leaves behind. */
	markUnavailable(sessionId: string, reason: string): void {
		setState("unavailable", sessionId, reason);
	},

	clearUnavailable(sessionId: string): void {
		setState(
			produce((s) => {
				delete s.unavailable[sessionId];
			}),
		);
	},

	/**
	 * Read what is new and fold it into the terminal's conversation. One read
	 * per terminal at a time: a wake that overlaps a read in flight would read
	 * from the same cursor and apply the same chunk twice, so it only asks for
	 * one more read once the current one settles.
	 */
	async refresh(sessionId: string): Promise<void> {
		const running = inFlight.get(sessionId);
		if (running) {
			running.again = true;
			return;
		}
		const run = { again: false };
		inFlight.set(sessionId, run);
		try {
			do {
				run.again = false;
				await readOnce(sessionId);
			} while (run.again);
		} catch (error) {
			warnRefresh(sessionId, error);
			throw error;
		} finally {
			inFlight.delete(sessionId);
		}
	},

	/** Refresh on every wake for one terminal. Returns the disposer. */
	async watch(sessionId: string): Promise<() => void> {
		const owner = Symbol();
		watchOwners.set(sessionId, owner);
		setState("cursors", sessionId, { epoch: null, nextSeq: 0 });
		const unlisten = await listen<{ session_id: string }>("chat-view-changed", (event) => {
			if (event.payload.session_id === sessionId && watchOwners.get(sessionId) === owner) {
				// refresh logs the failure; a later wake/keepalive retries it.
				void chatViewStore.refresh(sessionId).catch(() => {});
			}
		});
		return () => {
			unlisten();
			if (watchOwners.get(sessionId) !== owner) return;
			watchOwners.delete(sessionId);
			setState(
				produce((s) => {
					delete s.cursors[sessionId];
				}),
			);
			acpTranscript.clear(chatViewKey(sessionId));
		};
	},

	/** Tests only. */
	reset(): void {
		inFlight.clear();
		watchOwners.clear();
		setState({ cursors: {}, unavailable: {} });
	},
};
