import { invoke } from "../invoke";
import { repositoriesStore } from "../stores/repositories";
import { appLogger } from "../stores/appLogger";
import type { StepId } from "../components/PostMergeCleanupDialog/PostMergeCleanupDialog";

export interface CleanupConfig {
  repoPath: string;
  branchName: string;
  baseBranch: string;
  steps: { id: StepId; checked: boolean }[];
  onStepStart: (id: StepId) => void;
  onStepDone: (id: StepId, result: "success" | "error", error?: string) => void;
  closeTerminalsForBranch: (repoPath: string, branchName: string) => Promise<void>;
  /** When set, the "worktree" step calls finalize_merged_worktree with this action */
  worktreeAction?: "archive" | "delete";
}

/** Execute post-merge cleanup steps sequentially via Rust backend commands. */
export async function executeCleanup(config: CleanupConfig): Promise<void> {
  const { repoPath, branchName, baseBranch, steps, onStepStart, onStepDone } = config;
  let didDeleteLocal = false;
  let hadError = false;

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
          // Pre-check for dirty working directory
          const status = await invoke<{ stdout: string; stderr: string }>("run_git_command", {
            path: repoPath,
            args: ["status", "--porcelain"],
          });
          if (status.stdout.trim().length > 0) {
            throw new Error("Working directory has uncommitted changes — commit or stash first");
          }
          await invoke("switch_branch", {
            repoPath,
            branch: baseBranch,
            force: false,
            stash: false,
          });
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
          await invoke("delete_local_branch", {
            repoPath,
            branchName,
          });
          didDeleteLocal = true;
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
