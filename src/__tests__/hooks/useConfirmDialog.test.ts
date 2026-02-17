import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { useConfirmDialog } from "../../hooks/useConfirmDialog";

const mockAsk = ask as unknown as ReturnType<typeof vi.fn>;
const mockMessage = message as unknown as ReturnType<typeof vi.fn>;

describe("useConfirmDialog", () => {
  let dialog: ReturnType<typeof useConfirmDialog>;

  beforeEach(() => {
    mockAsk.mockReset();
    mockMessage.mockReset();
    dialog = useConfirmDialog();
  });

  describe("confirm()", () => {
    it("calls ask() with correct options and returns true", async () => {
      mockAsk.mockResolvedValueOnce(true);

      const result = await dialog.confirm({
        title: "Delete?",
        message: "Are you sure?",
        okLabel: "Yes",
        cancelLabel: "No",
        kind: "warning",
      });

      expect(result).toBe(true);
      expect(mockAsk).toHaveBeenCalledWith("Are you sure?", {
        title: "Delete?",
        okLabel: "Yes",
        cancelLabel: "No",
        kind: "warning",
      });
    });

    it("returns false when user cancels", async () => {
      mockAsk.mockResolvedValueOnce(false);

      const result = await dialog.confirm({
        title: "Delete?",
        message: "Are you sure?",
      });

      expect(result).toBe(false);
    });

    it("uses default okLabel, cancelLabel, and kind when not specified", async () => {
      mockAsk.mockResolvedValueOnce(true);

      await dialog.confirm({
        title: "Confirm",
        message: "Proceed?",
      });

      expect(mockAsk).toHaveBeenCalledWith("Proceed?", {
        title: "Confirm",
        okLabel: "OK",
        cancelLabel: "Cancel",
        kind: "warning",
      });
    });
  });

  describe("info()", () => {
    it("calls message() with kind:info", async () => {
      mockMessage.mockResolvedValueOnce(undefined);

      await dialog.info("Info Title", "Some information");

      expect(mockMessage).toHaveBeenCalledWith("Some information", {
        title: "Info Title",
        kind: "info",
      });
    });
  });

  describe("error()", () => {
    it("calls message() with kind:error", async () => {
      mockMessage.mockResolvedValueOnce(undefined);

      await dialog.error("Error Title", "Something went wrong");

      expect(mockMessage).toHaveBeenCalledWith("Something went wrong", {
        title: "Error Title",
        kind: "error",
      });
    });
  });

  describe("confirmRemoveWorktree()", () => {
    it("calls ask with correct message for the branch name", async () => {
      mockAsk.mockResolvedValueOnce(true);

      const result = await dialog.confirmRemoveWorktree("feature-x");

      expect(result).toBe(true);
      expect(mockAsk).toHaveBeenCalledWith(
        "Remove feature-x?\nThis deletes the worktree directory and its local branch.",
        {
          title: "Remove worktree?",
          okLabel: "Remove",
          cancelLabel: "Cancel",
          kind: "warning",
        }
      );
    });

    it("returns false when user declines", async () => {
      mockAsk.mockResolvedValueOnce(false);
      const result = await dialog.confirmRemoveWorktree("feature-y");
      expect(result).toBe(false);
    });
  });

  describe("confirmCloseTerminal()", () => {
    it("calls ask with correct message for terminal name", async () => {
      mockAsk.mockResolvedValueOnce(true);

      const result = await dialog.confirmCloseTerminal("Terminal 1");

      expect(result).toBe(true);
      expect(mockAsk).toHaveBeenCalledWith(
        "Close Terminal 1?\nAny running processes will be terminated.",
        {
          title: "Close terminal?",
          okLabel: "Close",
          cancelLabel: "Cancel",
          kind: "warning",
        }
      );
    });

    it("returns false when user declines", async () => {
      mockAsk.mockResolvedValueOnce(false);
      const result = await dialog.confirmCloseTerminal("Terminal 2");
      expect(result).toBe(false);
    });
  });

  describe("confirmRemoveRepo()", () => {
    it("calls ask with correct message for repo name", async () => {
      mockAsk.mockResolvedValueOnce(true);

      const result = await dialog.confirmRemoveRepo("my-repo");

      expect(result).toBe(true);
      expect(mockAsk).toHaveBeenCalledWith(
        "Remove my-repo from the list?\nThis does not delete any files.",
        {
          title: "Remove repository?",
          okLabel: "Remove",
          cancelLabel: "Cancel",
          kind: "warning",
        }
      );
    });

    it("returns false when user declines", async () => {
      mockAsk.mockResolvedValueOnce(false);
      const result = await dialog.confirmRemoveRepo("other-repo");
      expect(result).toBe(false);
    });
  });
});
