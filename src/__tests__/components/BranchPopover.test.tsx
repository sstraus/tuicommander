import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";

const mockRenameBranch = vi.fn().mockResolvedValue(undefined);

vi.mock("../../hooks/useRepository", () => ({
  useRepository: () => ({
    renameBranch: mockRenameBranch,
  }),
}));

import { BranchPopover } from "../../components/BranchPopover/BranchPopover";

describe("BranchPopover", () => {
  const defaultProps = {
    branch: "feature/my-branch",
    repoPath: "/repo" as string | null,
    onClose: vi.fn(),
    onBranchRenamed: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockRenameBranch.mockResolvedValue(undefined);
  });

  it("renders with branch name in input", () => {
    const { container } = render(() => <BranchPopover {...defaultProps} />);
    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input).not.toBeNull();
    expect(input.value).toBe("feature/my-branch");
  });

  it("shows 'Rename Branch' header", () => {
    const { container } = render(() => <BranchPopover {...defaultProps} />);
    const header = container.querySelector(".header h4");
    expect(header).not.toBeNull();
    expect(header!.textContent).toBe("Rename Branch");
  });

  it("cancel button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <BranchPopover {...defaultProps} onClose={onClose} />
    ));
    const cancelBtn = container.querySelector(".cancelBtn")!;
    fireEvent.click(cancelBtn);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("empty branch name shows error", async () => {
    const { container } = render(() => <BranchPopover {...defaultProps} />);
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    // Clear the input
    fireEvent.input(input, { target: { value: "" } });

    // The rename button is disabled when empty, but pressing Enter
    // triggers handleRename via the document keydown listener
    fireEvent.keyDown(document, { key: "Enter" });

    await waitFor(() => {
      const error = container.querySelector(".error");
      expect(error).not.toBeNull();
      expect(error!.textContent).toBe("Branch name cannot be empty");
    });
  });

  it("invalid characters show error", async () => {
    const { container } = render(() => <BranchPopover {...defaultProps} />);
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "bad branch name!@#" } });

    const renameBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      const error = container.querySelector(".error");
      expect(error).not.toBeNull();
      expect(error!.textContent).toBe("Invalid characters in branch name");
    });
  });

  it("protected branch (main) shows error", async () => {
    const { container } = render(() => (
      <BranchPopover {...defaultProps} branch="main" />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    // Enter a different valid name to trigger rename logic
    fireEvent.input(input, { target: { value: "new-name" } });

    const renameBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      const error = container.querySelector(".error");
      expect(error).not.toBeNull();
      expect(error!.textContent).toBe("Cannot rename protected branch");
    });
  });

  it("protected branch (master) shows error", async () => {
    const { container } = render(() => (
      <BranchPopover {...defaultProps} branch="master" />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "new-name" } });

    const renameBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      const error = container.querySelector(".error");
      expect(error).not.toBeNull();
      expect(error!.textContent).toBe("Cannot rename protected branch");
    });
  });

  it("no repoPath shows error", async () => {
    const { container } = render(() => (
      <BranchPopover {...defaultProps} repoPath={null} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "valid-name" } });

    const renameBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      const error = container.querySelector(".error");
      expect(error).not.toBeNull();
      expect(error!.textContent).toBe("No repository selected");
    });
  });

  it("successful rename calls onBranchRenamed and onClose", async () => {
    const onClose = vi.fn();
    const onBranchRenamed = vi.fn();
    const { container } = render(() => (
      <BranchPopover
        {...defaultProps}
        onClose={onClose}
        onBranchRenamed={onBranchRenamed}
      />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "renamed-branch" } });

    const renameBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      expect(mockRenameBranch).toHaveBeenCalledWith("/repo", "feature/my-branch", "renamed-branch");
      expect(onBranchRenamed).toHaveBeenCalledWith("feature/my-branch", "renamed-branch");
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("overlay click calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <BranchPopover {...defaultProps} onClose={onClose} />
    ));
    const overlay = container.querySelector(".overlay")!;
    fireEvent.click(overlay);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
