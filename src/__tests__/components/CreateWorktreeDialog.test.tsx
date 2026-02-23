import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { CreateWorktreeDialog } from "../../components/CreateWorktreeDialog/CreateWorktreeDialog";

const defaultProps = {
  visible: true,
  suggestedName: "bold-nexus-042",
  existingBranches: ["main", "develop", "feature/auth", "fix/login-bug"],
  worktreeBranches: ["main"],
  worktreesDir: "/repos/myproject/.worktrees",
  onClose: () => {},
  onCreate: () => {},
};

describe("CreateWorktreeDialog", () => {
  it("renders nothing when not visible", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} visible={false} />
    ));
    expect(container.querySelector(".popover")).toBeNull();
  });

  it("renders dialog with input and branch list when visible", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    expect(container.querySelector(".popover")).not.toBeNull();
    expect(container.querySelector("h4")!.textContent).toBe("New Worktree");

    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input).not.toBeNull();
  });

  it("starts with empty input and shows all non-worktree branches", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input.value).toBe("");

    // Branch list should show all branches
    const items = container.querySelectorAll("[class*='branchItem']");
    expect(items.length).toBe(4); // main, develop, feature/auth, fix/login-bug
  });

  it("shows worktree branches as disabled with suffix", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    // "main" has a worktree — should have disabled styling and "(has worktree)" text
    const items = container.querySelectorAll("[class*='branchItem']");
    const mainItem = Array.from(items).find((el) => el.textContent?.includes("main"));
    expect(mainItem).toBeDefined();
    expect(mainItem!.textContent).toContain("has worktree");
    expect(mainItem!.classList.toString()).toContain("disabled");
  });

  it("filters branch list as user types", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "feat" } });

    const items = container.querySelectorAll("[class*='branchItem']");
    // Only "feature/auth" should match
    expect(items.length).toBe(1);
    expect(items[0].textContent).toContain("feature/auth");
  });

  it("shows 'existing branch' status when input matches an existing branch", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "develop" } });

    const status = container.querySelector("[class*='statusLine']");
    expect(status).not.toBeNull();
    expect(status!.textContent).toContain("existing branch");
  });

  it("shows 'new branch' status when input doesn't match any branch", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "feature/new-thing" } });

    const status = container.querySelector("[class*='statusLine']");
    expect(status).not.toBeNull();
    expect(status!.textContent).toContain("new branch");
  });

  it("shows path preview with sanitized branch name", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "feature/auth" } });

    const path = container.querySelector("[class*='pathPreview']");
    expect(path).not.toBeNull();
    // Slashes in branch names should be replaced in the directory name
    expect(path!.textContent).toContain("/repos/myproject/.worktrees/");
    expect(path!.textContent).toContain("feature");
  });

  it("clicking a non-disabled branch populates input", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const items = container.querySelectorAll("[class*='branchItem']");
    const developItem = Array.from(items).find(
      (el) => el.textContent?.includes("develop") && !el.textContent?.includes("has worktree"),
    );
    expect(developItem).toBeDefined();

    fireEvent.click(developItem!);

    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input.value).toBe("develop");
  });

  it("clicking a disabled (worktree) branch does NOT populate input", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const items = container.querySelectorAll("[class*='branchItem']");
    const mainItem = Array.from(items).find((el) => el.textContent?.includes("has worktree"));
    expect(mainItem).toBeDefined();

    fireEvent.click(mainItem!);

    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input.value).toBe(""); // should remain empty
  });

  it("calls onCreate with createBranch=false for existing branch", () => {
    const handleCreate = vi.fn();
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "develop" } });

    const createBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(createBtn);

    expect(handleCreate).toHaveBeenCalledWith({
      branchName: "develop",
      createBranch: false,
    });
  });

  it("calls onCreate with createBranch=true for new branch", () => {
    const handleCreate = vi.fn();
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "feature/new-thing" } });

    const createBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(createBtn);

    expect(handleCreate).toHaveBeenCalledWith({
      branchName: "feature/new-thing",
      createBranch: true,
    });
  });

  it("disables Create button when input is empty", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const createBtn = container.querySelector(".primaryBtn") as HTMLButtonElement;
    expect(createBtn.disabled).toBe(true);
  });

  it("shows validation error for invalid branch name on create", () => {
    const handleCreate = vi.fn();
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "bad name" } });

    const createBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(createBtn);

    // Should show error, not call onCreate
    const error = container.querySelector(".error");
    expect(error).not.toBeNull();
    expect(handleCreate).not.toHaveBeenCalled();
  });

  it("shows error when trying to create worktree for branch that already has one", () => {
    const handleCreate = vi.fn();
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    // "main" already has a worktree
    fireEvent.input(input, { target: { value: "main" } });

    const createBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(createBtn);

    const error = container.querySelector(".error");
    expect(error).not.toBeNull();
    expect(error!.textContent).toContain("already has a worktree");
    expect(handleCreate).not.toHaveBeenCalled();
  });

  it("calls onClose when cancel button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} onClose={handleClose} />
    ));
    const cancelBtn = container.querySelector(".cancelBtn")!;
    fireEvent.click(cancelBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("no status or path preview shown when input is empty", () => {
    const { container } = render(() => (
      <CreateWorktreeDialog {...defaultProps} />
    ));
    const status = container.querySelector("[class*='statusLine']");
    const path = container.querySelector("[class*='pathPreview']");
    expect(status).toBeNull();
    expect(path).toBeNull();
  });
});
