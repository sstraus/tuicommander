import { beforeEach, describe, expect, it } from "vitest";
import { useConfirmDialog } from "../../hooks/useConfirmDialog";

describe("useConfirmDialog", () => {
	let dialog: ReturnType<typeof useConfirmDialog>;

	beforeEach(() => {
		dialog = useConfirmDialog();
	});

	describe("confirm()", () => {
		it("sets dialogState when called and resolves true on confirm", async () => {
			expect(dialog.dialogState()).toBe(null);

			const promise = dialog.confirm({
				title: "Delete?",
				message: "Are you sure?",
				okLabel: "Yes",
				cancelLabel: "No",
				kind: "warning",
			});

			expect(dialog.dialogState()).toEqual({
				title: "Delete?",
				message: "Are you sure?",
				confirmLabel: "Yes",
				cancelLabel: "No",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			const result = await promise;

			expect(result).toBe(true);
			expect(dialog.dialogState()).toBe(null);
		});

		it("resolves false on close", async () => {
			const promise = dialog.confirm({
				title: "Delete?",
				message: "Are you sure?",
			});

			dialog.handleClose();
			const result = await promise;

			expect(result).toBe(false);
			expect(dialog.dialogState()).toBe(null);
		});

		it("uses default okLabel, cancelLabel, and kind when not specified", async () => {
			const promise = dialog.confirm({
				title: "Confirm",
				message: "Proceed?",
			});

			expect(dialog.dialogState()).toEqual({
				title: "Confirm",
				message: "Proceed?",
				confirmLabel: "OK",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleClose();
			await promise;
		});

		it("threads autoCancelMs into dialogState when provided", async () => {
			const promise = dialog.confirm({
				title: "Switch to new worktree?",
				message: "Switch now?",
				cancelLabel: "Stay",
				kind: "info",
				autoCancelMs: 10_000,
			});

			expect(dialog.dialogState()?.autoCancelMs).toBe(10_000);

			dialog.handleClose();
			await promise;
		});

		it("leaves autoCancelMs undefined when not provided", async () => {
			const promise = dialog.confirm({ title: "Confirm", message: "Proceed?" });

			expect(dialog.dialogState()?.autoCancelMs).toBeUndefined();

			dialog.handleClose();
			await promise;
		});
	});

	describe("confirmOrphanCleanup()", () => {
		it("counts down only when every backend assessment is safe", async () => {
			const pending = dialog.confirmOrphanCleanup("/repo", [{ path: "/wt/clean", safe: true }], 10);
			expect(dialog.dialogState()?.autoConfirmMs).toBe(10_000);
			expect(dialog.dialogState()?.message).toContain("/wt/clean");
			dialog.handleClose();
			expect(await pending).toBe(false);
		});

		it("names the dirty orphan and runs no countdown", async () => {
			const pending = dialog.confirmOrphanCleanup(
				"/repo",
				[
					{ path: "/wt/clean", safe: true },
					{ path: "/wt/dirty", safe: false, reason: "untracked files" },
				],
				10,
			);
			expect(dialog.dialogState()?.autoConfirmMs).toBeUndefined();
			expect(dialog.dialogState()?.message).toContain("/wt/dirty: untracked files");
			dialog.handleClose();
			expect(await pending).toBe(false);
		});

		// Catches: a live-session orphan shown with a countdown or without naming the session.
		it("names the live session and runs no countdown", async () => {
			const pending = dialog.confirmOrphanCleanup(
				"/repo",
				[
					{
						path: "/wt/busy",
						safe: false,
						reason: "live session: Claude: refactor",
						live_sessions: [{ session_id: "s1", name: "Claude: refactor" }],
					},
				],
				10,
			);
			expect(dialog.dialogState()?.autoConfirmMs).toBeUndefined();
			expect(dialog.dialogState()?.message).toContain("/wt/busy: live session: Claude: refactor");
			dialog.handleClose();
			expect(await pending).toBe(false);
		});

		it("an agent answer settles the matching queued orphan prompt", async () => {
			const unrelated = dialog.confirm({ title: "First", message: "Unrelated" });
			const orphan = dialog.confirmOrphanCleanup("/repo", [{ path: "/wt/clean", safe: true }], 10);
			dialog.answerOrphanCleanup("/repo", false);
			expect(await orphan).toBe(false);
			expect(dialog.dialogState()?.title).toBe("First");
			dialog.handleClose();
			expect(await unrelated).toBe(false);
		});
	});

	describe("concurrent confirm() calls", () => {
		it("queues a second confirm and shows dialogs sequentially (FIFO)", async () => {
			const first = dialog.confirm({ title: "First", message: "1?" });
			const second = dialog.confirm({ title: "Second", message: "2?" });

			// Only the first is shown; the second waits in the queue.
			expect(dialog.dialogState()?.title).toBe("First");

			dialog.handleConfirm();
			expect(await first).toBe(true);

			// Resolving the first advances to the queued second.
			expect(dialog.dialogState()?.title).toBe("Second");

			dialog.handleClose();
			expect(await second).toBe(false);
			expect(dialog.dialogState()).toBe(null);
		});

		it("does not orphan the first promise when a second confirm arrives", async () => {
			// Regression: the old single-slot pendingResolve was overwritten by the
			// second confirm(), so the first promise never settled. If that bug
			// returns, `await first` below hangs and the test fails via timeout.
			const first = dialog.confirm({ title: "First", message: "1?" });
			const second = dialog.confirm({ title: "Second", message: "2?" });

			dialog.handleConfirm(); // settle the head (first)
			expect(await first).toBe(true);

			// Drain the queued second so the test leaves no pending promise.
			dialog.handleClose();
			expect(await second).toBe(false);
		});

		it("resolves all three queued confirms in order", async () => {
			const results: boolean[] = [];
			const a = dialog.confirm({ title: "A", message: "?" }).then((v) => results.push(v));
			const b = dialog.confirm({ title: "B", message: "?" }).then((v) => results.push(v));
			const c = dialog.confirm({ title: "C", message: "?" }).then((v) => results.push(v));

			expect(dialog.dialogState()?.title).toBe("A");
			dialog.handleConfirm(); // A -> true
			await a;
			expect(dialog.dialogState()?.title).toBe("B");
			dialog.handleClose(); // B -> false
			await b;
			expect(dialog.dialogState()?.title).toBe("C");
			dialog.handleConfirm(); // C -> true
			await c;

			expect(results).toEqual([true, false, true]);
			expect(dialog.dialogState()).toBe(null);
		});
	});

	describe("confirmRemoveWorktree()", () => {
		it("distinguishes unstarted work from merged commits before confirmation", async () => {
			const pending = dialog.confirmRemoveWorktree(
				"unstarted",
				{ dirtyFiles: 0, commitStatus: "in_sync", removalSafety: "safe" },
				true,
			);
			expect(dialog.dialogState()?.message).toContain("nothing of its own, not merged work");
			dialog.handleClose();
			expect(await pending).toBe(false);

			const merged = dialog.confirmRemoveWorktree(
				"completed",
				{ dirtyFiles: 0, commitStatus: "merged", removalSafety: "safe" },
				true,
			);
			expect(dialog.dialogState()?.message).toContain("commits are in the default branch");
			dialog.handleClose();
			expect(await merged).toBe(false);
		});

		it("names live sessions and counts untracked files before removing", async () => {
			const pending = dialog.confirmRemoveWorktree(
				"active",
				{
					dirtyFiles: 4,
					untrackedFiles: 2,
					liveSessions: [{ sessionId: "pty-1", name: "Codex: gate work" }],
					commitStatus: "in_sync",
					removalSafety: "requires_force",
				},
				true,
			);
			expect(dialog.dialogState()?.message).toContain("Codex: gate work");
			expect(dialog.dialogState()?.message).toContain("4 uncommitted files");
			expect(dialog.dialogState()?.message).toContain("2 untracked files");
			dialog.handleConfirm();
			expect(await pending).toBe(true);
		});
		it("identifies a missing checkout and offers cancellation before pruning its registration", async () => {
			const promise = dialog.confirmRemoveWorktree(
				"feature-missing",
				{ dirtyFiles: null, missingCheckout: true, commitStatus: "in_sync", removalSafety: "requires_force" },
				false,
			);
			expect(dialog.dialogState()?.message).toContain("checkout directory is missing");
			expect(dialog.dialogState()?.message).toContain("submodule refs will be preserved");
			expect(dialog.dialogState()?.message).not.toContain("Working tree: clean");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
		it("shows dialog with correct message and resolves true on confirm", async () => {
			const promise = dialog.confirmRemoveWorktree(
				"feature-x",
				{
					dirtyFiles: 0,
					commitStatus: "unmerged",
					removalSafety: "safe",
				},
				true,
			);

			expect(dialog.dialogState()).toEqual({
				title: "Remove workspace?",
				message:
					'Remove "feature-x"?\n\nWorking tree: clean.\nCommit state: commits remain in the parent repository.\nGit will safely delete the local branch; if it is unmerged, the branch is kept.',
				confirmLabel: "Remove",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmRemoveWorktree(
				"feature-y",
				{
					dirtyFiles: 3,
					submoduleUnpushedCommits: [{ path: "plugins", count: 7 }],
					commitStatus: "unmerged",
					removalSafety: "requires_force",
				},
				true,
			);
			expect(dialog.dialogState()?.title).toBe("Destroy workspace state?");
			// The count, not the adjective: "dirty" never told the user what a
			// removal costs, and this dialog is the last stop before it happens.
			expect(dialog.dialogState()?.message).toContain("3 uncommitted files will be discarded");
			expect(dialog.dialogState()?.message).toContain("plugins: 7 commits not on a remote-tracking branch");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmDirtyWorktreeCleanup()", () => {
		it("names a live agent before archiving a clean worktree", async () => {
			const pending = dialog.confirmDirtyWorktreeCleanup("active-feature", "archive", 1, {
				dirtyFiles: 0,
				dirtyFingerprint: "confirmed-clean",
				commitStatus: "unmerged",
				removalSafety: "safe",
				liveSessions: [{ sessionId: "pty-active", name: "Codex: gate work" }],
				warnings: ["Live session: Codex: gate work"],
			});

			try {
				expect(dialog.dialogState()?.message).toContain("Codex: gate work");
				expect(dialog.dialogState()?.message).not.toContain("has uncommitted changes");
			} finally {
				dialog.handleClose();
				expect(await pending).toBe(false);
			}
		});
	});

	describe("confirmCloseTerminal()", () => {
		it("shows dialog with correct message for terminal name", async () => {
			const promise = dialog.confirmCloseTerminal("Terminal 1");

			expect(dialog.dialogState()).toEqual({
				title: "Close terminal?",
				message: "Close Terminal 1?\nAny running processes will be terminated.",
				confirmLabel: "Close",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmCloseTerminal("Terminal 2");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmRemoveRepo()", () => {
		it("shows dialog with correct message for repo name", async () => {
			const promise = dialog.confirmRemoveRepo("my-repo");

			expect(dialog.dialogState()).toEqual({
				title: "Remove repository?",
				message: "Remove my-repo from the list?\nThis does not delete any files.",
				confirmLabel: "Remove",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmRemoveRepo("other-repo");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmStashAndSwitch()", () => {
		it("shows dialog with correct message and resolves true on confirm", async () => {
			const promise = dialog.confirmStashAndSwitch("main");

			expect(dialog.dialogState()).toEqual({
				title: "Uncommitted changes",
				message: "Working tree has uncommitted changes.\nStash them and switch to main?",
				confirmLabel: "Stash & Switch",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmStashAndSwitch("main");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("reportGitError()", () => {
		it("shows OK/Dismiss labels by default", async () => {
			const promise = dialog.reportGitError("Commit failed", "fatal: nothing to commit");

			expect(dialog.dialogState()).toEqual({
				title: "Commit failed",
				message: "fatal: nothing to commit",
				confirmLabel: "OK",
				cancelLabel: "Dismiss",
				kind: "error",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("shows Retry/Cancel labels and resolves true only when Retry is chosen", async () => {
			const promise = dialog.reportGitError("Push failed", "fatal: connection reset", true);

			const state = dialog.dialogState();
			expect(state?.confirmLabel).toBe("Retry");
			expect(state?.cancelLabel).toBe("Cancel");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("resolves false when the acknowledge dialog is dismissed", async () => {
			const promise = dialog.reportGitError("Commit failed", "fatal: nothing to commit");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmSaveChanges()", () => {
		it("offers Save / Don't Save / Cancel with Save as the Enter default", () => {
			void dialog.confirmSaveChanges("notes.md");

			expect(dialog.dialogState()).toEqual({
				title: "Unsaved changes",
				message: '"notes.md" has unsaved changes.\nDo you want to save your changes before closing?',
				confirmLabel: "Save",
				cancelLabel: "Cancel",
				discardLabel: "Don't Save",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleClose();
		});

		it("resolves 'confirm' when the user chooses Save", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleConfirm();
			expect(await promise).toBe("confirm");
		});

		it("resolves 'discard' when the user chooses Don't Save", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleDiscard();
			expect(await promise).toBe("discard");
		});

		it("resolves 'cancel' when the user cancels", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleClose();
			expect(await promise).toBe("cancel");
		});
	});
});
