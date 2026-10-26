import type { Accessor, Setter } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { closeWorktreeTerminals, removeWorktreeWorkspace } from "../../stores/terminalOwnership";
import type { WorkspaceLifecycleStatus } from "../../stores/workspaceIdentity";
import { reportActionError } from "../../utils/reportActionError";
import type { RemoveWorktreeResult } from "../useRepository";

interface WorktreeRemovalCoordinatorDeps {
	repo: {
		removeWorktree: (
			repoPath: string,
			workspaceId: string,
			deleteBranch: boolean,
			force?: boolean,
			overrideLock?: boolean,
			expectedFingerprint?: string,
			confirmMissingCheckout?: boolean,
		) => Promise<RemoveWorktreeResult | undefined>;
		getWorkspaceLifecycle: (repoPath: string, workspaceId: string) => Promise<WorkspaceLifecycleStatus>;
	};
	dialogs: {
		confirmRemoveWorktree: (
			branchName: string,
			status: WorkspaceLifecycleStatus,
			deleteBranch: boolean,
		) => Promise<boolean>;
		confirmRemoveLockedWorktree?: (branchName: string, deleteBranch?: boolean) => Promise<boolean>;
	};
	closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
	setStatusInfo: (message: string) => void;
	removingBranches: Accessor<Set<string>>;
	setRemovingBranches: Setter<Set<string>>;
}

function describeRemoveWorktreeSuccess(branchName: string, outcome: RemoveWorktreeResult | undefined): string {
	if (outcome?.branch_delete_warning) {
		return `Removed ${branchName} worktree; branch was kept: ${outcome.branch_delete_warning}`;
	}
	if (outcome?.removal_rule === "recyclable") {
		return `Archived ${branchName} worktree for reuse; branch was kept`;
	}
	return `Removed ${branchName}`;
}

/** Owns worktree removal locking, force recovery, and store cleanup. */
/** Mirrors the backend: recycling and keeping the branch leave its label alone. */
function branchWasDeleted(outcome: RemoveWorktreeResult | undefined): boolean {
	return (
		!outcome?.branch_delete_warning && outcome?.removal_rule !== "recyclable" && outcome?.removal_rule !== "kept_branch"
	);
}

export function createWorktreeRemovalCoordinator(deps: WorktreeRemovalCoordinatorDeps) {
	const { removingBranches, setRemovingBranches } = deps;

	/** `workspaceId` addresses the row and the backend. `branchName`, read off the
	 *  record, is what the user sees in the confirm dialog and the status line —
	 *  showing a minted id there would be showing a user an internal key. */
	const handleRemoveWorkspace = async (repoPath: string, workspaceId: string, keepBranch = false) => {
		const removeKey = `${repoPath}::${workspaceId}`;
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

		try {
			const repoState = repositoriesStore.get(repoPath);
			const branch = repoState?.workspaces[workspaceId];
			if (!branch?.worktreePath) {
				reportActionError(deps.setStatusInfo, `Cannot remove ${workspaceId}: not a worktree`);
				return;
			}
			const branchName = branch.branchName;

			const effective = repoSettingsStore.getEffective(repoPath);
			const deleteBranch = !keepBranch && (effective?.deleteBranchOnRemove ?? true);
			let lifecycle: WorkspaceLifecycleStatus;
			try {
				lifecycle = await deps.repo.getWorkspaceLifecycle(repoPath, workspaceId);
			} catch (err) {
				appLogger.warn("git", `workspace lifecycle preflight failed for ${workspaceId}`, err);
				reportActionError(deps.setStatusInfo, `Cannot verify whether ${branchName} is safe to remove`);
				return;
			}
			if (lifecycle.removalSafety === "unknown") {
				reportActionError(
					deps.setStatusInfo,
					`Cannot verify whether ${branchName} is safe to remove: ${lifecycle.error ?? "unknown state"}`,
				);
				return;
			}
			if (deleteBranch && lifecycle.commitStatus === "unmerged") {
				reportActionError(
					deps.setStatusInfo,
					`Cannot delete ${branchName}: the branch has unmerged commits. You can remove the worktree while keeping the branch.`,
					{
						label: "Remove worktree, keep branch",
						onClick: () =>
							handleRemoveWorkspace(repoPath, workspaceId, true).catch((err) => {
								reportActionError(deps.setStatusInfo, `Failed to remove ${branchName}: ${String(err)}`);
							}),
					},
				);
				return;
			}

			const confirmed = await deps.dialogs.confirmRemoveWorktree(branchName, lifecycle, deleteBranch);
			if (!confirmed) {
				return;
			}

			// Show "Removing…" in sidebar as soon as the user confirms — before
			// the terminal-close loop, which can take noticeable time. Otherwise
			// the lock is held while the UI still appears clickable.
			repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: true });

			// Use the same live-cwd authority as backend removal events and refresh.
			// The helper tries every resident even if a sibling close fails.
			try {
				await closeWorktreeTerminals(branch.worktreePath, deps.closeTerminal);
			} catch (err) {
				appLogger.warn("git", "handleRemoveWorkspace: terminal cleanup failed", {
					workspaceId,
					error: err instanceof Error ? err.message : String(err),
				});
			}

			appLogger.info("git", `handleRemoveWorkspace: invoking remove_worktree`, {
				repoPath,
				workspaceId,
				worktreePath: branch.worktreePath,
				deleteBranch,
			});

			// Tracks whether to remove the branch from the store at the end.
			// Set to true on success or non-fatal non-lock errors (old "remove from UI" behavior).
			// Stays false when: locked+cancelled, or force-remove failed (worktree still in git).
			let shouldRemoveFromStore = false;
			let shouldClearBranchLabel = true;
			const removeConfirmed = (overrideLock: boolean) => {
				if (lifecycle.removalSafety === "requires_force") {
					if (lifecycle.missingCheckout) {
						return deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, true, overrideLock, undefined, true);
					}
					if (!lifecycle.dirtyFingerprint) {
						throw new Error("Cannot verify the confirmed worktree state");
					}
					return deps.repo.removeWorktree(
						repoPath,
						workspaceId,
						deleteBranch,
						true,
						overrideLock,
						lifecycle.dirtyFingerprint,
					);
				}
				return overrideLock
					? deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, false, true)
					: deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, false);
			};
			try {
				// The user confirmed knowing the count, so the backend guard would only
				// bounce a decision that has already been made.
				const outcome = await removeConfirmed(false);
				appLogger.info("git", `handleRemoveWorkspace: remove_worktree SUCCESS`, { workspaceId });
				shouldRemoveFromStore = true;
				shouldClearBranchLabel = deleteBranch && branchWasDeleted(outcome);
				deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
			} catch (err) {
				const reason = err instanceof Error ? err.message : String(err);
				if (reason.startsWith("worktree_locked:")) {
					// Worktree is locked by an agent — ask for a separate lock override.
					repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
					appLogger.warn("git", `handleRemoveWorkspace: worktree locked — showing confirmation dialog`, {
						workspaceId,
						reason,
					});
					// Pass deleteBranch so the dialog can describe the requested cleanup.
					// Report dialog rejection; the outer finally releases the lock.
					let forceConfirmed = false;
					try {
						forceConfirmed = await (deps.dialogs.confirmRemoveLockedWorktree?.(branchName, deleteBranch) ?? false);
					} catch (dialogErr) {
						appLogger.error("git", `handleRemoveWorkspace: confirmRemoveLockedWorktree threw`, {
							workspaceId,
							error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
						});
						reportActionError(deps.setStatusInfo, `Failed to confirm force-remove for ${branchName}`);
						return;
					}
					if (!forceConfirmed) {
						appLogger.info("git", `handleRemoveWorkspace: user cancelled force removal of locked worktree`, {
							workspaceId,
						});
						return;
					}
					repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: true });
					try {
						const outcome = await removeConfirmed(true);
						appLogger.info("git", `handleRemoveWorkspace: force remove_worktree SUCCESS`, { workspaceId });
						shouldRemoveFromStore = true;
						shouldClearBranchLabel = deleteBranch && branchWasDeleted(outcome);
						deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
					} catch (forceErr) {
						const forceReason = forceErr instanceof Error ? forceErr.message : String(forceErr);
						appLogger.error("git", `handleRemoveWorkspace: force remove_worktree FAILED`, {
							workspaceId,
							reason: forceReason,
						});
						reportActionError(deps.setStatusInfo, `Failed to remove ${branchName}: ${forceReason}`);
						repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
						return;
					}
				} else if (reason.startsWith("worktree_is_main:")) {
					appLogger.warn("git", `handleRemoveWorkspace: branch is in main worktree — cannot remove as worktree`, {
						workspaceId,
					});
					reportActionError(
						deps.setStatusInfo,
						`Cannot remove ${branchName}: branch is in the main worktree, not a linked worktree`,
					);
					repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
					return;
				} else {
					appLogger.error("git", `handleRemoveWorkspace: remove_worktree FAILED — workspace kept`, {
						workspaceId,
						reason,
					});
					reportActionError(deps.setStatusInfo, `Failed to remove ${branchName}: ${reason}`);
					repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
					return;
				}
			}

			if (!shouldRemoveFromStore) {
				repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
				return;
			}
			appLogger.info("git", `handleRemoveWorkspace: calling removeWorkspace on store`, { workspaceId });
			removeWorktreeWorkspace(repoPath, workspaceId);
			if (shouldClearBranchLabel) {
				repoSettingsStore.setLabel(repoPath, branchName, null);
			}
		} finally {
			clearLock();
		}
	};

	return { handleRemoveWorkspace };
}
