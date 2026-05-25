import type { StepId } from "../components/PostMergeCleanupDialog/PostMergeCleanupDialog";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";

export interface CleanupConfig {
	repoPath: string;
	branchName: string;
	baseBranch: string;
	steps: { id: StepId; checked: boolean }[];
	onStepStart: (id: StepId) => void;
	onStepDone: (id: StepId, result: "success" | "error", error?: string) => void;
	onStepNote?: (id: StepId, note: string) => void;
	closeTerminalsForBranch: (repoPath: string, branchName: string) => Promise<void>;
	/** When set, the "worktree" step calls finalize_merged_worktree with this action */
	worktreeAction?: "archive" | "delete";
	/** When true, pop the stash after switching branches */
	unstash?: boolean;
}

/** Execute post-merge cleanup steps sequentially via Rust backend commands. */
export async function executeCleanup(config: CleanupConfig): Promise<void> {
	const { repoPath, branchName, baseBranch, steps, onStepStart, onStepDone } = config;
	let didDeleteLocal = false;
	let hadError = false;

	// When the "worktree" step is present in the list but unchecked, the user
	// explicitly chose to keep the worktree on disk. The "delete-local" step
	// must communicate that to the Rust side, otherwise `delete_local_branch`
	// will cascade through `remove_worktree_by_branch` and destroy the
	// worktree directory regardless of the user's intent.
	const worktreeStep = steps.find((s) => s.id === "worktree");
	const keepWorktree = worktreeStep !== undefined && !worktreeStep.checked;

	for (const step of steps) {
		if (!step.checked) continue;
		if (hadError) break;

		onStepStart(step.id);
		try {
			switch (step.id) {
				case "worktree": {
					if (!config.worktreeAction) break; // no-op if action not set
					await invoke("finalize_merged_worktree", {
						repoPath,
						branchName,
						action: config.worktreeAction,
					});
					break;
				}

				case "switch": {
					await invoke("switch_branch", {
						repoPath,
						branchName: baseBranch,
						force: false,
						stash: true,
					});
					if (config.unstash) {
						await invoke("run_git_command", {
							path: repoPath,
							args: ["stash", "pop"],
						});
					}
					break;
				}

				case "pull":
					await invoke("run_git_command", {
						path: repoPath,
						args: ["pull", "--ff-only"],
					});
					break;

				case "delete-local":
					await config.closeTerminalsForBranch(repoPath, branchName);
					try {
						await invoke("delete_local_branch", {
							repoPath,
							branchName,
							keepWorktree,
						});
					} catch (e) {
						const msg = String(e);
						if (msg.includes("not found") || msg.includes("no such branch")) {
							// Already deleted — treat as success
							appLogger.info("git", `Local branch ${branchName} already deleted`);
							onStepDone(step.id, "success", undefined);
							continue;
						}
						throw e;
					}
					didDeleteLocal = true;
					if (keepWorktree) {
						config.onStepNote?.(step.id, "Worktree kept — HEAD is now detached");
					}
					break;

				case "delete-remote":
					try {
						await invoke("run_git_command", {
							path: repoPath,
							args: ["push", "origin", "--delete", branchName],
						});
					} catch (e) {
						const msg = String(e);
						if (msg.includes("remote ref does not exist")) {
							// Already deleted on remote — treat as success
							appLogger.info("git", `Remote branch ${branchName} already deleted`);
							onStepDone(step.id, "success", undefined);
							continue;
						}
						throw e;
					}
					break;
			}
			onStepDone(step.id, "success", undefined);
		} catch (e) {
			const errorMsg = e instanceof Error ? e.message : String(e);
			onStepDone(step.id, "error", errorMsg);
			appLogger.error("git", `Post-merge cleanup step "${step.id}" failed`, { error: errorMsg });
			hadError = true;
		}
	}

	// Update frontend state
	if (didDeleteLocal) {
		repositoriesStore.removeBranch(repoPath, branchName);
	}
	repositoriesStore.bumpRevision(repoPath);
}
