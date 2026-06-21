import { open } from "@tauri-apps/plugin-dialog";
import { batch, createSignal } from "solid-js";
import type { WorktreeCreateOptions } from "../components/CreateWorktreeDialog";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { paneLayoutStore } from "../stores/paneLayout";
import { repoSettingsStore } from "../stores/repoSettings";
import { type RepositoryState, repositoriesStore } from "../stores/repositories";
import { paneLayoutKey, savedPaneLayouts } from "../stores/savedPaneLayouts";
import { terminalsStore } from "../stores/terminals";
import { isTauri, rpc } from "../transport";
import type { RepoInfo } from "../types";
import { verifyAndBuildResumeCommand } from "../utils/agentSession";
import { assignTabToActiveGroup } from "../utils/paneTabAssign";
import { pathStartsWith } from "../utils/pathUtils";
import { markPerf, timeSync } from "../utils/perfTrace";
import { effectiveMergeMethod, isMergeMethodNotAllowed } from "../utils/prMerge";
import { filterValidTerminals } from "../utils/terminalFilter";
import { findOrphanTerminals } from "../utils/terminalOrphans";

/** Dependencies injected into useGitOperations */
export interface GitOperationsDeps {
	repo: {
		getInfo: (path: string) => Promise<{
			path: string;
			name: string;
			initials: string;
			branch: string;
			status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown";
			is_git_repo: boolean;
		}>;
		getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
		getWorktreePaths: (repoPath: string) => Promise<Record<string, string>>;
		getRepoSummary: (repoPath: string) => Promise<{
			worktree_paths: Record<string, string>;
			merged_branches: string[];
			diff_stats: Record<string, { additions: number; deletions: number }>;
			last_commit_ts: Record<string, number | null>;
		}>;
		getRepoStructure: (repoPath: string) => Promise<{
			worktree_paths: Record<string, string>;
			merged_branches: string[];
		}>;
		getRepoDiffStats: (repoPath: string) => Promise<{
			diff_stats: Record<string, { additions: number; deletions: number }>;
			last_commit_ts: Record<string, number | null>;
		}>;
		removeWorktree: (repoPath: string, branchName: string, deleteBranch: boolean, force?: boolean) => Promise<void>;
		createWorktree: (
			baseRepo: string,
			branchName: string,
			createBranch?: boolean,
			baseRef?: string,
		) => Promise<{ status: "ok" | "pending"; name: string; path: string; branch: string; base_repo: string }>;
		renameBranch: (repoPath: string, oldName: string, newName: string) => Promise<void>;
		createBranch: (repoPath: string, name: string, startPoint: string | null, checkout: boolean) => Promise<void>;
		generateWorktreeName: (existingNames: string[]) => Promise<string>;
		generateCloneBranchName: (sourceBranch: string, existingNames: string[]) => Promise<string>;
		listBaseRefOptions: (repoPath: string) => Promise<import("./useRepository").BaseRefOption[]>;
		mergeAndArchiveWorktree: (
			repoPath: string,
			branchName: string,
			targetBranch: string,
			afterMerge: string,
		) => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
		finalizeMergedWorktree: (
			repoPath: string,
			branchName: string,
			action: "archive" | "delete",
		) => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
		listLocalBranches: (repoPath: string) => Promise<string[]>;
		getMergedBranches: (repoPath: string) => Promise<string[]>;
		checkoutRemoteBranch: (repoPath: string, branchName: string) => Promise<void>;
		detectOrphanWorktrees: (repoPath: string) => Promise<string[]>;
		removeOrphanWorktree: (repoPath: string, worktreePath: string) => Promise<void>;
		mergePrViaGithub: (repoPath: string, prNumber: number, mergeMethod: string) => Promise<string>;
		switchBranch: (
			repoPath: string,
			branchName: string,
			opts?: { force?: boolean; stash?: boolean },
		) => Promise<{ success: boolean; stashed: boolean; previous_branch: string; new_branch: string }>;
		runSetupScript: (script: string, cwd: string) => Promise<{ exit_code: number; stdout: string; stderr: string }>;
	};
	pty: {
		canSpawn: () => Promise<boolean>;
		write: (sessionId: string, data: string) => Promise<void>;
		getWorktreesDir: (repoPath?: string) => Promise<string>;
	};
	dialogs: {
		confirmRemoveRepo: (repoName: string) => Promise<boolean>;
		confirmRemoveWorktree: (branchName: string) => Promise<boolean>;
		confirmRemoveLockedWorktree?: (branchName: string, deleteBranch?: boolean) => Promise<boolean>;
		confirmStashAndSwitch?: (branchName: string) => Promise<boolean>;
		confirmOrphanCleanup?: (paths: string[]) => Promise<boolean>;
		/** Surface a git failure in a dialog with the full output; returns true if the user chose Retry. */
		reportGitError?: (title: string, detail: string, offerRetry?: boolean) => Promise<boolean>;
		/** Browser mode only: show an in-app text-input dialog to enter a repo path */
		promptRepoPath?: () => Promise<string | null>;
	};
	closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
	createNewTerminal: () => Promise<string | undefined>;
	setStatusInfo: (msg: string) => void;
	getDefaultFontSize: () => number;
	getMaxTabNameLength: () => number;
	/** Returns the effective promptOnCreate setting for the given repo.
	 *  When false, handleAddWorktree skips the dialog and creates instantly.
	 *  Defaults to true (show dialog) when not provided. */
	getPromptOnCreate?: (repoPath: string) => boolean;
}

/** Git and repository operations extracted from App.tsx */
export function useGitOperations(deps: GitOperationsDeps) {
	const [currentRepoPath, setCurrentRepoPath] = createSignal<string | undefined>(undefined);
	const [currentBranch, setCurrentBranch] = createSignal<string | null>(null);
	const [repoStatus, setRepoStatus] = createSignal<"clean" | "dirty" | "conflict" | "merge" | "unknown">("unknown");
	const [branchToRename, setBranchToRename] = createSignal<{ repoPath: string; branchName: string } | null>(null);
	const [branchToCreate, setBranchToCreate] = createSignal<{ repoPath: string; startPoint: string | null } | null>(
		null,
	);
	const [creatingWorktreeRepos, setCreatingWorktreeRepos] = createSignal<Set<string>>(new Set());
	// Key: `${repoPath}::${branchName}` — prevents concurrent remove calls for same branch
	const [removingBranches, setRemovingBranches] = createSignal<Set<string>>(new Set());

	// Pending creates whose Rust background recreation is still in-flight. When the
	// async create_worktree returned status:'pending', we deferred running the
	// setup script / spawning the initial terminal until the worktree files
	// actually exist. The refresh handler drains this map once `isPreparing` is
	// cleared for a branch (success path). On `worktree-create-failed` (Rust
	// error path) the entry is removed without running setup.
	type PendingCreation = {
		repoPath: string;
		displayName: string;
		result: { name: string; path: string; branch: string; base_repo: string };
	};
	const pendingCreations = new Map<string, PendingCreation>(); // key: `${repoPath}::${branchName}`
	const pendingKey = (repoPath: string, branchName: string) => `${repoPath}::${branchName}`;
	const [worktreeDialogState, setWorktreeDialogState] = createSignal<{
		repoPath: string;
		suggestedName: string;
		existingBranches: string[];
		worktreeBranches: string[];
		worktreesDir: string;
		baseRefs: import("./useRepository").BaseRefOption[];
	} | null>(null);

	/** Pending merge context — set when afterMerge=ask; cleared once the user picks or skips cleanup */
	const [mergePendingCtx, setMergePendingCtx] = createSignal<{
		repoPath: string;
		branchName: string;
		baseBranch: string;
		hasDirtyFiles: boolean;
	} | null>(null);

	/** Serialization queue: forces concurrent handleBranchSelect calls to run
	 *  strictly one-at-a-time (e.g. rapid sidebar clicks, quick-switcher), so they
	 *  can't duplicate terminals or race the pane layout. A FIFO promise chain —
	 *  NOT a single "await the in-flight promise" guard, which lets 3+ callers all
	 *  wake from the same promise and run concurrently. */
	let branchSelectQueue: Promise<void> = Promise.resolve();

	/** Transition a repo from git to shell mode (e.g. .git was removed) */
	const transitionToShell = (repoPath: string, currentRepo: RepositoryState) => {
		batch(() => {
			// Migrate all terminals to a shell branch
			const allTerminals: string[] = [];
			for (const branch of Object.values(currentRepo.branches)) {
				allTerminals.push(...branch.terminals);
				repositoriesStore.removeBranch(repoPath, branch.name);
			}
			repositoriesStore.setIsGitRepo(repoPath, false);
			const shellBranch = "shell";
			repositoriesStore.setBranch(repoPath, shellBranch, {
				worktreePath: repoPath,
				isMain: true,
				isShell: true,
			});
			for (const termId of allTerminals) {
				repositoriesStore.addTerminalToBranch(repoPath, shellBranch, termId);
			}
			repositoriesStore.setActiveBranch(repoPath, shellBranch);
		});
	};

	// Per-repo refresh generation counter — discards stale Phase 2 writes
	// when a newer refresh has started for the same repo.
	const refreshGeneration = new Map<string, number>();

	// Branch removals processed recently, keyed by `${repoPath}::${branchName}`.
	// FSEvents fires multiple repo-changed bursts when a worktree is deleted
	// (one for .git/worktrees/<name>, one for the worktree directory itself),
	// which can schedule overlapping refresh cycles. Without dedup, the same
	// terminals would be force-closed twice and the same branch removed twice,
	// racing store subscribers and causing visible UI thrash. Entries expire
	// after PROCESS_DEDUP_WINDOW_MS so a legitimate later re-creation is not
	// blocked indefinitely.
	const recentlyProcessedBranches = new Map<string, number>();
	const PROCESS_DEDUP_WINDOW_MS = 2000;

	// Grace period: branches just created via setupNewWorktree are protected from
	// refresh-triggered removal for CREATION_GRACE_WINDOW_MS. This guards against
	// the race where git hasn't fully registered the new worktree by the time the
	// first repo-changed refresh fires (idempotent dir-exists path, slow FS, etc.).
	const recentlyCreatedBranches = new Map<string, number>();
	// Bumped from 5s → 60s to cover the worst-case background stale-recovery
	// flow (large checkout, LFS, slow FS). 5s was shorter than the typical
	// recreate window, so the failure path silently removed the placeholder
	// before the grace expired.
	const CREATION_GRACE_WINDOW_MS = 60_000;
	const markRecentlyCreated = (repoPath: string, branchName: string): void => {
		const now = Date.now();
		for (const [k, ts] of recentlyCreatedBranches) {
			if (now - ts > CREATION_GRACE_WINDOW_MS) recentlyCreatedBranches.delete(k);
		}
		recentlyCreatedBranches.set(`${repoPath}::${branchName}`, now);
	};
	const isRecentlyCreated = (repoPath: string, branchName: string): boolean => {
		const key = `${repoPath}::${branchName}`;
		const ts = recentlyCreatedBranches.get(key);
		if (ts === undefined) return false;
		if (Date.now() - ts > CREATION_GRACE_WINDOW_MS) {
			recentlyCreatedBranches.delete(key);
			return false;
		}
		return true;
	};

	const alreadyProcessed = (repoPath: string, branchName: string): boolean => {
		const key = `${repoPath}::${branchName}`;
		const ts = recentlyProcessedBranches.get(key);
		if (ts === undefined) return false;
		if (Date.now() - ts > PROCESS_DEDUP_WINDOW_MS) {
			recentlyProcessedBranches.delete(key);
			return false;
		}
		return true;
	};
	const markProcessed = (repoPath: string, branchName: string): void => {
		const now = Date.now();
		// Sweep expired entries on every write so the map stays bounded by the
		// number of branches removed within PROCESS_DEDUP_WINDOW_MS — without
		// this, branches removed and never re-queried leak forever.
		for (const [k, ts] of recentlyProcessedBranches) {
			if (now - ts > PROCESS_DEDUP_WINDOW_MS) recentlyProcessedBranches.delete(k);
		}
		recentlyProcessedBranches.set(`${repoPath}::${branchName}`, now);
	};

	const refreshAllBranchStats = async () => {
		// Skip parked repos — they should stay dormant. (#1358-caf5)
		await Promise.all(
			repositoriesStore.getActivePaths().map(async (repoPath) => {
				const gen = (refreshGeneration.get(repoPath) ?? 0) + 1;
				refreshGeneration.set(repoPath, gen);

				const repo = repositoriesStore.get(repoPath);
				if (!repo) return;
				// Snapshot branch keys before any await so we can detect user-triggered
				// removals that happen while async ops are in-flight (race condition guard).
				const priorBranchKeys = new Set(Object.keys(repo.branches));
				// Non-git directories: check if they became a git repo
				if (repo.isGitRepo === false) {
					try {
						const info = await deps.repo.getInfo(repoPath);
						if (info.is_git_repo && info.branch) {
							// Directory gained .git — transition to git mode
							batch(() => {
								for (const branch of Object.values(repo.branches)) {
									repositoriesStore.removeBranch(repoPath, branch.name);
								}
								repositoriesStore.setIsGitRepo(repoPath, true);
								repositoriesStore.setBranch(repoPath, info.branch, { worktreePath: repoPath });
								repositoriesStore.setActiveBranch(repoPath, info.branch);
							});
							// Restart the repo watcher so it registers the now-present .git
							// sub-watches (HEAD/refs/worktrees). On macOS/Windows the recursive
							// root watch already covers .git, but Linux uses targeted watches
							// that were skipped while the directory was non-git.
							invoke("stop_repo_watcher", { repoPath })
								.then(() => invoke("start_repo_watcher", { repoPath }))
								.catch((e) => appLogger.debug("git", "Watcher restart after git-init failed", { repoPath, error: String(e) }));
						}
					} catch (e) {
						appLogger.debug("git", "Repo probe failed — staying in shell mode", { repoPath, error: String(e) });
					}
					return;
				}

				// === PHASE 1: Structure (fast) ===
				// Returns worktree_paths + merged_branches only — no expensive diff stats.
				const structure = await deps.repo.getRepoStructure(repoPath);
				if (refreshGeneration.get(repoPath) !== gen) return; // stale

				const worktreePaths = structure.worktree_paths;
				const mergedSet = new Set(structure.merged_branches);

				const currentRepo = repositoriesStore.get(repoPath);
				if (!currentRepo) return;

				if (Object.keys(worktreePaths).length === 0) {
					// Worktrees came back empty — either a transient backend error or the
					// repo is no longer a git repo. Probe to find out.
					try {
						const info = await deps.repo.getInfo(repoPath);
						if (!info.is_git_repo) {
							transitionToShell(repoPath, currentRepo);
							return;
						}
					} catch (e) {
						appLogger.debug("git", "getInfo failed — preserving UI state", { repoPath, error: String(e) });
					}
					// Still a git repo but no worktrees returned — skip to avoid
					// destroying existing branch state on a transient error.
					if (Object.keys(currentRepo.branches).length > 0) return;
				}

				// Compute the target set of branches to keep, then apply all
				// mutations in a single batch to prevent intermediate renders
				// (which caused the sidebar to flash/jump during refresh).
				const storeIds = new Set(terminalsStore.getIds());
				const toRemove: string[] = [];
				const terminalsToClose: string[] = [];

				// If activeBranch is no longer a worktree, find its replacement:
				// the worktree branch that occupies the same path (HEAD moved).
				let activeBranchReplacement: string | null = null;
				const active = currentRepo.activeBranch;
				if (active && !(active in worktreePaths)) {
					const activePath = currentRepo.branches[active]?.worktreePath;
					if (activePath) {
						for (const [wtBranch, wtPath] of Object.entries(worktreePaths)) {
							if (wtPath === activePath) {
								activeBranchReplacement = wtBranch;
								break;
							}
						}
					}
				}

				for (const branchName of Object.keys(currentRepo.branches)) {
					if (!(branchName in worktreePaths)) {
						// Skip branches that a concurrent/recent refresh already handled.
						// The store removal may not have settled yet (batch scheduled), so
						// we'd otherwise re-enqueue the same close+remove.
						if (alreadyProcessed(repoPath, branchName)) continue;
						// Skip branches just created — git may not have fully registered the
						// worktree by the time the first repo-changed refresh fires.
						if (isRecentlyCreated(repoPath, branchName)) {
							appLogger.info("git", `refreshAllBranchStats: CREATION GRACE skipping "${branchName}" (just created)`, {
								repoPath,
							});
							continue;
						}
						// If this is the stale activeBranch and we found a replacement, allow removal
						if (branchName === active && activeBranchReplacement) {
							appLogger.info(
								"terminal",
								`refreshAllBranchStats: activeBranch "${branchName}" replaced by "${activeBranchReplacement}"`,
							);
							toRemove.push(branchName);
							markProcessed(repoPath, branchName);
							continue;
						}
						// Branch has live terminals — only keep it if the worktree path
						// is the main repo checkout (HEAD switched away). If the worktree
						// directory was deleted externally, close the orphaned terminals
						// so the stale branch can be cleaned up.
						const branchState = currentRepo.branches[branchName];
						const hasLiveTerminals = branchState?.terminals.some((id) => storeIds.has(id));
						if (hasLiveTerminals) {
							const isLinkedWorktree = branchState.worktreePath && branchState.worktreePath !== repoPath;
							if (isLinkedWorktree) {
								// Linked worktree was removed externally — close its terminals
								appLogger.info(
									"terminal",
									`refreshAllBranchStats: closing terminals for deleted worktree "${branchName}"`,
									{
										terminals: branchState.terminals,
										worktreePath: branchState.worktreePath,
									},
								);
								terminalsToClose.push(...branchState.terminals.filter((id) => storeIds.has(id)));
								toRemove.push(branchName);
								markProcessed(repoPath, branchName);
							} else {
								appLogger.info("terminal", `refreshAllBranchStats: keeping "${branchName}" — has live terminals`, {
									terminals: branchState.terminals,
								});
							}
							continue;
						}
						toRemove.push(branchName);
						markProcessed(repoPath, branchName);
					}
				}

				if (toRemove.length > 0) {
					appLogger.info("terminal", `refreshAllBranchStats removing branches from ${repoPath}`, {
						toRemove,
						worktreePathKeys: Object.keys(worktreePaths),
						existingBranches: Object.keys(currentRepo.branches),
					});
				}

				// Close terminals for deleted worktrees before mutating store state.
				// Best-effort: a PTY may already be dead; log and continue so the
				// branch removal in the batch below is not blocked.
				for (const termId of terminalsToClose) {
					try {
						await deps.closeTerminal(termId, true);
					} catch (err) {
						appLogger.warn("terminal", `refreshAllBranchStats: failed to close terminal ${termId}`, err);
					}
				}

				const drainedPendings: PendingCreation[] = [];
				// Freeze-investigation: time the synchronous store-mutation batch per repo.
				timeSync(`git.refreshBatch:${repoPath}`, () =>
					batch(() => {
						// Guard against race: if a branch was present before our async ops
						// but is now gone from the live store, the user deleted it while we
						// were in-flight. Don't resurrect it via stale worktreePaths data.
						const liveRepo = repositoriesStore.get(repoPath);
						// Create new worktree branches first so mergeBranchState has a target
						for (const [branchName, wtPath] of Object.entries(worktreePaths)) {
							if (priorBranchKeys.has(branchName) && !liveRepo?.branches[branchName]) {
								appLogger.info("git", `refreshAllBranchStats: RACE GUARD blocked resurrection of "${branchName}"`, {
									repoPath,
									worktreePath: wtPath,
								});
								continue;
							}
							const update: Partial<import("../stores/repositories").BranchState> = {
								worktreePath: wtPath,
								isMerged: mergedSet.has(branchName),
							};
							// Branch finished background preparation — clear placeholder state
							// and queue the deferred setupNewWorktree (setup script, initial
							// terminal, runScript) for after the batch commits.
							if (liveRepo?.branches[branchName]?.isPreparing) {
								update.isPreparing = false;
								const k = pendingKey(repoPath, branchName);
								const pend = pendingCreations.get(k);
								if (pend) {
									pendingCreations.delete(k);
									drainedPendings.push(pend);
								}
							}
							repositoriesStore.setBranch(repoPath, branchName, update);
						}
						// Migrate terminal state from stale activeBranch to its replacement
						if (active && activeBranchReplacement && toRemove.includes(active)) {
							repositoriesStore.mergeBranchState(repoPath, active, activeBranchReplacement);
							repositoriesStore.setActiveBranch(repoPath, activeBranchReplacement);
						}
						for (const branchName of toRemove) {
							repositoriesStore.removeBranch(repoPath, branchName);
						}
					}),
				);

				// Drain pending creates: their backing worktree directory finally
				// exists, so it's safe to run the setup script + spawn the initial
				// terminal. Releases the per-repo creatingWorktreeRepos lock that
				// confirmCreateWorktree/handleCreateWorktreeFromBranch held open.
				for (const pend of drainedPendings) {
					try {
						await setupNewWorktree(pend.repoPath, pend.result, pend.displayName);
					} catch (err) {
						appLogger.error("git", `setupNewWorktree (pending drain) failed for ${pend.result.branch}`, err);
					}
					setCreatingWorktreeRepos((prev) => {
						const next = new Set(prev);
						next.delete(pend.repoPath);
						return next;
					});
				}

				const updatedRepo = repositoriesStore.get(repoPath);
				if (!updatedRepo) return;

				// Side effects that only need structure data — run before Phase 2
				await handleAutoArchiveMerged(repoPath, updatedRepo.branches);
				await handleOrphanCleanup(repoPath);

				// === PHASE 2: Stats (slow) ===
				// Per-worktree diff stats + last-commit timestamps.
				// Non-fatal: if this fails, UI shows rows from Phase 1 with stale/zero stats.
				if (refreshGeneration.get(repoPath) !== gen) return; // stale check before Phase 2

				try {
					const stats = await deps.repo.getRepoDiffStats(repoPath);
					if (refreshGeneration.get(repoPath) !== gen) return; // stale after await

					const currentRepoForStats = repositoriesStore.get(repoPath);
					if (!currentRepoForStats) return;

					batch(() => {
						for (const branch of Object.values(currentRepoForStats.branches)) {
							if (!branch.worktreePath) continue;
							const ds = stats.diff_stats[branch.worktreePath];
							if (ds) {
								repositoriesStore.updateBranchStats(repoPath, branch.name, ds.additions, ds.deletions);
							}
							const ts = stats.last_commit_ts?.[branch.name];
							if (ts !== undefined) {
								// Rust emits Unix seconds (%ct); JS Date.now() uses milliseconds
								repositoriesStore.setBranch(repoPath, branch.name, { lastCommitTs: ts !== null ? ts * 1000 : null });
							}
						}
					});
				} catch (err) {
					appLogger.warn("git", `Phase 2 diff stats failed for ${repoPath}`, err);
				}
			}),
		);
	};

	/** Detect orphaned linked worktrees and act based on the orphanCleanup setting. */
	let orphanDialogOpen = false;
	// Orphans the user chose to "Keep" this session — don't nag about them again
	// on every subsequent refresh/poll. Session-scoped (re-detected on next launch). (#65)
	const keptOrphans = new Set<string>();
	const handleOrphanCleanup = async (repoPath: string) => {
		const orphanCleanup = repoSettingsStore.getEffective(repoPath)?.orphanCleanup ?? "ask";
		if (orphanCleanup === "off") return;

		let orphanPaths: string[];
		try {
			orphanPaths = await deps.repo.detectOrphanWorktrees(repoPath);
		} catch {
			return; // Detection failure is non-fatal
		}
		if (orphanPaths.length === 0) return;

		if (orphanCleanup === "on") {
			// Auto-remove silently
			for (const wtPath of orphanPaths) {
				try {
					await closeTerminalsInWorktree(wtPath);
					await deps.repo.removeOrphanWorktree(repoPath, wtPath);
				} catch (err) {
					appLogger.warn("git", `Failed to auto-remove orphan worktree ${wtPath}`, err);
				}
			}
			deps.setStatusInfo(`Removed ${orphanPaths.length} orphaned worktree(s)`);
			return;
		}

		// orphanCleanup === "ask"
		// Skip orphans the user already chose to keep — otherwise the dialog re-fires
		// on every refresh until the underlying worktree state changes. (#65)
		const pending = orphanPaths.filter((p) => !keptOrphans.has(p));
		if (pending.length === 0) return;

		if (orphanDialogOpen) return; // Prevent duplicate dialogs from concurrent refreshes
		orphanDialogOpen = true;
		let confirmed: boolean;
		try {
			confirmed = (await deps.dialogs.confirmOrphanCleanup?.(pending)) ?? false;
		} finally {
			orphanDialogOpen = false;
		}
		if (!confirmed) {
			// User chose "Keep" — remember these so we don't prompt again this session.
			for (const p of pending) keptOrphans.add(p);
			return;
		}

		for (const wtPath of pending) {
			try {
				await closeTerminalsInWorktree(wtPath);
				await deps.repo.removeOrphanWorktree(repoPath, wtPath);
			} catch (err) {
				appLogger.warn("git", `Failed to remove orphan worktree ${wtPath}`, err);
			}
		}
		deps.setStatusInfo(`Removed ${pending.length} orphaned worktree(s)`);
	};

	/** Archive all merged linked worktrees when the autoArchiveMerged setting is enabled. */
	const handleAutoArchiveMerged = async (repoPath: string, branches: RepositoryState["branches"]) => {
		if (!repoSettingsStore.getEffective(repoPath)?.autoArchiveMerged) return;

		const mergedLinkedBranches = Object.values(branches).filter(
			(b) => b.isMerged && b.worktreePath !== null && b.worktreePath !== repoPath,
		);
		if (mergedLinkedBranches.length === 0) return;

		let archived = 0;
		for (const branch of mergedLinkedBranches) {
			try {
				await deps.repo.finalizeMergedWorktree(repoPath, branch.name, "archive");
				archived++;
			} catch (err) {
				appLogger.warn("git", `Failed to auto-archive merged worktree for "${branch.name}"`, err);
			}
		}
		if (archived > 0) {
			deps.setStatusInfo(`Auto-archived ${archived} merged worktree(s)`);
		}
	};

	const handleAddTerminalToBranch = async (repoPath: string, branchName: string) => {
		const canSpawn = await deps.pty.canSpawn();
		if (!canSpawn) {
			deps.setStatusInfo("Max sessions reached (50)");
			return;
		}

		// Ensure this repo+branch is active without going through handleBranchSelect
		// (which has auto-spawn logic that would create a duplicate terminal).
		// Batch all store writes to flush the reactive graph once instead of 6+ times.
		const activeRepo = repositoriesStore.getActive();
		const needsSwitch = activeRepo?.path !== repoPath || activeRepo?.activeBranch !== branchName;

		const branch = repositoriesStore.get(repoPath)?.branches[branchName];
		const termCount = branch?.terminals.length || 0;

		const label = repoSettingsStore.getEffective(repoPath)?.branchLabels?.[branchName];
		const tabName = label ?? `${branchName.split(/[\\/]/).pop()} ${termCount + 1}`;
		const id = terminalsStore.add({
			sessionId: null,
			fontSize: deps.getDefaultFontSize(),
			name: tabName,
			cwd: branch?.worktreePath || null,
			awaitingInput: null,
			tuicSession: crypto.randomUUID(),
		});
		if (label) terminalsStore.update(id, { nameIsCustom: true });

		batch(() => {
			if (needsSwitch) {
				repositoriesStore.setActive(repoPath);
				repositoriesStore.setActiveBranch(repoPath, branchName);
				setCurrentRepoPath(repoPath);
				setCurrentBranch(branchName);
			}
			repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
			terminalsStore.setActive(id);
			if (!needsSwitch) {
				assignTabToActiveGroup(id, "terminal");
			}
		});
		// Focus the new terminal after SolidJS renders and mounts the component
		// (onMount sets ref, which happens in the next frame).
		requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
		return id;
	};

	const handleBranchSelect = (repoPath: string, branchName: string): Promise<void> => {
		// Append to the FIFO queue: this select runs only after every previously
		// queued select has settled. Each caller awaits the returned promise and sees
		// its own result/rejection; the queue tail swallows rejections so one failed
		// select doesn't break serialization for the calls behind it.
		const run = branchSelectQueue.then(() => handleBranchSelectInner(repoPath, branchName));
		branchSelectQueue = run.then(
			() => {},
			() => {},
		);
		return run;
	};

	const handleBranchSelectInner = async (repoPath: string, branchName: string) => {
		// Freeze-investigation: repo/branch switch is the reported foreground-freeze
		// trigger. Breadcrumb so a main-thread block during the switch cascade
		// attributes here (the freeze detector reports the freshest crumb).
		markPerf("branch.select", { repoPath, branchName });
		// Auto-deactivate global workspace before branch switch
		if (globalWorkspaceStore.isActive()) {
			const prevRepoPath = repositoriesStore.state.activeRepoPath;
			const prevBranch = prevRepoPath ? repositoriesStore.state.repositories[prevRepoPath]?.activeBranch : null;
			const key = prevRepoPath && prevBranch ? paneLayoutKey(prevRepoPath, prevBranch) : undefined;
			globalWorkspaceStore.deactivate(key);
		}

		repositoriesStore.setBranchSwitching(true);
		try {
			// Log the state we're LEAVING — critical for diagnosing terminal disappearance
			const prevRepo = repositoriesStore.getActive();
			const prevBranchName = prevRepo?.activeBranch;
			const prevBranch = prevBranchName ? prevRepo?.branches[prevBranchName] : null;
			appLogger.debug(
				"terminal",
				`BranchSelect ${prevBranchName ?? "(none)"} → ${branchName} terms=${(prevBranch?.terminals ?? []).length}→?`,
			);

			// Save state for the branch we're leaving
			if (prevRepo?.activeBranch) {
				const currentActiveId = terminalsStore.state.activeId;
				if (currentActiveId && prevBranch?.terminals.includes(currentActiveId)) {
					repositoriesStore.setBranch(prevRepo.path, prevRepo.activeBranch, { lastActiveTerminal: currentActiveId });
				}
				// Save pane layout for the branch we're leaving
				if (paneLayoutStore.isSplit()) {
					const key = paneLayoutKey(prevRepo.path, prevRepo.activeBranch);
					savedPaneLayouts.set(key, paneLayoutStore.serialize());
				} else {
					// Clear any stale layout if user unsplit while on this branch
					savedPaneLayouts.delete(paneLayoutKey(prevRepo.path, prevRepo.activeBranch));
				}
			}

			// Batch all reactive updates so downstream effects (file browser, etc.)
			// see a consistent snapshot — prevents stale intermediate states where
			// repoPath updated but fsRoot still points to the old worktree.
			batch(() => {
				setCurrentRepoPath(repoPath);
				repositoriesStore.setActive(repoPath);
				repositoriesStore.setActiveBranch(repoPath, branchName);
				setCurrentBranch(branchName);
			});

			// Fire-and-forget: diff stats are cosmetic, don't block branch switch
			const selectedBranch = repositoriesStore.get(repoPath)?.branches[branchName];
			if (selectedBranch?.worktreePath) {
				const wtPath = selectedBranch.worktreePath;
				deps.repo
					.getDiffStats(wtPath)
					.then((stats) => {
						repositoriesStore.updateBranchStats(repoPath, branchName, stats.additions, stats.deletions);
					})
					.catch((err) => appLogger.debug("git", `getDiffStats failed for ${branchName}`, err));
			}
			let branch = repositoriesStore.get(repoPath)?.branches[branchName];

			// Adopt orphaned terminals whose cwd matches this branch's worktree path.
			// Pre-compute claimed set O(B×T) once, then check in O(1) per terminal.
			if (branch?.worktreePath) {
				const branchTermSet = new Set(branch.terminals);
				const claimedIds = new Set<string>();
				for (const b of Object.values(repositoriesStore.get(repoPath)?.branches ?? {})) {
					if (b.name !== branchName) {
						for (const tid of b.terminals) claimedIds.add(tid);
					}
				}
				for (const id of terminalsStore.getIds()) {
					if (branchTermSet.has(id)) continue;
					if (claimedIds.has(id)) continue;
					const term = terminalsStore.get(id);
					if (term?.cwd === branch.worktreePath) {
						repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
					}
				}
				// Re-read branch state after potential adoptions
				branch = repositoriesStore.get(repoPath)?.branches[branchName];
			}
			const validTerminals = filterValidTerminals(branch?.terminals, terminalsStore.getIds()).filter(
				(id) => !terminalsStore.isDetached(id),
			);
			appLogger.debug(
				"terminal",
				`BranchSelect → ${branchName} valid=${validTerminals.length} saved=${branch?.savedTerminals?.length ?? 0}`,
			);
			if (validTerminals.length === 0 && (branch?.terminals?.length ?? 0) > 0) {
				appLogger.warn(
					"terminal",
					`BranchSelect MISMATCH: branch has terminals ${JSON.stringify(branch?.terminals)} but none found in store ${JSON.stringify(terminalsStore.getIds())}. Will create fresh terminal.`,
				);
			}

			if (validTerminals.length > 0) {
				// Restore saved pane layout if available and all its terminals are still valid
				const layoutKey = paneLayoutKey(repoPath, branchName);
				const savedLayout = savedPaneLayouts.get(layoutKey);
				if (savedLayout) {
					const validSet = new Set(validTerminals);
					const layoutTerminals = Object.values(savedLayout.groups).flatMap((g) =>
						g.tabs.filter((t) => t.type === "terminal").map((t) => t.id),
					);
					const allValid = layoutTerminals.length > 0 && layoutTerminals.every((id) => validSet.has(id));
					if (allValid) {
						paneLayoutStore.restore(savedLayout);
					} else {
						savedPaneLayouts.delete(layoutKey);
						paneLayoutStore.reset();
					}
				} else if (paneLayoutStore.consumeRestoredFromDisk()) {
					// Layout was loaded from disk at startup — keep it if terminal IDs are still valid
					const currentLayout = paneLayoutStore.serialize();
					const validSet = new Set(validTerminals);
					const layoutTerminals = Object.values(currentLayout.groups).flatMap((g) =>
						g.tabs.filter((t) => t.type === "terminal").map((t) => t.id),
					);
					if (!(layoutTerminals.length > 0 && layoutTerminals.every((id) => validSet.has(id)))) {
						paneLayoutStore.reset();
					}
				} else {
					paneLayoutStore.reset();
				}
				// Prefer a terminal that is awaiting input (question/error), then lastActive, then first
				const awaitingId = validTerminals.find((id) => terminalsStore.get(id)?.awaitingInput);
				if (awaitingId) {
					terminalsStore.setActive(awaitingId);
				} else {
					const remembered = branch?.lastActiveTerminal;
					if (remembered && validTerminals.includes(remembered)) {
						terminalsStore.setActive(remembered);
					} else {
						terminalsStore.setActive(validTerminals[0]);
					}
				}
			} else if (branch?.savedTerminals && branch.savedTerminals.length > 0) {
				// Only restore agent tabs with resumable sessions — plain shell tabs
				// have nothing meaningful to resume and would just be empty shells.
				const restorableTerminals = branch.savedTerminals.filter((t) => t.agentType != null);
				// Clear savedTerminals (consume-once) regardless of filter result
				repositoriesStore.setBranch(repoPath, branchName, { savedTerminals: [] });

				if (restorableTerminals.length > 0) {
					// Capture old terminal IDs from the pane layout (branch.terminals is cleared on hydration)
					const oldTerminalIds = paneLayoutStore.getTerminalTabIds();
					// Lazy restore: create terminals from persisted session state
					// First pass: create all terminals synchronously (instant UI)
					const restoredIds: { id: string; terminal: (typeof restorableTerminals)[number] }[] = [];
					for (const terminal of restorableTerminals) {
						const id = terminalsStore.add({
							sessionId: null,
							fontSize: terminal.fontSize,
							name: terminal.name,
							cwd: terminal.cwd,
							awaitingInput: null,
							tuicSession: terminal.tuicSession ?? crypto.randomUUID(),
							agentType: terminal.agentType ?? null,
							agentSessionId: terminal.agentSessionId ?? null,
							agentLaunchCommand: terminal.agentLaunchCommand ?? null,
						});
						repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
						restoredIds.push({ id, terminal });
					}
					if (restoredIds.length > 0) terminalsStore.setActive(restoredIds[0].id);

					// Remap disk-restored layout terminal IDs to newly created IDs
					const hasDiskLayout = paneLayoutStore.consumeRestoredFromDisk();
					if (hasDiskLayout && oldTerminalIds.length > 0) {
						const idMap = new Map<string, string>();
						for (let i = 0; i < Math.min(oldTerminalIds.length, restoredIds.length); i++) {
							idMap.set(oldTerminalIds[i], restoredIds[i].id);
						}
						paneLayoutStore.remapTerminalIds(idMap);
						appLogger.debug("terminal", `BranchSelect REMAP disk-restored paneLayout for ${branchName}`, {
							remapped: idMap.size,
						});
					} else {
						paneLayoutStore.reset();
					}

					// Second pass: verify resume commands in parallel (non-blocking)
					Promise.all(
						restoredIds.map(async ({ id, terminal }) => {
							const resumeCmd = await verifyAndBuildResumeCommand(
								terminal.agentType!,
								terminal.cwd,
								terminal.tuicSession,
								terminal.agentSessionId,
								terminal.agentLaunchCommand,
							);
							if (resumeCmd) {
								terminalsStore.update(id, {
									pendingResumeCommand: resumeCmd,
									agentSessionId: terminal.agentSessionId ?? null,
								});
							}
						}),
					).catch((e) => appLogger.warn("terminal", "Resume command verification failed", { error: String(e) }));
				} else {
					// All saved tabs were plain shells — spawn a fresh terminal
					paneLayoutStore.reset();
					await handleAddTerminalToBranch(repoPath, branchName);
				}
			} else if (!branch?.hadTerminals) {
				// First time selecting this branch — auto-spawn a terminal
				paneLayoutStore.reset();
				await handleAddTerminalToBranch(repoPath, branchName);
			} else {
				// hadTerminals && no valid terminals → user closed them all, show empty state.
				// Clear layout and activeId so the previous branch's split doesn't bleed through.
				paneLayoutStore.reset();
				terminalsStore.setActive(null);
			}

			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					terminalsStore.getActive()?.ref?.focus();
				});
			});
		} finally {
			// Story 1281-a37d: always clear the flag, even on throw. Without this,
			// a rejected close_pty / getDiffStats / resume-verification left the
			// TabBar filtering on the previous repo until app restart.
			repositoriesStore.setBranchSwitching(false);
		}
	};

	const handleRemoveRepo = async (repoPath: string) => {
		const repoState = repositoriesStore.get(repoPath);
		if (!repoState) return;

		const confirmed = await deps.dialogs.confirmRemoveRepo(repoState.displayName);
		if (!confirmed) return;

		for (const branch of Object.values(repoState.branches)) {
			for (const termId of branch.terminals) {
				await deps.closeTerminal(termId, true);
			}
		}

		invoke("stop_repo_watcher", { repoPath }).catch((err) =>
			appLogger.warn("app", `RepoWatcher failed to stop for ${repoPath}`, err),
		);

		repositoriesStore.remove(repoPath);
		repoSettingsStore.remove(repoPath);

		if (currentRepoPath() === repoPath) {
			setCurrentRepoPath(undefined);
			setCurrentBranch(null);
		}

		deps.setStatusInfo(`Removed ${repoState.displayName}`);

		if (terminalsStore.getCount() === 0) {
			await deps.createNewTerminal();
		}
	};

	const handleRemoveBranch = async (repoPath: string, branchName: string) => {
		const removeKey = `${repoPath}::${branchName}`;
		// Lock IMMEDIATELY (synchronously) to prevent concurrent invocations that race the awaits below
		if (removingBranches().has(removeKey)) return;
		setRemovingBranches((prev) => new Set([...prev, removeKey]));

		const clearLock = () => {
			setRemovingBranches((prev) => {
				const next = new Set(prev);
				next.delete(removeKey);
				return next;
			});
		};

		const repoState = repositoriesStore.get(repoPath);
		const branch = repoState?.branches[branchName];
		if (!branch?.worktreePath) {
			deps.setStatusInfo(`Cannot remove ${branchName}: not a worktree`);
			clearLock();
			return;
		}

		const confirmed = await deps.dialogs.confirmRemoveWorktree(branchName);
		if (!confirmed) {
			clearLock();
			return;
		}

		// Show "Removing…" in sidebar as soon as the user confirms — before
		// the terminal-close loop, which can take noticeable time. Otherwise
		// the lock is held while the UI still appears clickable.
		repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });

		// Close terminals defensively: a thrown error here used to leak the
		// removingBranches lock (clearLock was unreachable) and left isRemoving
		// stuck. Catch per-terminal so one bad PTY doesn't block cleanup.
		for (const termId of branch.terminals) {
			try {
				await deps.closeTerminal(termId, true);
			} catch (err) {
				appLogger.warn("git", `handleRemoveBranch: closeTerminal failed`, {
					termId,
					branchName,
					error: err instanceof Error ? err.message : String(err),
				});
			}
		}

		const effective = repoSettingsStore.getEffective(repoPath);
		const deleteBranch = effective?.deleteBranchOnRemove ?? true;
		appLogger.info("git", `handleRemoveBranch: invoking remove_worktree`, {
			repoPath,
			branchName,
			worktreePath: branch.worktreePath,
			deleteBranch,
		});

		// Tracks whether to remove the branch from the store at the end.
		// Set to true on success or non-fatal non-lock errors (old "remove from UI" behavior).
		// Stays false when: locked+cancelled, or force-remove failed (worktree still in git).
		let shouldRemoveFromStore = false;
		try {
			await deps.repo.removeWorktree(repoPath, branchName, deleteBranch);
			appLogger.info("git", `handleRemoveBranch: remove_worktree SUCCESS`, { branchName });
			shouldRemoveFromStore = true;
			deps.setStatusInfo(`Removed ${branchName}`);
		} catch (err) {
			const reason = err instanceof Error ? err.message : String(err);
			if (reason.startsWith("worktree_locked:")) {
				// Worktree is locked by a Claude agent — ask user to confirm force removal
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				appLogger.warn("git", `handleRemoveBranch: worktree locked — showing confirmation dialog`, {
					branchName,
					reason,
				});
				// Pass deleteBranch so the dialog can warn about unmerged-commit loss
				// when force=true causes `git branch -D` to run on a branch with
				// unpushed work. Catch dialog rejection so the removingBranches
				// lock is released even when the modal subsystem errors out.
				let forceConfirmed = false;
				try {
					forceConfirmed = await (deps.dialogs.confirmRemoveLockedWorktree?.(branchName, deleteBranch) ?? false);
				} catch (dialogErr) {
					appLogger.error("git", `handleRemoveBranch: confirmRemoveLockedWorktree threw`, {
						branchName,
						error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
					});
					deps.setStatusInfo(`Failed to confirm force-remove for ${branchName}`);
					clearLock();
					return;
				}
				if (!forceConfirmed) {
					appLogger.info("git", `handleRemoveBranch: user cancelled force removal of locked worktree`, { branchName });
					clearLock();
					return;
				}
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });
				try {
					await deps.repo.removeWorktree(repoPath, branchName, deleteBranch, true);
					appLogger.info("git", `handleRemoveBranch: force remove_worktree SUCCESS`, { branchName });
					shouldRemoveFromStore = true;
					deps.setStatusInfo(`Removed ${branchName}`);
				} catch (forceErr) {
					const forceReason = forceErr instanceof Error ? forceErr.message : String(forceErr);
					appLogger.error("git", `handleRemoveBranch: force remove_worktree FAILED`, {
						branchName,
						reason: forceReason,
					});
					deps.setStatusInfo(`Failed to remove ${branchName}: ${forceReason}`);
					repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
					clearLock();
					return;
				}
			} else if (reason.startsWith("worktree_is_main:")) {
				appLogger.warn("git", `handleRemoveBranch: branch is in main worktree — cannot remove as worktree`, {
					branchName,
				});
				deps.setStatusInfo(`Cannot remove ${branchName}: branch is in the main worktree, not a linked worktree`);
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				clearLock();
				return;
			} else {
				appLogger.error("git", `handleRemoveBranch: remove_worktree FAILED — branch will be removed from UI only`, {
					branchName,
					reason,
				});
				shouldRemoveFromStore = true;
				deps.setStatusInfo(`Removed ${branchName} from UI (worktree removal failed)`);
			}
		}

		if (!shouldRemoveFromStore) {
			repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
			clearLock();
			return;
		}
		appLogger.info("git", `handleRemoveBranch: calling removeBranch on store`, { branchName });
		clearLock();
		repositoriesStore.removeBranch(repoPath, branchName);
		repoSettingsStore.setLabel(repoPath, branchName, null);
	};

	const handleOpenRenameBranchDialog = (repoPath: string, branchName: string) => {
		setBranchToRename({ repoPath, branchName });
	};

	const handleOpenCreateBranchDialog = (repoPath: string, startPoint?: string | null) => {
		setBranchToCreate({ repoPath, startPoint: startPoint ?? null });
	};

	const handleCreateBranch = async (name: string, checkout: boolean) => {
		const target = branchToCreate();
		if (!target) return;

		// Throw on failure so the dialog can surface the error inline.
		await deps.repo.createBranch(target.repoPath, name, target.startPoint, checkout);

		repositoriesStore.setBranch(target.repoPath, name, {});
		if (checkout) setCurrentBranch(name);
		deps.setStatusInfo(`Created branch ${name}${checkout ? " (checked out)" : ""}`);
		void refreshAllBranchStats();
	};

	const handleRenameBranch = async (oldName: string, newName: string) => {
		const branch = branchToRename();
		if (!branch) return;

		try {
			await deps.repo.renameBranch(branch.repoPath, oldName, newName);
		} catch (err) {
			appLogger.error("git", "Failed to rename branch", err);
			deps.setStatusInfo(`Failed to rename branch: ${err}`);
			return;
		}

		repositoriesStore.renameBranch(branch.repoPath, oldName, newName);

		if (currentBranch() === oldName) {
			setCurrentBranch(newName);
		}

		deps.setStatusInfo(`Renamed branch ${oldName} to ${newName}`);
	};

	const activeWorktreePath = () => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeBranch) return undefined;
		return activeRepo.branches[activeRepo.activeBranch]?.worktreePath || activeRepo.path;
	};

	const activeRunCommand = () => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeBranch) return undefined;
		return activeRepo.branches[activeRepo.activeBranch]?.runCommand;
	};

	const handleAddRepo = async () => {
		let path: string | null = null;

		if (isTauri()) {
			const selected = await open({
				directory: true,
				multiple: false,
				title: "Select Repository Folder",
				defaultPath: repositoriesStore.getActive()?.path ?? "/",
			});
			if (!selected) return;
			path = typeof selected === "string" ? selected : selected[0];
		} else {
			// Browser mode: no native file picker — use in-app text input dialog
			const input = await deps.dialogs.promptRepoPath?.();
			if (!input?.trim()) return;
			path = input.trim();
		}

		if (!path) return;

		try {
			const info = await deps.repo.getInfo(path);

			// Close orphan terminals (not associated with any branch)
			const branchTerminalMap: Record<string, string[]> = {};
			for (const repoPath of repositoriesStore.getPaths()) {
				const repoState = repositoriesStore.get(repoPath);
				if (repoState) {
					for (const branch of Object.values(repoState.branches)) {
						branchTerminalMap[`${repoPath}:${branch.name}`] = branch.terminals;
					}
				}
			}
			const orphanTerminals = findOrphanTerminals(terminalsStore.getIds(), branchTerminalMap);

			for (const id of orphanTerminals) {
				await deps.closeTerminal(id, true);
			}

			repositoriesStore.add({
				path: info.path,
				displayName: info.name,
				initials: info.initials,
				isGitRepo: info.is_git_repo,
			});

			if (info.branch) {
				repositoriesStore.setBranch(info.path, info.branch, { worktreePath: info.path });
				repositoriesStore.setActiveBranch(info.path, info.branch);
				await handleAddTerminalToBranch(info.path, info.branch);
			} else if (!info.is_git_repo) {
				// Non-git directory: create a shell entry so the user can open terminals
				const shellBranch = "shell";
				repositoriesStore.setBranch(info.path, shellBranch, {
					worktreePath: info.path,
					isMain: true,
					isShell: true,
				});
				repositoriesStore.setActiveBranch(info.path, shellBranch);
				await handleAddTerminalToBranch(info.path, shellBranch);
			}

			repositoriesStore.setActive(info.path);
			setCurrentRepoPath(info.path);
			setCurrentBranch(info.branch || (!info.is_git_repo ? "shell" : ""));
			setRepoStatus(info.status === "not-git" ? "unknown" : info.status);

			// Start unified repo watcher (covers HEAD, git state, working tree).
			// Also started for non-git directories so a later `git init` is detected
			// (the .git-creation event triggers the non-git→git transition probe).
			invoke("start_repo_watcher", { repoPath: info.path }).catch((err) =>
				appLogger.warn("app", `RepoWatcher failed to start for ${info.path}`, err),
			);

			await refreshAllBranchStats();
		} catch (err) {
			appLogger.error("git", "Failed to add repository", err);
			deps.setStatusInfo(`Failed to add repo: ${err}`);
		}
	};

	const handleAddRemoteRepo = async (connectionId: string) => {
		// Prompt for remote path
		const input = await deps.dialogs.promptRepoPath?.();
		if (!input?.trim()) return;
		const remotePath = input.trim();

		try {
			// Validate by fetching repo info from remote
			const info = await rpc<RepoInfo>("get_repo_info", { path: remotePath }, connectionId);

			repositoriesStore.add({
				path: info.path,
				displayName: info.name,
				initials: info.initials,
				isGitRepo: info.is_git_repo,
				connectionId,
			});

			if (info.branch) {
				repositoriesStore.setBranch(info.path, info.branch, { worktreePath: info.path });
				repositoriesStore.setActiveBranch(info.path, info.branch);
				await handleAddTerminalToBranch(info.path, info.branch);
			} else if (!info.is_git_repo) {
				const shellBranch = "shell";
				repositoriesStore.setBranch(info.path, shellBranch, {
					worktreePath: info.path,
					isMain: true,
					isShell: true,
				});
				repositoriesStore.setActiveBranch(info.path, shellBranch);
				await handleAddTerminalToBranch(info.path, shellBranch);
			}

			repositoriesStore.setActive(info.path);
		} catch (err) {
			appLogger.error("git", `Failed to add remote repo from ${connectionId}`, err);
			deps.setStatusInfo(`Failed to add remote repo: ${err}`);
		}
	};

	const handleAddWorktree = async (repoPath: string) => {
		// Prevent concurrent creations for the same repo
		if (creatingWorktreeRepos().has(repoPath)) return;

		const repoState = repositoriesStore.get(repoPath);
		const worktreeBranches = repoState ? Object.keys(repoState.branches) : [];

		// Fetch data for the dialog in parallel
		const [suggestedName, localBranches, worktreesDir, baseRefs] = await Promise.all([
			deps.repo.generateWorktreeName(worktreeBranches),
			deps.repo.listLocalBranches(repoPath),
			deps.pty.getWorktreesDir(repoPath),
			deps.repo.listBaseRefOptions(repoPath),
		]);

		const promptOnCreate = deps.getPromptOnCreate?.(repoPath) ?? true;

		if (!promptOnCreate) {
			// Skip dialog: create worktree instantly with auto-generated name
			setWorktreeDialogState({
				repoPath,
				suggestedName,
				existingBranches: localBranches,
				worktreeBranches,
				worktreesDir,
				baseRefs,
			});
			await confirmCreateWorktree({
				branchName: suggestedName,
				createBranch: true,
				baseRef: baseRefs[0]?.name ?? "HEAD",
			});
			return;
		}

		setWorktreeDialogState({
			repoPath,
			suggestedName,
			existingBranches: localBranches,
			worktreeBranches,
			worktreesDir,
			baseRefs,
		});
	};

	/** Shared post-creation setup: run scripts, open terminal, fetch stats */
	const setupNewWorktree = async (
		repoPath: string,
		result: { name: string; path: string; branch: string; base_repo: string },
		displayName: string,
	) => {
		markRecentlyCreated(repoPath, result.branch);
		repositoriesStore.setBranch(repoPath, result.branch, { worktreePath: result.path });
		repositoriesStore.setActiveBranch(repoPath, result.branch);

		const effective = repoSettingsStore.getEffective(repoPath);
		if (effective?.setupScript) {
			try {
				deps.setStatusInfo(`Running setup script in ${displayName}...`);
				const scriptResult = await deps.repo.runSetupScript(effective.setupScript, result.path);
				if (scriptResult.exit_code !== 0) {
					appLogger.warn("git", `Setup script failed (exit ${scriptResult.exit_code})`, scriptResult.stderr);
					deps.setStatusInfo(`Setup script failed (exit ${scriptResult.exit_code})`);
				}
			} catch (err) {
				appLogger.warn("git", "Setup script execution error", err);
				deps.setStatusInfo(`Setup script failed: ${err}`);
			}
		}

		const termId = await handleAddTerminalToBranch(repoPath, result.branch);

		if (termId && effective?.runScript) {
			terminalsStore.update(termId, { pendingInitCommand: effective.runScript });
		}

		try {
			const stats = await deps.repo.getDiffStats(result.path);
			repositoriesStore.updateBranchStats(repoPath, result.branch, stats.additions, stats.deletions);
		} catch (err) {
			appLogger.debug("git", `getDiffStats failed for ${result.branch}`, err);
		}

		deps.setStatusInfo(`Created worktree ${displayName}`);
	};

	const confirmCreateWorktree = async (options: WorktreeCreateOptions) => {
		const dialogState = worktreeDialogState();
		if (!dialogState) return;

		const { repoPath } = dialogState;

		if (creatingWorktreeRepos().has(repoPath)) return;
		setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

		let pendingHandoff = false;
		try {
			deps.setStatusInfo(`Creating worktree ${options.branchName}...`);
			const result = await deps.repo.createWorktree(
				repoPath,
				options.branchName,
				options.createBranch,
				options.baseRef,
			);

			setWorktreeDialogState(null);

			if (result.status === "pending") {
				// Stale directory being cleaned up in background — show placeholder
				// and defer setupNewWorktree until the recreate completes (drained
				// in refreshAllBranchStats when isPreparing clears).
				markRecentlyCreated(repoPath, result.branch);
				repositoriesStore.setBranch(repoPath, result.branch, {
					worktreePath: result.path,
					isPreparing: true,
				});
				repositoriesStore.setActiveBranch(repoPath, result.branch);
				deps.setStatusInfo(`Preparing worktree ${options.branchName}...`);
				pendingCreations.set(pendingKey(repoPath, result.branch), {
					repoPath,
					displayName: options.branchName,
					result,
				});
				// Keep the per-repo create lock held until the background recreate
				// completes (drainPendingCreation / handleWorktreeCreateFailed).
				pendingHandoff = true;
			} else {
				await setupNewWorktree(repoPath, result, options.branchName);
			}
		} catch (err) {
			appLogger.error("git", "Failed to create worktree", err);
			deps.setStatusInfo(`Failed to create worktree: ${err}`);
			// Re-throw so the dialog can show the error and stay open
			throw err;
		} finally {
			if (!pendingHandoff) {
				setCreatingWorktreeRepos((prev) => {
					const next = new Set(prev);
					next.delete(repoPath);
					return next;
				});
			}
		}
	};

	/** Quick-clone flow: right-click branch → instant worktree with hybrid name */
	const handleCreateWorktreeFromBranch = async (repoPath: string, branchName: string) => {
		if (creatingWorktreeRepos().has(repoPath)) return;
		setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

		let pendingHandoff = false;
		try {
			const repoState = repositoriesStore.get(repoPath);
			const existingBranches = repoState ? Object.keys(repoState.branches) : [];
			const cloneName = await deps.repo.generateCloneBranchName(branchName, existingBranches);

			deps.setStatusInfo(`Creating worktree ${cloneName}...`);
			const result = await deps.repo.createWorktree(repoPath, cloneName, true, branchName);

			if (result.status === "pending") {
				// Stale directory being cleaned up in background — show placeholder.
				// Mirrors confirmCreateWorktree: do NOT call setupNewWorktree because
				// the worktree files don't exist yet (setup script would race against
				// the background `rm -rf` + recreate). Setup runs after recreate
				// completes, via drainPendingCreation.
				markRecentlyCreated(repoPath, result.branch);
				repositoriesStore.setBranch(repoPath, result.branch, {
					worktreePath: result.path,
					isPreparing: true,
				});
				repositoriesStore.setActiveBranch(repoPath, result.branch);
				deps.setStatusInfo(`Preparing worktree ${cloneName}...`);
				pendingCreations.set(pendingKey(repoPath, result.branch), {
					repoPath,
					displayName: cloneName,
					result,
				});
				pendingHandoff = true;
			} else {
				await setupNewWorktree(repoPath, result, cloneName);
			}
		} catch (err) {
			appLogger.error("git", "Failed to create worktree from branch", err);
			deps.setStatusInfo(`Failed to create worktree: ${err}`);
		} finally {
			if (!pendingHandoff) {
				setCreatingWorktreeRepos((prev) => {
					const next = new Set(prev);
					next.delete(repoPath);
					return next;
				});
			}
		}
	};

	/** Merge a worktree branch into target, then archive/delete based on setting.
	 *  When the branch has an open PR, uses GitHub API with the configured merge strategy.
	 *  Falls back to local git merge if no PR is found or GitHub API fails. */
	/** Close all terminals whose cwd is inside a worktree path. */
	const closeTerminalsInWorktree = async (wtPath: string) => {
		for (const termId of terminalsStore.getIds()) {
			const terminal = terminalsStore.get(termId);
			if (terminal?.cwd && (terminal.cwd === wtPath || terminal.cwd.startsWith(wtPath + "/"))) {
				await deps.closeTerminal(termId, true);
			}
		}
	};

	/** Close all terminals belonging to a branch. */
	const closeTerminalsForBranch = async (repoPath: string, branchName: string) => {
		const repoState = repositoriesStore.get(repoPath);
		const branch = repoState?.branches[branchName];
		if (branch) {
			for (const termId of branch.terminals) {
				await deps.closeTerminal(termId, true);
			}
		}
	};

	const handleMergeAndArchive = async (
		repoPath: string,
		branchName: string,
		targetBranch: string,
		afterMerge: string,
	) => {
		try {
			deps.setStatusInfo(`Merging ${branchName} into ${targetBranch}...`);

			// Use GitHub API when a PR exists for this branch
			const pr = githubStore.getPrStatus(repoPath, branchName);
			if (pr && pr.state === "OPEN") {
				const preferred = repoSettingsStore.getEffective(repoPath)?.prMergeStrategy ?? "merge";
				const method = effectiveMergeMethod(pr, preferred);
				try {
					await deps.repo.mergePrViaGithub(repoPath, pr.number, method);
				} catch (githubErr) {
					if (isMergeMethodNotAllowed(githubErr)) {
						// Surface 405 to the caller — branch protection rules disallow this merge method
						throw githubErr;
					}
					// Other GitHub API failures — fall back to local git merge
					appLogger.warn("git", `GitHub API merge failed, falling back to local git merge: ${githubErr}`);
					await mergeLocalAndFinalize(repoPath, branchName, targetBranch, afterMerge);
					return;
				}
				// GitHub merge succeeded — close terminals and finalize the worktree locally
				await closeTerminalsForBranch(repoPath, branchName);
				if (afterMerge === "ask") {
					deps.setStatusInfo(`Merged ${branchName} via GitHub — choose what to do with the worktree`);
					let hasDirtyFiles = false;
					try {
						const status = await invoke<{ stdout: string }>("run_git_command", {
							path: repoPath,
							args: ["status", "--porcelain"],
						});
						hasDirtyFiles = status.stdout.trim().length > 0;
					} catch (err) {
						appLogger.warn("git", `Could not check dirty status for ${repoPath}, assuming clean`, err);
					}
					setMergePendingCtx({ repoPath, branchName, baseBranch: targetBranch, hasDirtyFiles });
					return;
				}
				const action = afterMerge as "archive" | "delete";
				await deps.repo.finalizeMergedWorktree(repoPath, branchName, action);
				deps.setStatusInfo(`Merged ${branchName} via GitHub (${action === "archive" ? "archived" : "deleted"})`);
				repositoriesStore.removeBranch(repoPath, branchName);
				await refreshAllBranchStats();
				return;
			}

			// No open PR — local git merge
			await mergeLocalAndFinalize(repoPath, branchName, targetBranch, afterMerge);
		} catch (err) {
			if (isMergeMethodNotAllowed(err)) {
				throw err;
			}
			appLogger.error("git", "Failed to merge and archive worktree", err);
			deps.setStatusInfo(`Failed to merge: ${err}`);
		}
	};

	/** Local git merge path: checkout target, merge, then archive/delete/ask. */
	const mergeLocalAndFinalize = async (
		repoPath: string,
		branchName: string,
		targetBranch: string,
		afterMerge: string,
	) => {
		const result = await deps.repo.mergeAndArchiveWorktree(repoPath, branchName, targetBranch, afterMerge);

		// Merge succeeded — close terminals now (not before, to avoid orphaning the branch on failure)
		await closeTerminalsForBranch(repoPath, branchName);

		if (result.action === "pending") {
			// "ask" mode — merge succeeded, user must choose what to do with the worktree
			deps.setStatusInfo(`Merged ${branchName} into ${targetBranch} — choose what to do with the worktree`);
			let hasDirtyFiles = false;
			try {
				const status = await invoke<{ stdout: string }>("run_git_command", {
					path: repoPath,
					args: ["status", "--porcelain"],
				});
				hasDirtyFiles = status.stdout.trim().length > 0;
			} catch (err) {
				appLogger.warn("git", `Could not check dirty status for ${repoPath}, assuming clean`, err);
			}
			setMergePendingCtx({ repoPath, branchName, baseBranch: targetBranch, hasDirtyFiles });
			// Branch stays in sidebar until the user decides via cleanup dialog
			return;
		}

		deps.setStatusInfo(`Merged ${branchName} into ${targetBranch} (${result.action})`);

		// Remove branch from sidebar
		repositoriesStore.removeBranch(repoPath, branchName);

		// Refresh to pick up updated branch stats
		await refreshAllBranchStats();
	};

	/** Dismiss the post-merge cleanup dialog. */
	const dismissMergePending = () => {
		setMergePendingCtx(null);
	};

	const handleNewTab = async () => {
		// Prefer the active terminal's branch registration and CWD as source of truth —
		// the store's activeBranch may be stale if HEAD changed externally and head-changed
		// hasn't been fully processed yet (race between refreshAllBranchStats and setActiveBranch).
		const activeTerminalId = terminalsStore.state.activeId;
		const activeTerminal = activeTerminalId ? terminalsStore.get(activeTerminalId) : null;
		const activeCwd = activeTerminal?.cwd ?? null;

		if (activeCwd) {
			for (const repoPath of repositoriesStore.getPaths()) {
				const repo = repositoriesStore.get(repoPath);
				if (!repo) continue;

				// When multiple branches share the same worktreePath (main checkout after HEAD move),
				// prefer the branch that owns the active terminal — it reflects the partially-processed
				// head-changed state more accurately than insertion-order iteration.
				if (activeTerminalId) {
					const ownerEntry = Object.entries(repo.branches).find(
						([, b]) => b.worktreePath === activeCwd && b.terminals.includes(activeTerminalId),
					);
					if (ownerEntry) {
						await handleAddTerminalToBranch(repoPath, ownerEntry[0]);
						return;
					}
				}

				// Linked worktree: unique worktreePath per branch, unambiguous match
				const match = Object.values(repo.branches).find((b) => b.worktreePath && b.worktreePath === activeCwd);
				if (match) {
					await handleAddTerminalToBranch(repoPath, match.name);
					return;
				}
			}
		}

		// Fall back to store's active branch (no active terminal or no CWD match)
		const activeRepo = repositoriesStore.getActive();
		if (activeRepo?.activeBranch) {
			await handleAddTerminalToBranch(activeRepo.path, activeRepo.activeBranch);
		} else {
			await deps.createNewTerminal();
		}
	};

	const handleRunCommand = (forceDialog: boolean, openDialog: () => void) => {
		const savedCmd = activeRunCommand();
		if (savedCmd && !forceDialog) {
			executeRunCommand(savedCmd);
		} else {
			openDialog();
		}
	};

	const executeRunCommand = async (command: string) => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeBranch) return;

		repositoriesStore.setRunCommand(activeRepo.path, activeRepo.activeBranch, command);

		const canSpawn = await deps.pty.canSpawn();
		if (!canSpawn) {
			deps.setStatusInfo("Max sessions reached (50)");
			return;
		}

		const branch = activeRepo.branches[activeRepo.activeBranch];
		const cwd = branch?.worktreePath || activeRepo.path;
		const maxNameLen = deps.getMaxTabNameLength();
		const tabName = command.length > maxNameLen ? command.slice(0, maxNameLen) + "..." : command;

		const id = terminalsStore.add({
			sessionId: null,
			fontSize: deps.getDefaultFontSize(),
			name: tabName,
			cwd,
			awaitingInput: null,
		});

		terminalsStore.setActive(id);
		repositoriesStore.addTerminalToBranch(activeRepo.path, activeRepo.activeBranch, id);

		let waitAttempts = 0;
		const waitForSession = setInterval(async () => {
			waitAttempts++;
			const terminal = terminalsStore.get(id);
			if (terminal?.sessionId) {
				clearInterval(waitForSession);
				try {
					await deps.pty.write(terminal.sessionId, command + "\n");
				} catch (err) {
					appLogger.error("terminal", "Failed to send run command", err);
				}
			} else if (waitAttempts >= 20) {
				clearInterval(waitForSession);
				appLogger.warn("terminal", "Timed out waiting for session on run command");
			}
		}, 500);
	};

	const generateWorktreeName = async (): Promise<string> => {
		const state = worktreeDialogState();
		const worktreeBranches = state?.worktreeBranches ?? [];
		return deps.repo.generateWorktreeName(worktreeBranches);
	};

	const handleRepoSettings = (
		repoPath: string,
		openSettingsPanel: (context: { kind: "repo"; repoPath: string }) => void,
	) => {
		setCurrentRepoPath(repoPath);
		openSettingsPanel({ kind: "repo", repoPath });
	};

	// --- Branch switching ---

	const [switchBranchLists, setSwitchBranchLists] = createSignal<Record<string, string[]>>({});
	const [currentBranches, setCurrentBranches] = createSignal<Record<string, string>>({});

	/** Refresh the local branch list and current branch for all git repos (for context menu). */
	const refreshBranchLists = async () => {
		const repos = repositoriesStore.getOrderedRepos().filter((r) => r.isGitRepo !== false);
		const results: Record<string, string[]> = {};
		const heads: Record<string, string> = {};
		await Promise.all(
			repos.map(async (r) => {
				try {
					const [branches, info] = await Promise.all([deps.repo.listLocalBranches(r.path), deps.repo.getInfo(r.path)]);
					results[r.path] = branches;
					heads[r.path] = info.branch;
				} catch (e) {
					appLogger.debug("git", "Skipping repo in branch listing", { path: r.path, error: String(e) });
				}
			}),
		);
		setSwitchBranchLists(results);
		setCurrentBranches(heads);
	};

	// Compose branch stats + branch list refresh into a single function
	const refreshAllBranchStatsAndLists = async () => {
		await refreshAllBranchStats();
		await refreshBranchLists();
	};

	/** After a branch switch on the main worktree, migrate all stale branch entries
	 *  (same worktreePath as repoPath) into the new branch and remove them. */
	const migrateMainWorktreeBranches = (repoPath: string, newBranch: string) => {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return;

		// Ensure the target branch entry exists
		repositoriesStore.setBranch(repoPath, newBranch, { worktreePath: repoPath });

		// Find all branches on the main worktree that aren't the new branch
		const stale = Object.values(repo.branches).filter((b) => b.worktreePath === repoPath && b.name !== newBranch);

		batch(() => {
			for (const branch of stale) {
				repositoriesStore.mergeBranchState(repoPath, branch.name, newBranch);
				repositoriesStore.removeBranch(repoPath, branch.name);
			}
			repositoriesStore.setActiveBranch(repoPath, newBranch);
		});
	};

	/** Handle branch switch request from sidebar. Checks terminal safety, then calls Rust. */
	const handleSwitchBranch = async (repoPath: string, branchName: string) => {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return;

		// Pre-flight: check for busy terminals on the main worktree
		const mainWorktreeBranches = Object.values(repo.branches).filter((b) => b.worktreePath === repoPath);
		for (const branch of mainWorktreeBranches) {
			for (const termId of branch.terminals) {
				const term = terminalsStore.get(termId);
				if (term?.shellState === "busy") {
					deps.setStatusInfo(`Cannot switch branch: terminal "${term.name || termId}" has a running process`);
					return;
				}
			}
		}

		// git stderr that means a stale/contended index.lock — these are recoverable
		// (the lock is auto-cleared once stale), so the error dialog offers a retry.
		const isLockError = (msg: string) => /index\.lock|could not write index|another git process/i.test(msg);

		// Stash + switch, then migrate branch entries and refresh. Throws on failure.
		const stashAndSwitch = async () => {
			const result = await deps.repo.switchBranch(repoPath, branchName, { stash: true });
			deps.setStatusInfo(`Switched to ${result.new_branch} (changes stashed)`);
			migrateMainWorktreeBranches(repoPath, result.new_branch);
			await refreshAllBranchStatsAndLists();
		};

		try {
			const result = await deps.repo.switchBranch(repoPath, branchName);
			if (result.stashed) {
				deps.setStatusInfo(`Switched to ${result.new_branch} (changes stashed)`);
			} else {
				deps.setStatusInfo(`Switched to ${result.new_branch}`);
			}
			// Migrate all main-worktree branches into the new branch entry, then remove stale ones
			migrateMainWorktreeBranches(repoPath, result.new_branch);
			// Refresh branch stats to pick up the new HEAD
			await refreshAllBranchStatsAndLists();
		} catch (err) {
			const errMsg = String(err);
			if (errMsg === "dirty" || errMsg.includes("dirty")) {
				// Dirty working tree — ask user to stash
				const confirmed = await deps.dialogs.confirmStashAndSwitch?.(branchName);
				if (!confirmed) return;
				try {
					await stashAndSwitch();
				} catch (stashErr) {
					const msg = String(stashErr);
					appLogger.error("git", "Stash & switch failed", { repoPath, branchName, error: msg });
					const lock = isLockError(msg);
					const detail = lock
						? `A stale git lock is blocking the index:\n\n${msg}\n\nStale locks clear automatically after a short while. Retry?`
						: msg;
					const retry = await deps.dialogs.reportGitError?.("Stash & switch failed", detail, lock);
					if (!retry) return;
					try {
						await stashAndSwitch();
					} catch (retryErr) {
						const retryMsg = String(retryErr);
						appLogger.error("git", "Stash & switch retry failed", { repoPath, branchName, error: retryMsg });
						await deps.dialogs.reportGitError?.("Stash & switch failed again", retryMsg, false);
					}
				}
			} else {
				appLogger.error("git", "Branch switch failed", { repoPath, branchName, error: errMsg });
				await deps.dialogs.reportGitError?.("Branch switch failed", errMsg, false);
			}
		}
	};

	const handleCheckoutRemoteBranch = async (repoPath: string, branchName: string) => {
		try {
			await deps.repo.checkoutRemoteBranch(repoPath, branchName);
			deps.setStatusInfo(`Checked out ${branchName}`);
			migrateMainWorktreeBranches(repoPath, branchName);
			await refreshAllBranchStatsAndLists();
			await handleBranchSelectInner(repoPath, branchName);
		} catch (err) {
			appLogger.error("git", "Failed to checkout remote branch", { error: String(err) });
			deps.setStatusInfo(`Checkout failed: ${err}`);
		}
	};

	// --- OSC 7 CWD tracking: reassign terminal to matching worktree branch ---

	const cwdDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

	/** Find the repo + branch whose worktreePath best matches the given cwd (longest prefix). */
	const findBranchForCwd = (cwd: string): { repoPath: string; branchName: string } | null => {
		let best: { repoPath: string; branchName: string } | null = null;
		let bestLen = 0;
		for (const repoPath of repositoriesStore.getPaths()) {
			const repo = repositoriesStore.get(repoPath);
			if (!repo) continue;
			for (const [branchName, branch] of Object.entries(repo.branches)) {
				// For main branches without a worktreePath, the repo path itself is the match
				const wt = branch.worktreePath ?? (branch.isMain ? repoPath : null);
				if (!wt) continue;
				if (pathStartsWith(cwd, wt) && wt.length > bestLen) {
					best = { repoPath, branchName };
					bestLen = wt.length;
				}
			}
		}
		return best;
	};

	/** Async body for debounced CWD change handling — extracted for proper error catching. */
	const performCwdReassignment = async (terminalId: string, newCwd: string) => {
		// Guard: terminal may have been closed during the debounce window
		if (!terminalsStore.get(terminalId)) return;

		const currentRepoPathForTerm = repositoriesStore.getRepoPathForTerminal(terminalId);
		// Find which branch the terminal currently belongs to
		let currentBranchName: string | null = null;
		if (currentRepoPathForTerm) {
			const repo = repositoriesStore.get(currentRepoPathForTerm);
			if (repo) {
				for (const [bName, branch] of Object.entries(repo.branches)) {
					if (branch.terminals.includes(terminalId)) {
						currentBranchName = bName;
						break;
					}
				}
			}
		}

		let target = findBranchForCwd(newCwd);

		// If no match and cwd is inside a known repo, the worktree may have just been created
		if (!target && currentRepoPathForTerm) {
			const isInsideKnownRepo = repositoriesStore.getPaths().some((rp) => pathStartsWith(newCwd, rp));
			if (isInsideKnownRepo) {
				await refreshAllBranchStats();
				target = findBranchForCwd(newCwd);
			}
		}

		if (!target) return; // Unknown path — no reassignment
		// Same branch? Nothing to do
		if (target.repoPath === currentRepoPathForTerm && target.branchName === currentBranchName) return;

		const { repoPath: targetRepoPath, branchName: targetBranchName } = target;

		appLogger.info("terminal", `[CwdChange] ${terminalId} → ${targetRepoPath}:${targetBranchName} (cwd=${newCwd})`);

		batch(() => {
			// Remove from old branch
			if (currentRepoPathForTerm && currentBranchName) {
				repositoriesStore.removeTerminalFromBranch(currentRepoPathForTerm, currentBranchName, terminalId);
			}
			// Add to new branch
			repositoriesStore.addTerminalToBranch(targetRepoPath, targetBranchName, terminalId);

			// If this is the active terminal, switch the active branch
			if (terminalsStore.state.activeId === terminalId) {
				repositoriesStore.setActiveBranch(targetRepoPath, targetBranchName);
				setCurrentBranch(targetBranchName);
				if (targetRepoPath !== currentRepoPathForTerm) {
					repositoriesStore.setActive(targetRepoPath);
					setCurrentRepoPath(targetRepoPath);
				}
			}
		});
	};

	/** Called from Terminal.tsx OSC 7 handler when a terminal's cwd changes. */
	const handleTerminalCwdChange = (terminalId: string, newCwd: string) => {
		// Debounce reassignment per terminal (300ms) — cwd store update is immediate in Terminal.tsx
		clearTimeout(cwdDebounceTimers.get(terminalId));
		cwdDebounceTimers.set(
			terminalId,
			setTimeout(() => {
				cwdDebounceTimers.delete(terminalId);
				void performCwdReassignment(terminalId, newCwd).catch((err) =>
					appLogger.warn("terminal", `[CwdChange] reassignment error for ${terminalId}`, err),
				);
			}, 300),
		);
	};

	/** Handle the `worktree-create-failed` event emitted by the Rust background
	 *  recreate task. Drops the pending creation (no setup), removes the
	 *  placeholder from the store, releases the per-repo create lock, and
	 *  surfaces the error to the user. */
	const handleWorktreeCreateFailed = (payload: { repoPath: string; branch: string; reason: string }) => {
		const { repoPath, branch, reason } = payload;
		appLogger.error("git", `Worktree creation failed`, payload);
		pendingCreations.delete(pendingKey(repoPath, branch));
		repositoriesStore.removeBranch(repoPath, branch);
		setCreatingWorktreeRepos((prev) => {
			const next = new Set(prev);
			next.delete(repoPath);
			return next;
		});
		deps.setStatusInfo(`Failed to create worktree ${branch}: ${reason}`);
	};

	/** Cancel any pending CWD debounce timer for a terminal (call on terminal close). */
	const cancelCwdTracking = (terminalId: string) => {
		const timer = cwdDebounceTimers.get(terminalId);
		if (timer !== undefined) {
			clearTimeout(timer);
			cwdDebounceTimers.delete(terminalId);
		}
	};

	/** Get available worktree targets for moving a terminal (excludes current branch). */
	const getWorktreeTargets = (terminalId: string): Array<{ branchName: string; path: string }> => {
		const repoPath = repositoriesStore.getRepoPathForTerminal(terminalId);
		if (!repoPath) return [];
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return [];

		// Find current branch for this terminal
		let currentBranchName: string | null = null;
		for (const [name, branch] of Object.entries(repo.branches)) {
			if (branch.terminals.includes(terminalId)) {
				currentBranchName = name;
				break;
			}
		}

		const targets: Array<{ branchName: string; path: string }> = [];
		for (const [name, branch] of Object.entries(repo.branches)) {
			if (name === currentBranchName) continue;
			const wtPath = branch.worktreePath ?? (branch.isMain ? repoPath : null);
			if (wtPath) {
				targets.push({ branchName: name, path: wtPath });
			}
		}
		return targets;
	};

	/** Move a terminal to a different worktree by sending cd to the PTY. */
	const moveTerminalToWorktree = async (terminalId: string, worktreePath: string): Promise<void> => {
		const terminal = terminalsStore.get(terminalId);
		if (!terminal?.sessionId) return;
		// Single-quote the path to handle spaces safely
		const escaped = "'" + worktreePath.replace(/'/g, "'\\''") + "'";
		await deps.pty.write(terminal.sessionId, `cd ${escaped}\n`);
		appLogger.info("terminal", `[MoveToWorktree] ${terminalId} → cd ${worktreePath}`);
	};

	return {
		currentRepoPath,
		setCurrentRepoPath,
		currentBranch,
		setCurrentBranch,
		repoStatus,
		setRepoStatus,
		branchToRename,
		setBranchToRename,
		refreshAllBranchStats: refreshAllBranchStatsAndLists,
		handleBranchSelect,
		handleAddTerminalToBranch,
		handleRemoveRepo,
		handleRemoveBranch,
		handleOpenRenameBranchDialog,
		handleRenameBranch,
		branchToCreate,
		setBranchToCreate,
		handleOpenCreateBranchDialog,
		handleCreateBranch,
		activeWorktreePath,
		activeRunCommand,
		handleAddRepo,
		handleAddRemoteRepo,
		handleAddWorktree,
		confirmCreateWorktree,
		handleCreateWorktreeFromBranch,
		handleMergeAndArchive,
		mergePendingCtx,
		dismissMergePending,
		closeTerminalsForBranch,
		worktreeDialogState,
		setWorktreeDialogState,
		creatingWorktreeRepos,
		removingBranches,
		handleWorktreeCreateFailed,
		handleNewTab,
		handleRunCommand,
		executeRunCommand,
		generateWorktreeName,
		handleRepoSettings,
		handleCheckoutRemoteBranch,
		handleSwitchBranch,
		handleTerminalCwdChange,
		cancelCwdTracking,
		switchBranchLists,
		currentBranches,
		refreshBranchLists,
		getWorktreeTargets,
		moveTerminalToWorktree,
		/** Create a new terminal for the branch and queue the review command */
		handleReviewPr: async (repoPath: string, branchName: string, command: string) => {
			const termId = await handleAddTerminalToBranch(repoPath, branchName);
			if (termId) {
				terminalsStore.update(termId, { pendingInitCommand: command });
			}
		},
	};
}
