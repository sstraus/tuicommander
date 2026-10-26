import { createSignal } from "solid-js";
import type { OrphanAssessment } from "./git/createRepositoryRefreshCoordinator";

/** Which button the Enter key activates. Defaults to "confirm" for backward
 *  compatibility; destructive dialogs can set "cancel" so an accidental Enter
 *  takes the safe path instead of the primary (destructive) action. */
export type ConfirmDefaultButton = "confirm" | "cancel";

/** Outcome of a confirm dialog. "discard" is only reachable when a third
 *  (discard) button is configured, e.g. the Save / Don't Save / Cancel prompt. */
export type ConfirmResult = "confirm" | "cancel" | "discard";

export interface ConfirmOptions {
	title: string;
	message: string;
	okLabel?: string;
	cancelLabel?: string;
	/** Optional middle button (e.g. "Don't Save"). When set the dialog offers a
	 *  third outcome ("discard") between confirm and cancel. */
	discardLabel?: string;
	kind?: "info" | "warning" | "error";
	/** Which button Enter activates. Defaults to "confirm". */
	defaultButton?: ConfirmDefaultButton;
	/** When set, the dialog auto-clicks cancel after this many ms, with a countdown on the cancel label. */
	autoCancelMs?: number;
	/** When set, the dialog auto-clicks confirm after this many ms, with a countdown on the confirm label. */
	autoConfirmMs?: number;
}

/** Internal state for the currently visible confirm dialog */
export interface ConfirmDialogState {
	title: string;
	message: string;
	confirmLabel: string;
	cancelLabel: string;
	discardLabel?: string;
	kind: "info" | "warning" | "error";
	defaultButton: ConfirmDefaultButton;
	autoCancelMs?: number;
	autoConfirmMs?: number;
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
	const queue: Array<{ options: ConfirmOptions; resolve: (value: ConfirmResult) => void; key?: string }> = [];

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
			discardLabel: head.options.discardLabel,
			kind: head.options.kind || "warning",
			defaultButton: head.options.defaultButton || "confirm",
			autoCancelMs: head.options.autoCancelMs,
			autoConfirmMs: head.options.autoConfirmMs,
		});
	}

	/** Show a confirmation dialog — resolves true on confirm, false on cancel.
	 *  When a dialog is already visible, this one queues and shows after it. */
	function confirm(options: ConfirmOptions, key?: string): Promise<boolean> {
		return new Promise<boolean>((resolve) => {
			queue.push({ options, resolve: (value) => resolve(value === "confirm"), key });
			if (queue.length === 1) showHead();
		});
	}

	/** Resolve the current dialog with `value` and advance to the next queued one. */
	function settle(value: ConfirmResult) {
		const head = queue.shift();
		head?.resolve(value);
		showHead();
	}

	function answerOrphanCleanup(repoPath: string, remove: boolean) {
		const index = queue.findIndex((request) => request.key === `orphan:${repoPath}`);
		if (index < 0) return;
		const [request] = queue.splice(index, 1);
		request.resolve(remove ? "confirm" : "cancel");
		if (index === 0) showHead();
	}

	/** Called when user confirms */
	function handleConfirm() {
		settle("confirm");
	}

	/** Called when user cancels (button, Escape, or overlay click) */
	function handleClose() {
		settle("cancel");
	}

	/** Called when the user picks the middle "discard" action (e.g. Don't Save) */
	function handleDiscard() {
		settle("discard");
	}

	/** Confirm removing a linked worktree. */
	async function confirmRemoveWorktree(
		branchName: string,
		status: import("../stores/workspaceIdentity").WorkspaceLifecycleStatus,
		deleteBranch: boolean,
	): Promise<boolean> {
		const lost = status.dirtyFiles ?? 0;
		const missing = status.missingCheckout === true;
		const dirty = lost > 0 ? `${lost} uncommitted file${lost === 1 ? "" : "s"} will be discarded` : "clean";
		const commits =
			status.commitStatus === "merged"
				? "This branch's commits are in the default branch"
				: status.commitStatus === "in_sync"
					? "This branch has nothing of its own, not merged work"
					: status.commitStatus === "pushed_unmerged"
						? "This branch is not merged, but all its commits are pushed"
						: "commits remain in the parent repository";
		const untracked = status.untrackedFiles
			? `\nUntracked files: ${status.untrackedFiles} untracked files will be discarded.`
			: "";
		const sessions = status.liveSessions?.length
			? `\nLive sessions in this worktree: ${status.liveSessions.map((session) => session.name).join(", ")}. Removing it will interrupt them.`
			: "";
		// Mirrors `retain_finished_checkout` (src-tauri/src/worktree.rs): a clean, integrated
		// checkout is archived for reuse, so the branch stays checked out there.
		const recycled =
			deleteBranch &&
			status.removalSafety === "safe" &&
			!missing &&
			status.dirtyFiles === 0 &&
			!(status.submoduleUnpushedCommits ?? []).some((entry) => entry.count > 0) &&
			(status.commitStatus === "merged" || status.commitStatus === "in_sync");
		const branchAction =
			deleteBranch && !recycled
				? "Git will safely delete the local branch; if it is unmerged, the branch is kept."
				: "The local branch will be kept.";
		const destructive = status.removalSafety === "requires_force";
		const submoduleCommits = (status.submoduleUnpushedCommits ?? [])
			.filter((entry) => entry.count > 0)
			.map((entry) => `${entry.path}: ${entry.count} commits not on a remote-tracking branch`);
		return await confirm({
			title: destructive ? "Destroy workspace state?" : recycled ? "Archive workspace?" : "Remove workspace?",
			message: `${recycled ? "Archive" : "Remove"} "${branchName}"?\n\n${recycled ? "The clean checkout moves to the archive area for reuse; it is not deleted.\n" : ""}${missing ? "The checkout directory is missing; Git still has a worktree registration. Its submodule refs will be preserved before cleanup." : `Working tree: ${dirty}.`}${untracked}${sessions}${submoduleCommits.length ? `\nSubmodules: ${submoduleCommits.join("; ")}.` : ""}\nCommit state: ${commits}.\n${branchAction}`,
			okLabel: destructive ? "Delete anyway" : recycled ? "Archive" : "Remove",
			cancelLabel: "Cancel",
			kind: destructive ? "error" : "warning",
		});
	}

	/** Confirm overriding an agent's worktree lock. */
	async function confirmRemoveLockedWorktree(branchName: string, deleteBranch: boolean = true): Promise<boolean> {
		const branchWarning = deleteBranch
			? `\n\nThe branch "${branchName}" will be deleted only if its commits are already integrated. Otherwise it will be kept and the result will explain why.`
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

	/** Prompt to save unsaved editor changes before closing a tab.
	 *  Enter defaults to Save (a safe, non-destructive action) and Escape cancels,
	 *  so an accidental keypress never discards the user's changes.
	 *  Resolves "confirm" = save then close, "discard" = close without saving,
	 *  "cancel" = keep the tab open. */
	async function confirmSaveChanges(fileName: string): Promise<ConfirmResult> {
		return new Promise<ConfirmResult>((resolve) => {
			queue.push({
				options: {
					title: "Unsaved changes",
					message: '"' + fileName + '" has unsaved changes.\nDo you want to save your changes before closing?',
					okLabel: "Save",
					discardLabel: "Don't Save",
					cancelLabel: "Cancel",
					kind: "warning",
					defaultButton: "confirm",
				},
				resolve,
			});
			if (queue.length === 1) showHead();
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
	async function confirmOrphanCleanup(
		repoPath: string,
		assessments: OrphanAssessment[],
		countdownSeconds: number,
	): Promise<boolean> {
		const allSafe = assessments.every((entry) => entry.safe);
		const list = assessments.map((entry) => `  • ${entry.path}${entry.reason ? `: ${entry.reason}` : ""}`).join("\n");
		return await confirm(
			{
				title: "Orphaned worktrees found",
				message: allSafe
					? `${assessments.length} clean worktree(s) have no branch and can be removed:\n${list}`
					: `Review before removing these orphaned worktrees:\n${list}`,
				okLabel: "Remove",
				cancelLabel: "Keep",
				kind: "warning",
				defaultButton: "cancel",
				autoConfirmMs: allSafe ? countdownSeconds * 1000 : undefined,
			},
			`orphan:${repoPath}`,
		);
	}

	/** Confirm cleanup hazards before retrying with an explicit fingerprint.
	 *  `commitsAhead` is 0 when the merge itself would also be a no-op. */
	async function confirmDirtyWorktreeCleanup(
		branchName: string,
		action: string,
		commitsAhead: number,
		lifecycle?: import("../stores/workspaceIdentity").WorkspaceLifecycleStatus,
	): Promise<boolean> {
		const verb = action === "delete" ? "Deleting" : "Archiving";
		const dirty = lifecycle?.dirtyFiles;
		const fate = action === "delete" ? "removes the worktree directory" : "moves the worktree to __archived/";
		const consequence =
			dirty === 0
				? `The worktree for "${branchName}" is clean. ${verb} it ${fate}.`
				: `The worktree for "${branchName}" has uncommitted changes. ${verb} it ${fate} — ${action === "delete" ? "the changes are lost" : "the changes move with it"}.`;
		const warnings = lifecycle?.warnings?.length
			? lifecycle.warnings
			: (lifecycle?.liveSessions?.map((session) => `Live session: ${session.name}`) ?? []);
		const noop =
			commitsAhead === 0
				? `\n\n"${branchName}" also has no commits the target branch lacks, so the merge itself would do nothing.`
				: "";
		const submoduleCommits = (lifecycle?.submoduleUnpushedCommits ?? [])
			.filter((entry) => entry.count > 0)
			.map((entry) => `${entry.path}: ${entry.count} commits not on a remote-tracking branch`);
		return await confirm({
			title: dirty === 0 ? "Review worktree cleanup" : "Uncommitted work in the worktree",
			message: `${consequence}${noop}${warnings.length ? `\n\n${warnings.join("\n")}` : ""}${submoduleCommits.length ? `\n\nSubmodules: ${submoduleCommits.join("; ")}.` : ""}\n\nContinue?`,
			okLabel: action === "delete" ? "Delete anyway" : "Archive anyway",
			cancelLabel: "Keep it",
			kind: "warning",
		});
	}

	return {
		confirm,
		confirmSaveChanges,
		confirmRemoveWorktree,
		confirmRemoveLockedWorktree,
		confirmCloseTerminal,
		confirmRemoveRepo,
		confirmStashAndSwitch,
		confirmOrphanCleanup,
		answerOrphanCleanup,
		confirmDirtyWorktreeCleanup,
		reportGitError,
		/** Reactive state for rendering the dialog — null when hidden */
		dialogState,
		/** Handler for confirm button / Enter key */
		handleConfirm,
		/** Handler for cancel button / Escape key / overlay click */
		handleClose,
		/** Handler for the middle discard button (e.g. Don't Save) */
		handleDiscard,
	};
}
