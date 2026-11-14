import { onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { keptCheckoutsStore } from "../stores/keptCheckouts";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { recordWorkspaceCleanup } from "./git/workspaceCleanupDecision";
import type { ConfirmOptions } from "./useConfirmDialog";
import { useRepository } from "./useRepository";

interface AutoDeleteDeps {
	confirm: (options: ConfirmOptions) => Promise<boolean>;
	setStatusInfo: (message: string) => void;
}

/** Retry delays after a transient failure; the list length is the retry budget. */
export const AUTO_DELETE_RETRY_DELAYS_MS = [5_000, 30_000, 120_000];

type Phase = "inflight" | "retry" | "done" | "keep";
interface Entry {
	phase: Phase;
	attempt: number;
	branch: string;
	timer?: ReturnType<typeof setTimeout>;
}
type Outcome = "done" | "retry" | "keep" | "idle";

export interface AutoDeleteControls {
	/** The documented user-controlled reset of an explicit Keep: the next PR notification for it is processed again. */
	resetKeep: (repoPath: string, prNumber: number) => void;
}

/**
 * Cleans up the local branch/checkout of a PR that reached a terminal state.
 *
 * Reads the per-repo `autoDeleteOnPrClose` setting (off/ask/auto):
 * - off: does nothing and consumes nothing — a later notification is still processed
 * - ask: confirm dialog; answering Keep is sticky until `resetKeep`
 * - auto: deletes silently only when a fresh backend inspection proves the work merged
 *   and the checkout safe (no local changes, sessions, locks)
 *
 * Each PR transition is one entry: in-flight (duplicates ignored), retry (transient failure,
 * bounded, always behind a fresh inspection), done, or keep. A closed-unmerged PR is never
 * proof of integration, and nothing is forced: the backend revalidates identity at removal.
 * Safety: never deletes the default/main branch.
 */
export function useAutoDeleteBranch(deps: AutoDeleteDeps): AutoDeleteControls {
	const entries = new Map<string, Entry>();
	const repo = useRepository();

	function handlePrTerminal(repoPath: string, branch: string, prNumber: number, type: "merged" | "closed"): void {
		const key = `${repoPath}:${prNumber}`;
		if (entries.has(key)) return;
		// Off must not consume the transition, so check before recording anything.
		if ((repoSettingsStore.getEffective(repoPath)?.autoDeleteOnPrClose ?? "off") === "off") return;
		const entry: Entry = { phase: "inflight", attempt: 0, branch };
		entries.set(key, entry);
		void run(key, entry, repoPath, branch, prNumber, type);
	}

	async function run(
		key: string,
		entry: Entry,
		repoPath: string,
		branch: string,
		prNumber: number,
		type: "merged" | "closed",
	): Promise<void> {
		entry.phase = "inflight";
		let outcome: Outcome = "retry";
		try {
			outcome = await attempt(repoPath, branch, prNumber, type);
		} catch (err) {
			appLogger.warn("git", `Auto-delete failed for ${branch}`, err);
		}
		if (entries.get(key) !== entry) return; // reset or disposed meanwhile
		if (outcome === "idle") {
			entries.delete(key);
		} else if (outcome === "retry" && entry.attempt < AUTO_DELETE_RETRY_DELAYS_MS.length) {
			entry.phase = "retry";
			entry.timer = setTimeout(
				() => {
					entry.timer = undefined;
					void run(key, entry, repoPath, branch, prNumber, type);
				},
				AUTO_DELETE_RETRY_DELAYS_MS[entry.attempt++],
			);
		} else {
			entry.phase = outcome === "keep" ? "keep" : "done";
			if (outcome === "keep") keptCheckoutsStore.add(repoPath, branch, prNumber);
		}
	}

	async function attempt(
		repoPath: string,
		branch: string,
		prNumber: number,
		type: "merged" | "closed",
	): Promise<Outcome> {
		const mode = repoSettingsStore.getEffective(repoPath)?.autoDeleteOnPrClose ?? "off";
		if (mode === "off") return "idle";

		// A closed PR names its head BRANCH, so the row it belongs to comes from the
		// single branch->id seam (see its DEFERRED note: with two workspaces on one
		// branch, a PR cannot say which checkout to delete).
		const repoState = repositoriesStore.get(repoPath);
		const workspaceId = repositoriesStore.workspaceIdOnBranch(repoPath, branch);
		if (repoState) {
			const branchState = workspaceId ? repoState.workspaces[workspaceId] : undefined;
			if (branchState?.isMain) {
				appLogger.debug("git", `Skipping auto-delete for default branch '${branch}'`);
				return "done";
			}
		}
		// No workspace has it checked out — nothing local to delete.
		if (repoState && !workspaceId) return "done";
		const id = workspaceId ?? branch;

		const inspected = await inspect(repoPath, id);
		if (!inspected) {
			deps.setStatusInfo(`Kept '${branch}': removal preview failed`);
			return "retry";
		}
		const { status, decision } = inspected;
		const warnings = status.warnings ?? [];

		if (mode === "auto") {
			// A closed PR is not merge proof: only the backend's own proof may authorise it.
			if (!decision.eligible) {
				appLogger.info("git", `Skipping auto-delete for '${branch}'`, { reason: decision.reason, warnings });
				deps.setStatusInfo(`Kept '${branch}': ${warnings.join("; ") || decision.detail}`);
				// Unknown proof is transient; unmerged or blocked work stays for this transition.
				return decision.reason === "uncertain" ? "retry" : "done";
			}
		} else {
			const confirmed = await deps.confirm({
				title: "Delete local branch?",
				message: `PR #${prNumber} was ${type}.\nDelete local branch '${branch}'?${decision.eligible ? "" : `\n\n${decision.detail}`}${warnings.length ? `\n\n${warnings.join("\n")}` : ""}`,
				okLabel: "Delete",
				cancelLabel: "Keep",
				kind: "warning",
			});
			if (!confirmed) return "keep";
			// The dialog can stay open for minutes: delete only the exact state the user was shown.
			const again = await inspect(repoPath, id);
			if (
				!again ||
				again.status.tip !== status.tip ||
				again.status.dirtyFingerprint !== status.dirtyFingerprint ||
				(again.status.liveSessions?.length ?? 0) > 0
			) {
				deps.setStatusInfo(`Kept '${branch}': it changed while waiting for confirmation`);
				return again ? "done" : "retry";
			}
		}

		await invoke("delete_local_branch", { repoPath, branchName: branch, workspaceId: id }).catch((err) => {
			appLogger.warn("git", `Failed to delete branch '${branch}': ${err}`);
			throw err;
		});
		repositoriesStore.bumpGitRevision(repoPath);
		appLogger.info("git", `Auto-deleted branch '${branch}' (PR #${prNumber} ${type})`);
		return "done";
	}

	async function inspect(repoPath: string, workspaceId: string) {
		try {
			const status = await repo.getWorkspaceLifecycle(repoPath, workspaceId);
			return { status, decision: recordWorkspaceCleanup(workspaceId, status, Date.now()).decision };
		} catch (error) {
			appLogger.warn("git", `Removal preview failed for '${workspaceId}'`, error);
			return null;
		}
	}

	function resetKeep(repoPath: string, prNumber: number): void {
		const key = `${repoPath}:${prNumber}`;
		const entry = entries.get(key);
		if (entry?.phase !== "keep") return;
		entries.delete(key);
		keptCheckoutsStore.remove(repoPath, entry.branch);
	}

	githubStore.setOnPrTerminal(handlePrTerminal);
	keptCheckoutsStore.setResetHandler(resetKeep);

	onCleanup(() => {
		githubStore.setOnPrTerminal(null);
		keptCheckoutsStore.setResetHandler(null);
		keptCheckoutsStore.clear();
		for (const entry of entries.values()) clearTimeout(entry.timer);
		entries.clear();
	});

	return { resetKeep };
}
