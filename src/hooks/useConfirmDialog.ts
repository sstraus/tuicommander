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
		/** Reactive state for rendering the dialog — null when hidden */
		dialogState,
		/** Handler for confirm button / Enter key */
		handleConfirm,
		/** Handler for cancel button / Escape key / overlay click */
		handleClose,
	};
}
