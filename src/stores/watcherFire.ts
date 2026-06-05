// Frontend handoff for backend watcher fires.
//
// The Rust WatcherEngine emits a `watcher-fire` Tauri event (one path for both
// terminal and PR watchers). This module resolves the referenced smart prompt
// (or falls back to raw instructions) and runs it in the target session.
//
// PR-review fires (those carrying `repoPath`) are routed through the *assisted*
// conversation engine so every outward `gh pr review/approve/comment/merge`
// passes the existing pendingApproval (ui:confirm) gate — the review agent is
// never run fully unrestricted.

import { appLogger } from "./appLogger";
import { conversationStore } from "./conversationStore";
import { prNotificationsStore } from "./prNotifications";
import { promptLibraryStore, type SavedPrompt } from "./promptLibrary";
import { terminalsStore } from "./terminals";

/** Payload of the `watcher-fire` Tauri event (camelCase per serde rename_all). */
export interface WatcherFirePayload {
	ruleId: string;
	sessionId: string;
	repoPath?: string;
	promptId?: string;
	instructions?: string;
	headRefOid?: string;
	prNumber?: number;
	branch?: string;
	context: string;
}

/** Injected dependencies — real implementations in {@link watcherFireDeps}. */
export interface WatcherFireDeps {
	getSmartById: (id: string) => SavedPrompt | undefined;
	/** Focus the target session so executeSmartPrompt / the conversation engine act on it. */
	setActiveSession: (sessionId: string) => void;
	/** Run a smart prompt against the active session (executionMode decides inject/headless/api). */
	executeSmartPrompt: (prompt: SavedPrompt) => Promise<unknown>;
	/** Run a message through the assisted conversation engine (pendingApproval gate). */
	sendAssisted: (content: string, sessionId: string) => Promise<void>;
	/** Run a raw goal as an autonomous agent. */
	startAgent: (sessionId: string, goal: string) => Promise<void>;
	/** Add a "review started" entry to the PR notification bell. */
	notifyReview: (n: { repoPath: string; branch: string; prNumber: number; title: string }) => void;
	warn: (msg: string) => void;
}

/**
 * Dispatch a single watcher-fire payload. Pure control flow over its deps so the
 * routing is unit-testable without Tauri.
 */
export async function handleWatcherFire(payload: WatcherFirePayload, deps: WatcherFireDeps): Promise<void> {
	const isPrFire = !!payload.repoPath;
	const prompt = payload.promptId ? deps.getSmartById(payload.promptId) : undefined;

	if (payload.promptId && !prompt) {
		deps.warn(`watcher-fire: smart prompt "${payload.promptId}" not found`);
	}

	// PR review → assisted conversation so gh pr actions hit the ui:confirm gate.
	// DEFERRED (2026-06-04) — prompt.content is used raw; {branch}/{pr_*} tokens are
	// not substituted on this path. The assisted conversation assembles its own
	// terminal context from the worktree session, so curated PR-review prompts
	// should rely on that rather than template vars. Wire resolve_prompt_variables
	// here if a real PR-review prompt needs substituted tokens.
	if (isPrFire) {
		const content = prompt?.content ?? payload.instructions;
		if (!content) {
			deps.warn(`watcher-fire: PR fire for rule ${payload.ruleId} has neither prompt nor instructions`);
			return;
		}
		// Surface "review started" in the PR bell before kicking off the review.
		if (payload.prNumber != null && payload.branch) {
			deps.notifyReview({
				repoPath: payload.repoPath ?? "",
				branch: payload.branch,
				prNumber: payload.prNumber,
				title: `Reviewing PR #${payload.prNumber}`,
			});
		}
		deps.setActiveSession(payload.sessionId);
		await deps.sendAssisted(content, payload.sessionId);
		return;
	}

	// Terminal watcher with a referenced smart prompt → executeSmartPrompt.
	if (prompt) {
		deps.setActiveSession(payload.sessionId);
		await deps.executeSmartPrompt(prompt);
		return;
	}

	// Fallback: raw instructions → autonomous agent.
	if (payload.instructions) {
		await deps.startAgent(payload.sessionId, payload.instructions);
		return;
	}

	deps.warn(`watcher-fire: rule ${payload.ruleId} had neither prompt nor instructions`);
}

/** Real dependencies wiring the handler to the live stores. */
export function watcherFireDeps(executeSmartPrompt: (prompt: SavedPrompt) => Promise<unknown>): WatcherFireDeps {
	return {
		getSmartById: (id) => promptLibraryStore.getSmartById(id),
		setActiveSession: (sessionId) => {
			// executeSmartPrompt reads terminalsStore.getActive(); the conversation
			// engine keys on the active conversation — set both.
			terminalsStore.setActive(sessionId);
			conversationStore.setActiveTerminal(sessionId);
		},
		executeSmartPrompt,
		sendAssisted: (content, sessionId) => conversationStore.sendMessage(content, sessionId),
		startAgent: (sessionId, goal) => conversationStore.startAgent(sessionId, goal),
		notifyReview: (n) => prNotificationsStore.add({ ...n, type: "review_started" }),
		warn: (msg) => appLogger.warn("ai-agent", msg),
	};
}
