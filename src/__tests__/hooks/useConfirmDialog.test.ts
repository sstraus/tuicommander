import { describe, it, expect, beforeEach } from "vitest";
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
