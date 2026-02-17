import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { RenameBranchDialog, validateBranchName } from "../../components/RenameBranchDialog/RenameBranchDialog";

describe("validateBranchName", () => {
  it("returns error for empty name", () => {
    expect(validateBranchName("")).toBe("Branch name cannot be empty");
    expect(validateBranchName("  ")).toBe("Branch name cannot be empty");
  });

  it("returns error for name with spaces", () => {
    expect(validateBranchName("my branch")).toBe("Branch name cannot contain spaces");
  });

  it("returns error for name starting with hyphen", () => {
    expect(validateBranchName("-feature")).toBe("Branch name cannot start with a hyphen");
  });

  it("returns error for name containing '..'", () => {
    expect(validateBranchName("feat..fix")).toBe("Branch name cannot contain '..'");
  });

  it("returns error for name ending with '.lock'", () => {
    expect(validateBranchName("main.lock")).toBe("Branch name cannot end with '.lock'");
  });

  it("returns error for invalid characters", () => {
    expect(validateBranchName("feat~1")).toBe("Branch name contains invalid characters");
    expect(validateBranchName("feat^1")).toBe("Branch name contains invalid characters");
    expect(validateBranchName("feat:fix")).toBe("Branch name contains invalid characters");
  });

  it("returns error for invalid slash usage", () => {
    expect(validateBranchName("/feat")).toBe("Invalid slash usage in branch name");
    expect(validateBranchName("feat/")).toBe("Invalid slash usage in branch name");
    expect(validateBranchName("feat//fix")).toBe("Invalid slash usage in branch name");
  });

  it("returns null for valid branch names", () => {
    expect(validateBranchName("feature/my-branch")).toBeNull();
    expect(validateBranchName("main")).toBeNull();
    expect(validateBranchName("fix-123")).toBeNull();
  });
});

describe("RenameBranchDialog", () => {
  it("renders dialog when visible", () => {
    const { container } = render(() => (
      <RenameBranchDialog
        visible={true}
        currentName="main"
        onClose={() => {}}
        onRename={async () => {}}
      />
    ));
    const dialog = container.querySelector(".branch-popover");
    expect(dialog).not.toBeNull();
    const heading = container.querySelector("h4");
    expect(heading!.textContent).toBe("Rename Branch");
  });

  it("returns null when not visible", () => {
    const { container } = render(() => (
      <RenameBranchDialog
        visible={false}
        currentName="main"
        onClose={() => {}}
        onRename={async () => {}}
      />
    ));
    const dialog = container.querySelector(".branch-popover");
    expect(dialog).toBeNull();
  });

  it("renders input with current branch name", () => {
    const { container } = render(() => (
      <RenameBranchDialog
        visible={true}
        currentName="feature-branch"
        onClose={() => {}}
        onRename={async () => {}}
      />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input).not.toBeNull();
    expect(input.value).toBe("feature-branch");
  });

  it("shows validation error for invalid branch name", async () => {
    const { container } = render(() => (
      <RenameBranchDialog
        visible={true}
        currentName="main"
        onClose={() => {}}
        onRename={async () => {}}
      />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    // Enter invalid branch name with spaces
    fireEvent.input(input, { target: { value: "bad name" } });

    // Click the rename button
    const renameBtn = container.querySelector(".branch-popover-rename")!;
    fireEvent.click(renameBtn);

    // Check for validation error
    const error = container.querySelector(".branch-popover-error");
    expect(error).not.toBeNull();
    expect(error!.textContent).toBe("Branch name cannot contain spaces");
  });

  it("calls onRename with old and new name on valid submission", async () => {
    const handleRename = vi.fn().mockResolvedValue(undefined);
    const handleClose = vi.fn();
    const { container } = render(() => (
      <RenameBranchDialog
        visible={true}
        currentName="old-branch"
        onClose={handleClose}
        onRename={handleRename}
      />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "new-branch" } });

    const renameBtn = container.querySelector(".branch-popover-rename")!;
    fireEvent.click(renameBtn);

    // Wait for the async onRename to complete
    await vi.waitFor(() => {
      expect(handleRename).toHaveBeenCalledWith("old-branch", "new-branch");
    });
  });

  it("calls onClose when cancel button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <RenameBranchDialog
        visible={true}
        currentName="main"
        onClose={handleClose}
        onRename={async () => {}}
      />
    ));
    const cancelBtn = container.querySelector(".branch-popover-cancel")!;
    fireEvent.click(cancelBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });
});
