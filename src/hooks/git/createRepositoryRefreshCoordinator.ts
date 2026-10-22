import { batch } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { type RepositoryState, repositoriesStore } from "../../stores/repositories";
import { closeWorktreeTerminals, removeWorktreeWorkspace } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import { timeBatch } from "../../utils/perfTrace";

/** The backend's removal verdict for one detached checkout. `live_sessions` lists the sessions
 *  still working inside it; a checkout with any is never `safe`. */
export interface OrphanAssessment {
	path: string;
	safe: boolean;
	reason?: string;
	/** The checkout's status + HEAD fingerprint; absent when it is gone or unreadable. */
	dirty_fingerprint?: string;
	live_sessions?: Array<{ session_id: string; name: string }>;
}

interface WorkspaceLifecycleResponse {
	dirty_files: number | null;
	commit_status: import("../../stores/workspaceIdentity").WorkspaceCommitStatus;
	removal_safety: import("../../stores/workspaceIdentity").WorkspaceRemovalSafety;
	error?: string;
}

export interface PendingCreation {
	repoPath: string;
	displayName: string;
	result: {
		name: string;
		path: string;
		workspace_id: string;
		branch: string;
		base_repo: string;
		kind?: "worktree";
	};
}

interface RepositoryRefreshCoordinatorDeps {
	repo: {
		getInfo: (path: string) => Promise<{ branch: string; is_git_repo: boolean }>;
		getRepoStructure: (repoPath: string) => Promise<{
			worktree_paths: Record<string, import("../useRepository").WorkspaceWorktree>;
			merged_branches: string[];
		}>;
		getRepoDiffStats: (repoPath: string) => Promise<{
			diff_stats: Record<string, { additions: number; deletions: number }>;
			last_commit_ts: Record<string, number | null>;
			workspace_statuses: Record<string, WorkspaceLifecycleResponse>;
		}>;
		detectOrphanWorktrees: (repoPath: string) => Promise<string[]>;
		assessOrphanCleanup: (repoPath: string) => Promise<OrphanAssessment[]>;
		beginOrphanCleanup: (repoPath: string, paths: string[]) => Promise<void>;
		pendingOrphanCleanupAnswer: (repoPath: string) => Promise<boolean | null>;
		clearOrphanCleanup: (repoPath: string, kept: boolean) => Promise<void>;
		removeOrphanWorktree: (
			repoPath: string,
			worktreePath: string,
			safeOnly?: boolean,
			confirmedSessions?: string[],
		) => Promise<void>;
		getWorkspaceLifecycle: (
			repoPath: string,
			workspaceId: string,
		) => Promise<import("../../stores/workspaceIdentity").WorkspaceLifecycleStatus>;
		finalizeMergedWorktree: (
			repoPath: string,
			workspaceId: string,
			action: "archive" | "delete",
		) => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
	};
	dialogs: {
		confirmOrphanCleanup?: (
			repoPath: string,
			assessments: Array<{ path: string; safe: boolean; reason?: string }>,
			countdownSeconds: number,
		) => Promise<boolean>;
		answerOrphanCleanup?: (repoPath: string, remove: boolean) => void;
	};
	closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
	closeTerminalsInWorktree: (worktreePath: string) => Promise<void>;
	setStatusInfo: (message: string) => void;
}

/** Owns two-phase repository refresh, stale-write guards, cleanup, and creation grace. */
export function createRepositoryRefreshCoordinator(deps: RepositoryRefreshCoordinatorDeps) {
	/** Transition a repo from git to shell mode (e.g. .git was removed) */
	const transitionToShell = (repoPath: string, currentRepo: RepositoryState) => {
		batch(() => {
			// Migrate all terminals to a shell branch. Removal is by KEY: a record's
			// `branchName` is not necessarily its key, and removing by the wrong one
			// leaves the row behind with its terminals already re-homed.
			const allTerminals: string[] = [];
			for (const [workspaceId, workspace] of Object.entries(currentRepo.workspaces)) {
				allTerminals.push(...workspace.terminals);
				repositoriesStore.removeWorkspace(repoPath, workspaceId);
			}
			repositoriesStore.setIsGitRepo(repoPath, false);
			const shellBranch = "shell";
			repositoriesStore.setWorkspace(repoPath, shellBranch, {
				worktreePath: repoPath,
				isMain: true,
				isShell: true,
			});
			for (const termId of allTerminals) {
				repositoriesStore.addTerminalToWorkspace(repoPath, shellBranch, termId);
			}
			repositoriesStore.setActiveWorkspace(repoPath, shellBranch);
		});
	};

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

	// When each workspace key was first seen by a refresh of its repo, keyed by
	// repoPath. The backend coalesces and caches worktree_paths (GIT_CACHE_TTL,
	// 60s), so a snapshot requested after a worktree was created can still have
	// been computed before it. A key that appears after the repo's first refresh
	// is not judged deleted until a snapshot requested CREATION_GRACE_WINDOW_MS
	// later. Keys present at the first refresh are old (-Infinity): persisted
	// rows from a previous session are pruned at once. (#1317)
	const workspaceFirstSeen = new Map<string, Map<string, number>>();
	const trackFirstSeen = (repoPath: string, keys: Set<string>, now: number): Map<string, number> => {
		const known = workspaceFirstSeen.get(repoPath);
		const next = new Map<string, number>();
		for (const key of keys) next.set(key, known ? (known.get(key) ?? now) : Number.NEGATIVE_INFINITY);
		workspaceFirstSeen.set(repoPath, next);
		return next;
	};

	// A probe that never settles (dead mount, stuck git status) would hold
	// refreshInFlight forever, so it is bounded; timeout and error both mean "not gone".
	const CHECKOUT_PROBE_TIMEOUT_MS = 5_000;
	const isCheckoutGone = async (worktreePath: string): Promise<boolean> => {
		let timer: ReturnType<typeof setTimeout> | undefined;
		try {
			// One settled result instead of a race whose losing timeout promise
			// stays pending after its timer has been cleared.
			return await new Promise<boolean>((resolve) => {
				timer = setTimeout(() => resolve(false), CHECKOUT_PROBE_TIMEOUT_MS);
				deps.repo
					.getInfo(worktreePath)
					.then((info) => resolve(!info.is_git_repo))
					.catch(() => resolve(false));
			});
		} catch {
			return false;
		} finally {
			clearTimeout(timer);
		}
	};

	const refreshRepoOnce = async (repoPath: string) => {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return;
		// Snapshot branch keys before any await so we can detect user-triggered
		// removals that happen while async ops are in-flight (race condition guard).
		const priorBranchKeys = new Set(Object.keys(repo.workspaces));
		const requestedAt = Date.now();
		const firstSeen = trackFirstSeen(repoPath, priorBranchKeys, requestedAt);
		// Non-git directories: check if they became a git repo
		if (repo.isGitRepo === false) {
			try {
				const info = await deps.repo.getInfo(repoPath);
				if (info.is_git_repo && info.branch) {
					// Directory gained .git — transition to git mode. Carry the shell
					// branch's terminals (and its last-active) into the new git branch:
					// otherwise `git init` removes the shell branch and creates an empty
					// git branch, orphaning the open terminals so they vanish from the
					// view until a later branch-select re-adopts them by cwd.
					batch(() => {
						const carriedTerminals: string[] = [];
						let carriedActive: string | null = null;
						for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
							carriedTerminals.push(...workspace.terminals);
							if (workspace.lastActiveTerminal) carriedActive = workspace.lastActiveTerminal;
							repositoriesStore.removeWorkspace(repoPath, workspaceId);
						}
						repositoriesStore.setIsGitRepo(repoPath, true);
						repositoriesStore.setWorkspace(repoPath, info.branch, {
							worktreePath: repoPath,
							lastActiveTerminal: carriedActive,
						});
						// addTerminalToWorkspace (not setWorkspace terminals) keeps the
						// terminalToRepo inverse index consistent.
						for (const tid of carriedTerminals) {
							repositoriesStore.addTerminalToWorkspace(repoPath, info.branch, tid);
						}
						repositoriesStore.setActiveWorkspace(repoPath, info.branch);
					});
					// Restart the repo watcher so it registers the now-present .git
					// sub-watches (HEAD/refs/worktrees). On macOS/Windows the recursive
					// root watch already covers .git, but Linux uses targeted watches
					// that were skipped while the directory was non-git.
					invoke("stop_repo_watcher", { repoPath })
						.then(() => invoke("start_repo_watcher", { repoPath }))
						.catch((e) =>
							appLogger.debug("git", "Watcher restart after git-init failed", { repoPath, error: String(e) }),
						);
				}
			} catch (e) {
				appLogger.debug("git", "Repo probe failed — staying in shell mode", { repoPath, error: String(e) });
			}
			return;
		}

		// === PHASE 1: Structure (fast) ===
		// Returns worktree_paths + merged_branches only — no expensive diff stats.
		const structure = await deps.repo.getRepoStructure(repoPath);

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
			if (Object.keys(currentRepo.workspaces).length > 0) return;
		}

		// Compute the target set of branches to keep, then apply all
		// mutations in a single batch to prevent intermediate renders
		// (which caused the sidebar to flash/jump during refresh).
		const storeIds = new Set(terminalsStore.getIds());
		const toRemove: string[] = [];
		const worktreesToClose: string[] = [];
		const probeCandidates: Array<{ branchName: string; worktreePath: string; terminals: string[] }> = [];

		const active = currentRepo.activeWorkspaceId;
		// A branch switch changes the workspace id, not the checkout directory.
		// Re-home sessions for every changed worktree, including inactive rows.
		const replacementByPath = new Map(Object.entries(worktreePaths).map(([id, wt]) => [wt.path, id]));
		const replacements = new Map<string, string>();

		for (const branchName of Object.keys(currentRepo.workspaces)) {
			if (!(branchName in worktreePaths)) {
				// Skip branches that a concurrent/recent refresh already handled.
				// The store removal may not have settled yet (batch scheduled), so
				// we'd otherwise re-enqueue the same close+remove.
				if (alreadyProcessed(repoPath, branchName)) continue;
				// The structure snapshot was requested before this workspace entered the
				// store, so its absence says nothing about the checkout: a worktree created
				// while the fetch was in flight (MCP worktree_create has no creation grace)
				// would otherwise be read as deleted and its terminals killed. The next
				// refresh sees it with a snapshot that postdates it.
				if (!priorBranchKeys.has(branchName)) {
					appLogger.info("git", `refreshAllBranchStats: SNAPSHOT PREDATES "${branchName}" — not judging it deleted`, {
						repoPath,
					});
					continue;
				}
				// Skip branches just created — git may not have fully registered the
				// worktree by the time the first repo-changed refresh fires.
				if (isRecentlyCreated(repoPath, branchName)) {
					appLogger.info("git", `refreshAllBranchStats: CREATION GRACE skipping "${branchName}" (just created)`, {
						repoPath,
					});
					continue;
				}
				const replacement = replacementByPath.get(currentRepo.workspaces[branchName]?.worktreePath ?? "");
				if (replacement) {
					appLogger.info(
						"terminal",
						`refreshAllBranchStats: workspace "${branchName}" replaced by "${replacement}" at the same path`,
					);
					replacements.set(branchName, replacement);
					toRemove.push(branchName);
					markProcessed(repoPath, branchName);
					continue;
				}
				// The snapshot was requested after the workspace entered the store, but
				// it may still be a cached/coalesced answer older than the checkout.
				if (requestedAt - (firstSeen.get(branchName) ?? requestedAt) < CREATION_GRACE_WINDOW_MS) {
					appLogger.info(
						"git",
						`refreshAllBranchStats: SNAPSHOT MAY PREDATE "${branchName}" — not judging it deleted`,
						{
							repoPath,
						},
					);
					continue;
				}
				// Branch has live terminals — only keep it if the worktree path
				// is the main repo checkout (HEAD switched away). If the worktree
				// directory was deleted externally, close the orphaned terminals
				// so the stale branch can be cleaned up.
				const branchState = currentRepo.workspaces[branchName];
				const hasLiveTerminals = branchState?.terminals.some((id) => storeIds.has(id));
				if (hasLiveTerminals) {
					const linkedPath = branchState.worktreePath !== repoPath ? branchState.worktreePath : null;
					if (linkedPath) {
						// A snapshot that omits the worktree is not proof it is gone: the backend
						// serves coalesced/cached worktree_paths and concurrent runs can judge it
						// with different snapshots. Closing a session is irreversible, so the
						// directory is probed after the loop (concurrently). (#1317)
						probeCandidates.push({
							branchName,
							worktreePath: linkedPath,
							terminals: branchState.terminals,
						});
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

		const gone = await Promise.all(probeCandidates.map((c) => isCheckoutGone(c.worktreePath)));
		probeCandidates.forEach(({ branchName, worktreePath, terminals }, i) => {
			if (!gone[i]) {
				// Keep the row too: its terminals are filed under it.
				appLogger.info(
					"terminal",
					`refreshAllBranchStats: keeping "${branchName}" — snapshot omits it but its checkout is still on disk`,
					{ worktreePath },
				);
				return;
			}
			// Linked worktree was removed externally — close its terminals
			appLogger.info("terminal", `refreshAllBranchStats: closing terminals for deleted worktree "${branchName}"`, {
				terminals,
				worktreePath,
			});
			worktreesToClose.push(worktreePath);
			toRemove.push(branchName);
			markProcessed(repoPath, branchName);
		});

		if (toRemove.length > 0) {
			appLogger.info("terminal", `refreshAllBranchStats removing branches from ${repoPath}`, {
				toRemove,
				worktreePathKeys: Object.keys(worktreePaths),
				existingBranches: Object.keys(currentRepo.workspaces),
			});
		}

		// Close terminals for deleted worktrees before mutating store state.
		// Best-effort: a PTY may already be dead; log and continue so the
		// branch removal in the batch below is not blocked.
		await Promise.allSettled(
			worktreesToClose.map(async (worktreePath) => {
				try {
					await closeWorktreeTerminals(worktreePath, deps.closeTerminal);
				} catch (err) {
					appLogger.warn("terminal", `refreshAllBranchStats: failed to close terminals in ${worktreePath}`, err);
				}
			}),
		);

		// Freeze-investigation: split the structural batch into body (our
		// setState loop) vs reactive flush (dependent effects/memos waking).
		timeBatch(
			`git.refreshBatch:${repoPath}`,
			(markBodyEnd) =>
				batch(() => {
					// Guard against race: if a branch was present before our async ops
					// but is now gone from the live store, the user deleted it while we
					// were in-flight. Don't resurrect it via stale worktreePaths data.
					const liveRepo = repositoriesStore.get(repoPath);
					// Create new worktree branches first so mergeWorkspaceState has a target
					for (const [workspaceId, wt] of Object.entries(worktreePaths)) {
						if (priorBranchKeys.has(workspaceId) && !liveRepo?.workspaces[workspaceId]) {
							appLogger.info("git", `refreshAllBranchStats: RACE GUARD blocked resurrection of "${workspaceId}"`, {
								repoPath,
								worktreePath: wt.path,
							});
							continue;
						}
						// `mergedSet` holds BRANCH names, so it is queried with the
						// record's branch — never the key, which is a workspace id and
						// only equals the branch under the identity migration.
						const update: Partial<import("../../stores/repositories").WorkspaceState> = {
							worktreePath: wt.path,
							branchName: wt.branch,
							kind: wt.path === repoPath ? "main" : wt.kind,
							isMerged: mergedSet.has(wt.branch),
						};
						repositoriesStore.setWorkspace(repoPath, workspaceId, update);
					}
					for (const [source, target] of replacements) {
						repositoriesStore.mergeWorkspaceState(repoPath, source, target);
						if (source === active) repositoriesStore.setActiveWorkspace(repoPath, target);
					}
					for (const branchName of toRemove) {
						removeWorktreeWorkspace(repoPath, branchName);
					}
					markBodyEnd();
				}),
			// DEFERRED (2026-10-16) — real-stall replay needs captured consumer state;
			// counts and byte size alone cannot reconstruct the reactive graph.
			() => ({
				repo_count: repositoriesStore.getActivePaths().length,
				worktree_count: Object.keys(worktreePaths).length,
				store_worktree_count: Object.keys(currentRepo.workspaces).length,
				// Diagnostic-only: serialize only after a slow batch, never per normal refresh.
				response_bytes: new TextEncoder().encode(JSON.stringify(structure)).byteLength,
			}),
		);

		const updatedRepo = repositoriesStore.get(repoPath);
		if (!updatedRepo) return;

		// Side effects that only need structure data — run before Phase 2
		await handleAutoArchiveMerged(repoPath, updatedRepo.workspaces);
		await handleOrphanCleanup(repoPath);

		// === PHASE 2: Stats (slow) ===
		// Per-worktree diff stats + last-commit timestamps.
		// Non-fatal: if this fails, UI shows rows from Phase 1 with stale/zero stats.
		try {
			const stats = await deps.repo.getRepoDiffStats(repoPath);

			const currentRepoForStats = repositoriesStore.get(repoPath);
			if (!currentRepoForStats) return;

			// Freeze-investigation: same body-vs-flush split for the stats batch.
			timeBatch(`git.statsBatch:${repoPath}`, (markBodyEnd) =>
				batch(() => {
					// Diff stats are keyed by checkout directory, last-commit timestamps
					// by branch, and store writes by workspace id.
					for (const [workspaceId, workspace] of Object.entries(currentRepoForStats.workspaces)) {
						if (!workspace.worktreePath) continue;
						const ds = stats.diff_stats[workspace.worktreePath];
						if (ds) {
							repositoriesStore.updateWorkspaceStats(repoPath, workspaceId, ds.additions, ds.deletions);
						}
						const ts = stats.last_commit_ts?.[workspace.branchName];
						const lifecycle = stats.workspace_statuses?.[workspaceId];
						if (lifecycle) {
							repositoriesStore.setWorkspace(repoPath, workspaceId, {
								isMerged: lifecycle.commit_status === "merged",
								lifecycleStatus: {
									dirtyFiles: lifecycle.dirty_files,
									commitStatus: lifecycle.commit_status,
									removalSafety: lifecycle.removal_safety,
									error: lifecycle.error,
								},
							});
						}
						if (ts !== undefined) {
							// Rust emits Unix seconds (%ct); JS Date.now() uses milliseconds
							repositoriesStore.setWorkspace(repoPath, workspaceId, {
								lastCommitTs: ts !== null ? ts * 1000 : null,
							});
						}
					}
					markBodyEnd();
				}),
			);
		} catch (err) {
			appLogger.warn("git", `Phase 2 diff stats failed for ${repoPath}`, err);
		}
	};

	// Per-repo single-flight with one trailing rerun. The former generation
	// cancellation made structure reconciliation starvation-prone: a sustained
	// repo-changed stream could obsolete every in-flight Phase 1 before it pruned
	// deleted worktrees, leaving persisted ghost rows in the sidebar forever.
	// Automatic starts are at least 5s apart per repo. Explicit UI operations
	// can wake a trailing wait, but still join any active backend pass.
	const refreshInFlight = new Map<string, Promise<void>>();
	const refreshQueued = new Set<string>();
	const refreshStartedAt = new Map<string, number>();
	const MIN_REFRESH_INTERVAL_MS = 5_000;
	const refreshImmediate = new Set<string>();
	const wakeRefresh = new Map<string, () => void>();
	const refreshRepo = async (repoPath: string, immediate: boolean): Promise<void> => {
		if (immediate) {
			refreshImmediate.add(repoPath);
			wakeRefresh.get(repoPath)?.();
		}
		const existing = refreshInFlight.get(repoPath);
		if (existing) {
			refreshQueued.add(repoPath);
			await existing;
			return;
		}

		const run = (async () => {
			do {
				const now = Date.now();
				const startedAt = refreshStartedAt.get(repoPath);
				// A start stamped in the future means the clock went back: refresh now rather
				// than wait out the whole skew.
				const sinceStart = startedAt === undefined ? Number.POSITIVE_INFINITY : now - startedAt;
				const remaining = sinceStart < 0 ? 0 : MIN_REFRESH_INTERVAL_MS - sinceStart;
				if (remaining > 0 && !refreshImmediate.has(repoPath)) {
					await new Promise<void>((resolve) => {
						const timer = setTimeout(resolve, remaining);
						wakeRefresh.set(repoPath, () => {
							clearTimeout(timer);
							resolve();
						});
					});
					wakeRefresh.delete(repoPath);
				}
				refreshImmediate.delete(repoPath);
				// Requests received while waiting belong to this pass, not another rerun.
				refreshQueued.delete(repoPath);
				// A repo may have been removed or parked during the trailing wait.
				if (!repositoriesStore.getActivePaths().includes(repoPath)) break;
				const start = Date.now();
				refreshStartedAt.set(repoPath, start);
				try {
					await refreshRepoOnce(repoPath);
				} catch (err) {
					appLogger.warn("git", `Repository refresh failed for ${repoPath}`, err);
				}
			} while (refreshQueued.delete(repoPath));
		})();
		refreshInFlight.set(repoPath, run);
		try {
			await run;
		} finally {
			if (refreshInFlight.get(repoPath) === run) refreshInFlight.delete(repoPath);
		}
	};

	// `scopeRepoPath` limits the refresh to a single repo. A `repo-changed`
	// event carries the one repo that changed, so scoping avoids re-scanning
	// every open repo in unison on each filesystem event. Called with no arg
	// (init, branch ops) it refreshes all active repos as before.
	const refreshReposCapped = async (paths: string[], maxConcurrent: number, immediate: boolean): Promise<void> => {
		let nextIndex = 0;
		const worker = async () => {
			while (nextIndex < paths.length) {
				const path = paths[nextIndex++];
				await refreshRepo(path, immediate);
			}
		};
		await Promise.all(Array.from({ length: Math.min(maxConcurrent, paths.length) }, worker));
	};

	const refreshAllBranchStats = async (scopeRepoPath?: string, options: { immediate?: boolean } = {}) => {
		// Skip parked repos — they should stay dormant. (#1358-caf5)
		const activePaths = repositoriesStore.getActivePaths();
		const paths = scopeRepoPath ? activePaths.filter((p) => p === scopeRepoPath) : [...activePaths];
		const activeRepoPath = repositoriesStore.state.activeRepoPath;
		if (activeRepoPath && paths.includes(activeRepoPath)) {
			paths.splice(paths.indexOf(activeRepoPath), 1);
			paths.unshift(activeRepoPath);
		}
		await refreshReposCapped(paths, 4, options.immediate === true);
	};

	/** Detect orphaned linked worktrees and act based on the orphanCleanup setting. */
	let orphanDialogOpen = false;
	// Orphans the user chose to "Keep" this session, with the fingerprint they had:
	// a Keep holds until the checkout changes, then the orphan is judged afresh. (#65)
	// Session-scoped (re-detected on next launch).
	const keptOrphans = new Map<string, string | undefined>();
	const isKept = (entry: OrphanAssessment) =>
		keptOrphans.has(entry.path) && keptOrphans.get(entry.path) === entry.dirty_fingerprint;
	/** Remove one orphan, then close its terminals. Resolves with how many terminals could not be closed. The backend verdict comes first: a session
	 *  that started after the assessment makes the backend refuse, and its terminal must survive.
	 *  A review-confirmed entry carries the session ids the user saw; the backend refuses if
	 *  another one appeared since. */
	const removeOrphan = async (repoPath: string, entry: OrphanAssessment): Promise<number> => {
		if (entry.safe) {
			await deps.repo.removeOrphanWorktree(repoPath, entry.path, true);
		} else {
			const seen = (entry.live_sessions ?? []).map((session) => session.session_id);
			await deps.repo.removeOrphanWorktree(repoPath, entry.path, false, seen);
		}
		// The checkout is gone: a terminal that will not close must not turn that into a failed removal.
		try {
			await deps.closeTerminalsInWorktree(entry.path);
			return 0;
		} catch (err) {
			appLogger.warn("git", `Removed orphan worktree ${entry.path} but could not close its terminals`, err);
			return err instanceof AggregateError ? err.errors.length : 1;
		}
	};
	interface OrphanRemovalTally {
		removed: number;
		unclosedTerminals: number;
	}
	/** Remove the orphans concurrently and add the outcome to `tally` once all settled: each task
	 *  returns its own count, so concurrent removals cannot overwrite one another's. */
	const removeOrphans = async (
		repoPath: string,
		entries: OrphanAssessment[],
		failure: string,
		tally: OrphanRemovalTally,
	) => {
		const results = await Promise.allSettled(
			entries.map(async (entry) => {
				try {
					return await removeOrphan(repoPath, entry);
				} catch (err) {
					appLogger.warn("git", `${failure} ${entry.path}`, err);
					return null;
				}
			}),
		);
		for (const result of results) {
			if (result.status !== "fulfilled" || result.value === null) continue;
			tally.removed++;
			tally.unclosedTerminals += result.value;
		}
	};
	/** One status line for the whole sweep, whichever phase removed the checkouts. */
	const handleOrphanCleanup = async (repoPath: string) => {
		const tally: OrphanRemovalTally = { removed: 0, unclosedTerminals: 0 };
		try {
			await sweepOrphans(repoPath, tally);
		} finally {
			if (tally.removed > 0) {
				const unclosed =
					tally.unclosedTerminals > 0 ? `; ${tally.unclosedTerminals} terminal(s) could not be closed` : "";
				deps.setStatusInfo(`Removed ${tally.removed} orphaned worktree(s)${unclosed}`);
			}
		}
	};
	const sweepOrphans = async (repoPath: string, tally: OrphanRemovalTally) => {
		const orphanCleanup = repoSettingsStore.getEffective(repoPath)?.orphanCleanup ?? "ask";
		if (orphanCleanup === "off") return;

		let assessments: OrphanAssessment[];
		try {
			assessments = await deps.repo.assessOrphanCleanup(repoPath);
		} catch {
			return; // Detection failure is non-fatal
		}
		if (assessments.length === 0) return;

		// A checkout the backend judged safe (clean, reachable from a branch, no live
		// session) is removed without asking, in both modes: the same rule as any
		// merged clean worktree. Whatever else is left waits for the review below;
		// in "on" mode only a checkout a session still works in does.
		await removeOrphans(
			repoPath,
			assessments.filter((entry) => entry.safe),
			"Failed to auto-remove orphan worktree",
			tally,
		);
		const unsafe = assessments.filter((entry) => !entry.safe);
		const reviewable =
			orphanCleanup === "on" ? unsafe.filter((entry) => (entry.live_sessions?.length ?? 0) > 0) : unsafe;

		// Skip orphans the user already chose to keep until their fingerprint moves —
		// otherwise the dialog re-fires on every refresh. (#65)
		const pending = reviewable.filter((entry) => !isKept(entry));
		if (pending.length === 0) return;

		if (orphanDialogOpen) return; // Prevent duplicate dialogs from concurrent refreshes
		orphanDialogOpen = true;
		let confirmed: boolean | undefined;
		let poll: ReturnType<typeof setInterval> | undefined;
		let dialogActive = true;
		try {
			await deps.repo.beginOrphanCleanup(
				repoPath,
				pending.map((entry) => entry.path),
			);
			let polling = false;
			poll = setInterval(async () => {
				if (polling) return;
				polling = true;
				try {
					const answer = await deps.repo.pendingOrphanCleanupAnswer(repoPath);
					if (dialogActive && answer !== null) {
						clearInterval(poll);
						deps.dialogs.answerOrphanCleanup?.(repoPath, answer);
					}
				} catch (err) {
					appLogger.warn("git", `Failed to read pending orphan cleanup answer for ${repoPath}`, err);
				} finally {
					polling = false;
				}
			}, 500);
			confirmed =
				(await deps.dialogs.confirmOrphanCleanup?.(
					repoPath,
					pending,
					Math.max(1, repoDefaultsStore.state.orphanCleanupCountdownSeconds),
				)) ?? false;
			// An agent's Keep answer wins if it arrived at the end of the countdown,
			// before the next poll could settle the dialog.
			const finalAnswer = await deps.repo.pendingOrphanCleanupAnswer(repoPath);
			if (finalAnswer !== null) confirmed = finalAnswer;
		} finally {
			dialogActive = false;
			if (poll) clearInterval(poll);
			try {
				// A Keep stays on the backend so other clients showing this dialog
				// see it instead of counting down to a removal.
				await deps.repo.clearOrphanCleanup(repoPath, confirmed === false);
			} finally {
				orphanDialogOpen = false;
			}
		}
		if (!confirmed) {
			// User chose "Keep" — remember these so we don't prompt again this session.
			for (const entry of pending) keptOrphans.set(entry.path, entry.dirty_fingerprint);
			return;
		}

		await removeOrphans(repoPath, pending, "Failed to remove orphan worktree", tally);
	};

	/** Archive all merged linked worktrees when the autoArchiveMerged setting is enabled. */
	const handleAutoArchiveMerged = async (repoPath: string, branches: RepositoryState["workspaces"]) => {
		if (!repoSettingsStore.getEffective(repoPath)?.autoArchiveMerged) return;

		const mergedLinkedBranches = Object.values(branches).filter(
			(b) => b.kind === "worktree" && b.isMerged && b.worktreePath !== null && b.worktreePath !== repoPath,
		);
		if (mergedLinkedBranches.length === 0) return;

		let archived = 0;
		const kept: string[] = [];
		const safe: typeof mergedLinkedBranches = [];
		for (const ws of mergedLinkedBranches) {
			try {
				const preview = await deps.repo.getWorkspaceLifecycle(repoPath, ws.workspaceId);
				if (
					preview.commitStatus === "merged" &&
					preview.removalSafety === "safe" &&
					preview.dirtyFiles === 0 &&
					!preview.liveSessions?.length
				) {
					safe.push(ws);
				} else {
					kept.push(`${ws.branchName}: ${(preview.warnings ?? []).join("; ") || "removal needs review"}`);
				}
			} catch (error) {
				kept.push(`${ws.branchName}: removal preview failed`);
				appLogger.warn("git", `Could not inspect ${ws.branchName} before auto-archive`, error);
			}
		}
		const results = await Promise.allSettled(
			// By workspaceId, never branchName: with two workspaces on one branch the
			// branch cannot say which checkout to archive (#726-5ac7).
			safe.map((ws) => deps.repo.finalizeMergedWorktree(repoPath, ws.workspaceId, "archive")),
		);
		results.forEach((result, i) => {
			const name = safe[i].branchName;
			if (result.status === "rejected") {
				appLogger.warn("git", `Failed to auto-archive merged worktree for "${name}"`, result.reason);
				return;
			}
			// This sweep runs on a refresh tick with nobody watching, so it never
			// passes `force`. A worktree that is not known to be clean comes back
			// untouched and stays in the sidebar for the user to handle by hand.
			if (result.value.action === "needs_confirmation") {
				kept.push(`${name}: uncommitted work`);
				appLogger.info("git", `Kept the merged worktree for "${name}" — it has uncommitted work`);
				return;
			}
			archived++;
		});
		if (archived > 0 || kept.length > 0) {
			const keptNote = kept.length > 0 ? `, kept ${kept.length}: ${kept.join(" | ")}` : "";
			deps.setStatusInfo(`Auto-archived ${archived} merged worktree(s)${keptNote}`);
		}
	};

	return { markRecentlyCreated, refreshAllBranchStats };
}
