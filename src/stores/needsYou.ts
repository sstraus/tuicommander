import { createMemo, createRoot } from "solid-js";
import type { AgentType } from "../agents";
import { repositoriesStore } from "./repositories";
import { terminalsStore } from "./terminals";

/** One session waiting for the user, as shown by the toolbar pill and the sidebar banner. */
export interface WaitingEntry {
	id: string;
	kind: "question" | "error";
	repoPath: string | null;
	repoName: string | null;
	branchName: string | null;
	agentType: AgentType | null;
	question: string | null;
	/** When this session was first seen waiting in this window (ms epoch). */
	since: number;
}

/** First-seen time per waiting terminal. Kept only while the terminal keeps waiting. */
const firstSeen = new Map<string, number>();

function placement(id: string): Pick<WaitingEntry, "repoPath" | "repoName" | "branchName"> {
	const repoPath = repositoriesStore.getRepoPathForTerminal(id);
	const repo = repoPath ? repositoriesStore.state.repositories[repoPath] : undefined;
	const workspace = repo && Object.values(repo.workspaces).find((ws) => ws.terminals.includes(id));
	return { repoPath, repoName: repo?.displayName ?? null, branchName: workspace?.branchName ?? null };
}

function computeEntries(): WaitingEntry[] {
	const now = Date.now();
	const entries: WaitingEntry[] = [];
	for (const id of terminalsStore.getIds()) {
		const term = terminalsStore.get(id);
		const kind = term?.awaitingInput;
		if (!term || !kind || terminalsStore.isDetached(id)) continue;
		if (!firstSeen.has(id)) firstSeen.set(id, now);
		entries.push({
			id,
			kind,
			...placement(id),
			agentType: term.agentType,
			question: term.questionText?.trim() || null,
			since: firstSeen.get(id) as number,
		});
	}
	const live = new Set(entries.map((entry) => entry.id));
	for (const id of firstSeen.keys()) if (!live.has(id)) firstSeen.delete(id);
	return entries.sort((a, b) => a.since - b.since || a.id.localeCompare(b.id));
}

let memo: (() => WaitingEntry[]) | undefined;

/** Waiting sessions, oldest first: the one forgotten longest comes up first. */
export function waitingEntries(): WaitingEntry[] {
	memo ??= createRoot(() => createMemo(computeEntries));
	return memo();
}

/** Waiting sessions owned by one repository. */
export function waitingCountForRepo(repoPath: string): number {
	return waitingEntries().filter((entry) => entry.repoPath === repoPath).length;
}

/** Short age label: "now", "4m", "2h". */
export function waitingAge(since: number, now = Date.now()): string {
	const minutes = Math.floor((now - since) / 60_000);
	if (minutes < 1) return "now";
	if (minutes < 60) return `${minutes}m`;
	return `${Math.floor(minutes / 60)}h`;
}
