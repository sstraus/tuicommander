import { ask, message } from "@tauri-apps/plugin-dialog";

export interface ConfirmOptions {
  title: string;
  message: string;
  okLabel?: string;
  cancelLabel?: string;
  kind?: "info" | "warning" | "error";
}

/** Hook for native confirmation dialogs */
export function useConfirmDialog() {
  /** Show a confirmation dialog */
  async function confirm(options: ConfirmOptions): Promise<boolean> {
    return await ask(options.message, {
      title: options.title,
      okLabel: options.okLabel || "OK",
      cancelLabel: options.cancelLabel || "Cancel",
      kind: options.kind || "warning",
    });
  }

  /** Show an info message */
  async function info(title: string, msg: string): Promise<void> {
    await message(msg, { title, kind: "info" });
  }

  /** Show an error message */
  async function error(title: string, msg: string): Promise<void> {
    await message(msg, { title, kind: "error" });
  }

  /** Confirm removing a worktree/branch */
  async function confirmRemoveWorktree(branchName: string): Promise<boolean> {
    return await confirm({
      title: "Remove worktree?",
      message: `Remove ${branchName}?\nThis deletes the worktree directory and its local branch.`,
      okLabel: "Remove",
      cancelLabel: "Cancel",
      kind: "warning",
    });
  }

  /** Confirm closing a terminal */
  async function confirmCloseTerminal(terminalName: string): Promise<boolean> {
    return await confirm({
      title: "Close terminal?",
      message: `Close ${terminalName}?\nAny running processes will be terminated.`,
      okLabel: "Close",
      cancelLabel: "Cancel",
      kind: "warning",
    });
  }

  /** Confirm removing a repository */
  async function confirmRemoveRepo(repoName: string): Promise<boolean> {
    return await confirm({
      title: "Remove repository?",
      message: `Remove ${repoName} from the list?\nThis does not delete any files.`,
      okLabel: "Remove",
      cancelLabel: "Cancel",
      kind: "warning",
    });
  }

  return {
    confirm,
    info,
    error,
    confirmRemoveWorktree,
    confirmCloseTerminal,
    confirmRemoveRepo,
  };
}
