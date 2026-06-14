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
		it("shows dialog with correct message and resolves true on confirm", async () => {
			const promise = dialog.confirmRemoveWorktree("feature-x");

			expect(dialog.dialogState()).toEqual({
				title: "Remove worktree?",
				message: "Remove feature-x?\nThis deletes the worktree directory and its local branch.",
				confirmLabel: "Remove",
				cancelLabel: "Cancel",
				kind: "warning",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmRemoveWorktree("feature-y");
			dialog.handleClose();
			expect(await promise).toBe(false);
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
});
