import { onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { githubStore } from "../stores/github";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { appLogger } from "../stores/appLogger";
import type { ConfirmOptions } from "./useConfirmDialog";

interface AutoDeleteDeps {
  confirm: (options: ConfirmOptions) => Promise<boolean>;
}

/**
 * Handles automatic deletion of local branches when their PR is merged or closed.
 *
 * Reads the per-repo `autoDeleteOnPrClose` setting (off/ask/auto) and:
 * - off: does nothing
 * - ask: shows a confirm dialog
 * - auto: deletes silently (falls back to ask if worktree is dirty)
 *
 * Safety: never deletes the default/main branch.
 */
export function useAutoDeleteBranch(deps: AutoDeleteDeps): void {
  /** Track processed transitions to prevent double-firing */
  const processed = new Set<string>();

  function handlePrTerminal(repoPath: string, branch: string, prNumber: number, type: "merged" | "closed"): void {
    const key = `${repoPath}:${prNumber}`;
    if (processed.has(key)) return;
    processed.add(key);

    // Don't block the polling loop — run async
    processAutoDelete(repoPath, branch, prNumber, type).catch((err) =>
      appLogger.warn("git", `Auto-delete failed for ${branch}`, err),
    );
  }

  async function processAutoDelete(repoPath: string, branch: string, prNumber: number, type: "merged" | "closed"): Promise<void> {
    // Check setting
    const effective = repoSettingsStore.getEffective(repoPath);
    const mode = effective?.autoDeleteOnPrClose ?? "off";
    if (mode === "off") return;

    // Never delete the default/main branch
    const repo = repositoriesStore.get(repoPath);
    if (repo) {
      const branchState = repo.branches[branch];
      if (branchState?.isMain) {
        appLogger.debug("git", `Skipping auto-delete for default branch '${branch}'`);
        return;
      }
    }

    // Check if branch even exists locally
    const branches = repo?.branches;
    if (branches && !(branch in branches)) {
      // Branch doesn't exist locally — nothing to delete
      return;
    }

    let effectiveMode = mode;

    // If auto mode, check dirty state first
    if (effectiveMode === "auto") {
      try {
        const dirty = await invoke<boolean>("check_worktree_dirty", { repoPath, branchName: branch });
        if (dirty) {
          appLogger.info("git", `Branch '${branch}' has uncommitted changes — asking before deleting`);
          effectiveMode = "ask";
        }
      } catch {
        // If dirty check fails, fall back to ask
        effectiveMode = "ask";
      }
    }

    if (effectiveMode === "ask") {
      const action = type === "merged" ? "merged" : "closed";
      const confirmed = await deps.confirm({
        title: "Delete local branch?",
        message: `PR #${prNumber} was ${action}.\nDelete local branch '${branch}'?`,
        okLabel: "Delete",
        cancelLabel: "Keep",
        kind: "warning",
      });
      if (!confirmed) return;
    }

    // Perform deletion
    try {
      await invoke("delete_local_branch", { repoPath, branchName: branch });
      repositoriesStore.bumpRevision(repoPath);
      appLogger.info("git", `Auto-deleted branch '${branch}' (PR #${prNumber} ${type})`);
    } catch (err) {
      appLogger.warn("git", `Failed to delete branch '${branch}': ${err}`);
    }
  }

  githubStore.setOnPrTerminal(handlePrTerminal);

  onCleanup(() => {
    githubStore.setOnPrTerminal(null);
    processed.clear();
  });
}
