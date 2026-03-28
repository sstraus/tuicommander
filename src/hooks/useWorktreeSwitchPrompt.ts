import { onCleanup } from "solid-js";
import { listen, invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { activityStore } from "../stores/activityStore";
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
    activityStore.addItem({
      id: `wt-${branch}-${Date.now()}`,
      pluginId: "core",
      sectionId: "worktrees",
      title: `Worktree: ${branch}`,
      subtitle: worktree_path.split("/").slice(-2).join("/"),
      icon: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0zm0 2.122a2.25 2.25 0 1 0-1.5 0v.878A2.25 2.25 0 0 0 5.75 8.5h1.5v2.128a2.251 2.251 0 1 0 1.5 0V8.5h1.5a2.25 2.25 0 0 0 2.25-2.25v-.878a2.25 2.25 0 1 0-1.5 0v.878a.75.75 0 0 1-.75.75h-5a.75.75 0 0 1-.75-.75v-.878zM8 12.25a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5zm3.25-9a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5z"/></svg>',
      repoPath: repo_path,
      dismissible: true,
    });
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
