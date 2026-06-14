import { createSignal } from "solid-js";

export interface ConfirmOptions {
	title: string;
	message: string;
	okLabel?: string;
	cancelLabel?: string;
	kind?: "info" | "warning" | "error";
	/** When set, the dialog auto-clicks cancel after this many ms, with a countdown on the cancel label. */
	autoCancelMs?: number;
}

/** Internal state for the currently visible confirm dialog */
export interface ConfirmDialogState {
	title: string;
	message: string;
	confirmLabel: string;
	cancelLabel: string;
	kind: "info" | "warning" | "error";
	autoCancelMs?: number;
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
	// FIFO queue of pending confirm requests. The head is the dialog currently
	// shown. Concurrent confirm() calls enqueue instead of overwriting a single
	// resolver — the previous single-slot design orphaned every promise but the
	// last (its await never settled) and silently dropped earlier dialogs.
	const queue: Array<{ options: ConfirmOptions; resolve: (value: boolean) => void }> = [];

	/** Render the dialog at the head of the queue, or hide it when empty. */
	function showHead() {
		const head = queue[0];
		if (!head) {
			setDialogState(null);
			return;
		}
		setDialogState({
			title: head.options.title,
			message: head.options.message,
			confirmLabel: head.options.okLabel || "OK",
			cancelLabel: head.options.cancelLabel || "Cancel",
			kind: head.options.kind || "warning",
			autoCancelMs: head.options.autoCancelMs,
		});
	}

	/** Show a confirmation dialog — resolves true on confirm, false on cancel.
	 *  When a dialog is already visible, this one queues and shows after it. */
	function confirm(options: ConfirmOptions): Promise<boolean> {
		return new Promise<boolean>((resolve) => {
			queue.push({ options, resolve });
			if (queue.length === 1) showHead();
		});
	}

	/** Resolve the current dialog with `value` and advance to the next queued one. */
	function settle(value: boolean) {
		const head = queue.shift();
		head?.resolve(value);
		showHead();
	}

	/** Called when user confirms */
	function handleConfirm() {
		settle(true);
	}

	/** Called when user cancels (button, Escape, or overlay click) */
	function handleClose() {
		settle(false);
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

	/** Confirm force-removing a worktree that is locked by an active agent.
	 *
	 *  Pass `deleteBranch=true` when the branch ref will also be force-deleted —
	 *  the dialog then warns that any unmerged/unpushed commits will be
	 *  destroyed along with the worktree (force-remove uses `git branch -D`).
	 */
	async function confirmRemoveLockedWorktree(branchName: string, deleteBranch: boolean = true): Promise<boolean> {
		const branchWarning = deleteBranch
			? `\n\nThe branch "${branchName}" will be force-deleted (\`git branch -D\`). Any unmerged or unpushed commits will be permanently lost.`
			: "";
		return await confirm({
			title: "Worktree is locked by an agent",
			message: `"${branchName}" is currently locked by an active Claude agent.\n\nForce-removing it may interrupt the agent mid-task.${branchWarning}\n\nContinue anyway?`,
			okLabel: "Force Remove",
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

	/** Confirm stashing changes before switching branch */
	async function confirmStashAndSwitch(branchName: string): Promise<boolean> {
		return await confirm({
			title: "Uncommitted changes",
			message: `Working tree has uncommitted changes.\nStash them and switch to ${branchName}?`,
			okLabel: "Stash & Switch",
			cancelLabel: "Cancel",
			kind: "warning",
		});
	}

	/** Report a git operation failure in a dialog, showing the full git output
	 *  (not a truncated status-line message). When `offerRetry` is true the
	 *  primary button reads "Retry" and the promise resolves true if the user
	 *  chose it; otherwise it's a plain acknowledge dialog. */
	async function reportGitError(title: string, detail: string, offerRetry = false): Promise<boolean> {
		return await confirm({
			title,
			message: detail,
			okLabel: offerRetry ? "Retry" : "OK",
			cancelLabel: offerRetry ? "Cancel" : "Dismiss",
			kind: "error",
		});
	}

	/** Confirm removing orphaned worktrees (detached-HEAD, branch deleted) */
	async function confirmOrphanCleanup(paths: string[]): Promise<boolean> {
		const list = paths.map((p) => `  • ${p}`).join("\n");
		return await confirm({
			title: "Orphaned worktrees found",
			message: `${paths.length} worktree(s) have no branch and will be removed:\n${list}`,
			okLabel: "Remove",
			cancelLabel: "Keep",
			kind: "warning",
		});
	}

	return {
		confirm,
		confirmRemoveWorktree,
		confirmRemoveLockedWorktree,
		confirmCloseTerminal,
		confirmRemoveRepo,
		confirmStashAndSwitch,
		confirmOrphanCleanup,
		reportGitError,
		/** Reactive state for rendering the dialog — null when hidden */
		dialogState,
		/** Handler for confirm button / Enter key */
		handleConfirm,
		/** Handler for cancel button / Escape key / overlay click */
		handleClose,
	};
}
