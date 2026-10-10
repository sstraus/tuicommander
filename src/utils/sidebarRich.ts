import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";
import type { ProgressFlow } from "../stores/progress";

/**
 * Facts the rich sidebar prints under a row. Everything here is a pure read of
 * data the stores already hold; nothing calls the backend.
 */

/** A branch with no commit for this long, and not merged, reads as stale. */
export const STALE_AFTER_DAYS = 30;

const MINUTE = 60_000;

/** `<1m`, `5m`, `3h`, `12d`. `nowMs` and `thenMs` are both milliseconds. */
export function compactAge(thenMs: number | null | undefined, nowMs: number): string {
	if (!thenMs) return "";
	const minutes = Math.max(0, Math.floor((nowMs - thenMs) / MINUTE));
	if (minutes < 1) return "<1m";
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h`;
	return `${Math.floor(hours / 24)}d`;
}

export interface BranchFactsInput {
	/** Unix milliseconds, as `lastCommitTs` holds it. */
	lastCommitTs: number | null;
	ahead?: number;
	behind?: number;
	additions: number;
	deletions: number;
	dirtyFiles: number | null | undefined;
	isMerged: boolean;
	commitStatus?: string;
	/** A main checkout is never removed, so it carries no stale, merged, unknown or dirty fact. */
	isMain?: boolean;
}

export interface BranchFacts {
	commitAge: string | null;
	/** `↑2 ↓1`, either half omitted when zero; null when both are zero. */
	sync: string | null;
	additions: number;
	deletions: number;
	dirtyFiles: number;
	/** `unknown`: removal is blocked because the lifecycle status could not be read. */
	state: "unknown" | "merged" | "stale" | null;
}

export function branchFacts(i: BranchFactsInput, nowMs: number): BranchFacts {
	const commitMs = i.lastCommitTs || null;
	const merged = !i.isMain && (i.isMerged || i.commitStatus === "merged");
	const stale = !i.isMain && !merged && commitMs !== null && nowMs - commitMs > STALE_AFTER_DAYS * 24 * 60 * MINUTE;
	const sync = [i.ahead ? `↑${i.ahead}` : "", i.behind ? `↓${i.behind}` : ""].filter(Boolean).join(" ");
	const unknown = !i.isMain && i.commitStatus === "unknown";
	return {
		commitAge: commitMs ? compactAge(commitMs, nowMs) : null,
		sync: sync || null,
		additions: i.additions,
		deletions: i.deletions,
		dirtyFiles: i.isMain ? 0 : (i.dirtyFiles ?? 0),
		state: unknown ? "unknown" : merged ? "merged" : stale ? "stale" : null,
	};
}

export type AgentRowState = "working" | "idle" | "input" | "error";

export interface AgentFactsInput {
	awaitingInput: "question" | "error" | string | null;
	busy: boolean;
	agentIntent: string | null;
	currentTask: string | null;
	lastPrompt: string | null;
}

export interface AgentFacts {
	state: AgentRowState;
	/** One line of what the agent is doing or was last asked; null when none is known. */
	line: string | null;
}

/** `task` is the already-displayable current task (see `displayTask`). */
export function agentFacts(i: AgentFactsInput, task: string | null): AgentFacts {
	const state: AgentRowState =
		i.awaitingInput === "error" ? "error" : i.awaitingInput === "question" ? "input" : i.busy ? "working" : "idle";
	return {
		state,
		// Blank is absent: an empty intent must not hide the task or the prompt.
		line: [i.agentIntent, task, i.lastPrompt].find((v) => v?.trim()) ?? null,
	};
}

export interface RepoFactsInput {
	currentBranch: string | null;
	openPrs: number;
	worktrees: number;
	/** Milliseconds of the last GitHub/remote poll, 0 when never polled. */
	polledAt: number;
}

export interface RepoFacts {
	currentBranch: string | null;
	openPrs: number;
	worktrees: number;
	syncedAge: string | null;
}

export function repoFacts(i: RepoFactsInput, nowMs: number): RepoFacts {
	return {
		currentBranch: i.currentBranch,
		openPrs: i.openPrs,
		worktrees: i.worktrees,
		syncedAge: i.polledAt ? compactAge(i.polledAt, nowMs) : null,
	};
}

const [sharedNow, setSharedNow] = createSignal(Date.now());
let clockUsers = 0;
let clockTimer: ReturnType<typeof setInterval> | undefined;

/**
 * Wall clock that ticks once a minute, for ages that must not freeze on screen.
 * One interval serves every row: it runs while at least one caller is `active`,
 * so a compact sidebar, which prints no ages, keeps no timer at all.
 */
export function createMinuteClock(active: Accessor<boolean> = () => true): Accessor<number> {
	createEffect(() => {
		if (!active()) return;
		// A late joiner must not read the time of the last tick, up to a minute old.
		setSharedNow(Date.now());
		if (clockUsers++ === 0) {
			clockTimer = setInterval(() => setSharedNow(Date.now()), MINUTE);
		}
		onCleanup(() => {
			if (--clockUsers === 0) clearInterval(clockTimer);
		});
	});
	return sharedNow;
}

/** Trailing separators and backslashes differ between how git and the store spell one checkout. */
export function normalizePath(path: string): string {
	return path.replaceAll("\\", "/").replace(/\/+$/, "");
}

/**
 * Linked worktrees of a repo: every workspace with its own checkout path that is
 * not the repo root itself.
 */
// DEFERRED (2026-10-01) — a symlinked spelling of the repo root still counts as a worktree;
// resolving it needs a realpath, which only the backend has.
export function countWorktrees(repoPath: string, worktreePaths: (string | null | undefined)[]): number {
	const root = normalizePath(repoPath);
	return worktreePaths.filter((p) => p && normalizePath(p) !== root).length;
}

/** More subagents than this collapse into one "N subagents" line. */
export const SUBAGENT_COLLAPSE_AFTER = 3;

export interface SubagentRow {
	id: string;
	title: string;
	running: boolean;
	toolCalls: number;
	/** Compact age: since spawn while running, since the return once done. */
	age: string;
}

/**
 * The in-session subagents of the agent running in PTY session `sessionId`,
 * running ones first. Age comes from the flow's spawn and return events; a
 * subagent with neither has none.
 */
export function subagentRows(flow: ProgressFlow | undefined, sessionId: string | null, nowMs: number): SubagentRow[] {
	if (!flow || !sessionId) return [];
	const at = new Map(flow.events.map((e) => [e.id, e.atMs]));
	const rows = flow.participants
		.filter((p) => p.kind === "subagent" && p.ptyId === sessionId)
		.map((p) => {
			const running = p.state === "running";
			return {
				id: p.id,
				title: p.title,
				running,
				toolCalls: p.toolCalls,
				age: compactAge(at.get(`${p.id}:${running ? "spawn" : "return"}`), nowMs),
			};
		});
	return [...rows.filter((r) => r.running), ...rows.filter((r) => !r.running)];
}
