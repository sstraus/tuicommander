import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import type { ProgressEntry } from "./progress";
import { progressStore } from "./progress";
import { terminalsStore } from "./terminals";

/**
 * Discussion: side questions answered in a read-only fork of a terminal's
 * agent session. Questions and answers are Progress journal entries; this
 * module only asks, continues, cancels and reads the full thread of an entry.
 */

export type DiscussionCategory = "answer" | "action" | "unknown";

export interface DiscussionTurn {
	questionEntryId: number;
	answerEntryId: number | null;
	question: string;
	answer: string | null;
	category: DiscussionCategory | null;
	title: string | null;
	error: string | null;
	cacheReadTokens: number | null;
	cacheCreationTokens: number | null;
	costUsd: number | null;
	finishedMs: number | null;
}

export interface DiscussionThread {
	threadId: number;
	project: string;
	ptyId: string | null;
	/** `side` (forked) or `direct` (captured from the transcript). */
	source: "side" | "direct";
	forkSessionId: string | null;
	running: boolean;
	turns: DiscussionTurn[];
}

/** A fork's cache stays warm this long after its last activity (1 h TTL measured). */
export const WARM_MS = 60 * 60 * 1000;

const [askFocus, setAskFocus] = createSignal(0);

export const discussionStore = {
	ask: (sessionId: string, question: string) => invoke<ProgressEntry>("discussion_ask", { sessionId, question }),
	followUp: (entryId: number, question: string) => invoke<ProgressEntry>("discussion_follow_up", { entryId, question }),
	get: (entryId: number) => invoke<DiscussionThread>("discussion_get", { entryId }),
	cancel: (entryId: number) => invoke<void>("discussion_cancel", { entryId }),

	/** Bumped to move focus into the ask box. */
	askFocus,

	/** Open Progress on the active terminal with the ask box focused. */
	requestAsk(): void {
		const active = terminalsStore.getActive();
		progressStore.open(active?.repoPath ?? null, active?.sessionId ?? null);
		setAskFocus((n) => n + 1);
	},

	/** A follow-up still reads the fork's prompt cache. */
	isWarm(thread: DiscussionThread, now = Date.now()): boolean {
		const last = thread.turns.at(-1)?.finishedMs;
		return thread.forkSessionId !== null && last != null && now - last < WARM_MS;
	},
};
