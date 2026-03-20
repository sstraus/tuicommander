import { onCleanup } from "solid-js";
import { listen, invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import type { ConfirmOptions } from "./useConfirmDialog";

interface WorktreeSwitchDeps {
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
}

interface WorktreeCreatedPayload {
  repo_path: string;
  branch: string;
  worktree_path: string;
}

/**
 * Listens for worktree-created events (from MCP) and offers to switch
 * the active tab + terminal to the new worktree.
 */
export function useWorktreeSwitchPrompt(deps: WorktreeSwitchDeps): void {
  let unlisten: (() => void) | null = null;

  listen<WorktreeCreatedPayload>("worktree-created", (event) => {
    const { repo_path, branch, worktree_path } = event.payload;
    handlePrompt(repo_path, branch, worktree_path).catch((err) =>
      appLogger.warn("git", "Worktree switch prompt failed", err),
    );
  })
    .then((fn) => {
      unlisten = fn;
    })
    .catch((err) =>
      appLogger.error("app", "Failed to register worktree-created listener", err),
    );

  onCleanup(() => unlisten?.());

  async function handlePrompt(
    repoPath: string,
    branch: string,
    worktreePath: string,
  ): Promise<void> {
    const confirmed = await deps.confirm({
      title: "Switch to new worktree?",
      message: `Worktree "${branch}" was created.\nSwitch to it now?`,
      okLabel: "Switch",
      cancelLabel: "Stay",
      kind: "info",
    });
    if (!confirmed) return;

    // Switch the tab/branch
    await deps.handleBranchSelect(repoPath, branch);

    // cd the active terminal into the worktree path
    const activeTerm = terminalsStore.getActive();
    if (activeTerm?.sessionId) {
      await invoke("write_pty", {
        id: activeTerm.sessionId,
        data: `cd ${shellEscape(worktreePath)}\n`,
      });
    }
  }
}

/** Minimal shell escaping — wrap in single quotes, escape existing quotes */
function shellEscape(path: string): string {
  if (!/[^a-zA-Z0-9_.\/\-]/.test(path)) return path;
  return `'${path.replace(/'/g, "'\\''")}'`;
}
