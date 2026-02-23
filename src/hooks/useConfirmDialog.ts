import { createSignal } from "solid-js";

export interface ConfirmOptions {
  title: string;
  message: string;
  okLabel?: string;
  cancelLabel?: string;
  kind?: "info" | "warning" | "error";
}

/** Internal state for the currently visible confirm dialog */
export interface ConfirmDialogState {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  kind: "info" | "warning" | "error";
}

/**
 * Hook for confirmation dialogs — renders an in-app ConfirmDialog
 * instead of native OS dialogs for consistent dark-theme styling.
 *
 * confirm() returns a Promise<boolean> that resolves when the user
 * clicks confirm or cancel (or presses Enter/Escape).
 */
export function useConfirmDialog() {
  const [dialogState, setDialogState] = createSignal<ConfirmDialogState | null>(null);
  let pendingResolve: ((value: boolean) => void) | null = null;

  /** Show a confirmation dialog — resolves true on confirm, false on cancel */
  function confirm(options: ConfirmOptions): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      pendingResolve = resolve;
      setDialogState({
        title: options.title,
        message: options.message,
        confirmLabel: options.okLabel || "OK",
        cancelLabel: options.cancelLabel || "Cancel",
        kind: options.kind || "warning",
      });
    });
  }

  /** Called when user confirms */
  function handleConfirm() {
    setDialogState(null);
    if (pendingResolve) {
      pendingResolve(true);
      pendingResolve = null;
    }
  }

  /** Called when user cancels (button, Escape, or overlay click) */
  function handleClose() {
    setDialogState(null);
    if (pendingResolve) {
      pendingResolve(false);
      pendingResolve = null;
    }
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
    confirmRemoveWorktree,
    confirmCloseTerminal,
    confirmRemoveRepo,
    /** Reactive state for rendering the dialog — null when hidden */
    dialogState,
    /** Handler for confirm button / Enter key */
    handleConfirm,
    /** Handler for cancel button / Escape key / overlay click */
    handleClose,
  };
}
