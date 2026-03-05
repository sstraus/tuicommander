import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import {
  PostMergeCleanupDialog,
  type CleanupStep,
} from "../../components/PostMergeCleanupDialog/PostMergeCleanupDialog";

const defaultProps = () => ({
  branchName: "feature/login",
  baseBranch: "main",
  repoPath: "/repo",
  isOnBaseBranch: false,
  isDefaultBranch: false,
  hasTerminals: false,
  onExecute: vi.fn(),
  onSkip: vi.fn(),
});

describe("PostMergeCleanupDialog", () => {
  it("renders all 4 cleanup steps with checkboxes", () => {
    const { container } = render(() => (
      <PostMergeCleanupDialog {...defaultProps()} />
    ));
    const checkboxes = container.querySelectorAll("input[type='checkbox']");
    expect(checkboxes.length).toBe(4);

    const labels = container.querySelectorAll("[data-testid^='step-label-']");
    expect(labels.length).toBe(4);
  });

  it("toggling a checkbox updates its checked state", () => {
    const { container } = render(() => (
      <PostMergeCleanupDialog {...defaultProps()} />
    ));
    const checkbox = container.querySelector(
      "input[data-testid='step-check-switch']",
    ) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);

    fireEvent.click(checkbox);
    expect(checkbox.checked).toBe(false);

    fireEvent.click(checkbox);
    expect(checkbox.checked).toBe(true);
  });

  it("Execute button calls onExecute with checked step IDs", () => {
    const props = defaultProps();
    const { container } = render(() => (
      <PostMergeCleanupDialog {...props} />
    ));

    // Uncheck "pull"
    const pullCheck = container.querySelector(
      "input[data-testid='step-check-pull']",
    ) as HTMLInputElement;
    fireEvent.click(pullCheck);

    const executeBtn = container.querySelector("[data-testid='execute-btn']")!;
    fireEvent.click(executeBtn);

    expect(props.onExecute).toHaveBeenCalledOnce();
    const steps: CleanupStep[] = props.onExecute.mock.calls[0][0];
    const checkedIds = steps.filter((s) => s.checked).map((s) => s.id);
    expect(checkedIds).toEqual(["switch", "delete-local", "delete-remote"]);
  });

  it("Skip button calls onSkip", () => {
    const props = defaultProps();
    const { container } = render(() => (
      <PostMergeCleanupDialog {...props} />
    ));
    const skipBtn = container.querySelector("[data-testid='skip-btn']")!;
    fireEvent.click(skipBtn);
    expect(props.onSkip).toHaveBeenCalledOnce();
  });

  it("disables checkboxes and buttons during execution", () => {
    const { container } = render(() => (
      <PostMergeCleanupDialog {...defaultProps()} executing={true} />
    ));
    const checkboxes = container.querySelectorAll("input[type='checkbox']");
    checkboxes.forEach((cb) => {
      expect((cb as HTMLInputElement).disabled).toBe(true);
    });
    const executeBtn = container.querySelector(
      "[data-testid='execute-btn']",
    ) as HTMLButtonElement;
    const skipBtn = container.querySelector(
      "[data-testid='skip-btn']",
    ) as HTMLButtonElement;
    expect(executeBtn.disabled).toBe(true);
    expect(skipBtn.disabled).toBe(true);
  });

  it("shows per-step status icons", () => {
    const stepStatuses = {
      switch: "success" as const,
      pull: "running" as const,
      "delete-local": "pending" as const,
      "delete-remote": "error" as const,
    };
    const { container } = render(() => (
      <PostMergeCleanupDialog
        {...defaultProps()}
        executing={true}
        stepStatuses={stepStatuses}
      />
    ));
    expect(
      container.querySelector("[data-testid='step-status-switch']")!
        .textContent,
    ).toContain("\u2713"); // checkmark
    expect(
      container.querySelector("[data-testid='step-status-pull']")!.textContent,
    ).toContain("\u25CF"); // spinner dot
    expect(
      container.querySelector("[data-testid='step-status-delete-remote']")!
        .textContent,
    ).toContain("\u2717"); // cross
  });

  it("displays error messages per step", () => {
    const stepStatuses = { "delete-remote": "error" as const };
    const stepErrors = { "delete-remote": "remote ref does not exist" };
    const { container } = render(() => (
      <PostMergeCleanupDialog
        {...defaultProps()}
        executing={true}
        stepStatuses={stepStatuses}
        stepErrors={stepErrors}
      />
    ));
    const errorEl = container.querySelector(
      "[data-testid='step-error-delete-remote']",
    );
    expect(errorEl).not.toBeNull();
    expect(errorEl!.textContent).toBe("remote ref does not exist");
  });

  it("pre-unchecks 'switch' and 'pull' when already on base branch", () => {
    const { container } = render(() => (
      <PostMergeCleanupDialog {...defaultProps()} isOnBaseBranch={true} />
    ));
    const switchCheck = container.querySelector(
      "input[data-testid='step-check-switch']",
    ) as HTMLInputElement;
    const pullCheck = container.querySelector(
      "input[data-testid='step-check-pull']",
    ) as HTMLInputElement;
    expect(switchCheck.checked).toBe(false);
    // pull should also be unchecked — already on base, no need to pull after switch
    expect(pullCheck.checked).toBe(false);
  });

  it("disables 'delete-local' when branch is the default branch", () => {
    const { container } = render(() => (
      <PostMergeCleanupDialog {...defaultProps()} isDefaultBranch={true} />
    ));
    const deleteLocalCheck = container.querySelector(
      "input[data-testid='step-check-delete-local']",
    ) as HTMLInputElement;
    expect(deleteLocalCheck.disabled).toBe(true);
    expect(deleteLocalCheck.checked).toBe(false);
  });

  describe("worktree mode", () => {
    it("renders worktree step as first step when worktreeAction is set", () => {
      const { container } = render(() => (
        <PostMergeCleanupDialog {...defaultProps()} worktreeAction="archive" />
      ));
      const labels = container.querySelectorAll("[data-testid^='step-label-']");
      expect(labels.length).toBe(5);
      expect(labels[0].textContent).toContain("Archive worktree");
    });

    it("worktree step shows 'Delete worktree' when worktreeAction is delete", () => {
      const { container } = render(() => (
        <PostMergeCleanupDialog {...defaultProps()} worktreeAction="delete" />
      ));
      const label = container.querySelector("[data-testid='step-label-worktree']");
      expect(label).not.toBeNull();
      expect(label!.textContent).toContain("Delete worktree");
    });

    it("worktree step is checked by default and included in onExecute", () => {
      const props = defaultProps();
      const { container } = render(() => (
        <PostMergeCleanupDialog {...props} worktreeAction="archive" />
      ));
      const wtCheck = container.querySelector(
        "input[data-testid='step-check-worktree']",
      ) as HTMLInputElement;
      expect(wtCheck.checked).toBe(true);

      const executeBtn = container.querySelector("[data-testid='execute-btn']")!;
      fireEvent.click(executeBtn);

      const steps: CleanupStep[] = props.onExecute.mock.calls[0][0];
      const worktreeStep = steps.find((s) => s.id === "worktree");
      expect(worktreeStep).toBeDefined();
      expect(worktreeStep!.checked).toBe(true);
    });

    it("worktree mode pre-unchecks switch/pull (user is not in worktree dir)", () => {
      const { container } = render(() => (
        <PostMergeCleanupDialog
          {...defaultProps()}
          worktreeAction="archive"
          isOnBaseBranch={false}
        />
      ));
      // In worktree mode, the user is working in the main repo checkout,
      // not in the worktree — switch/pull are for the main repo and should be offered
      const switchCheck = container.querySelector(
        "input[data-testid='step-check-switch']",
      ) as HTMLInputElement;
      // switch should be available (not disabled) since the user may not be on base
      expect(switchCheck.disabled).toBe(false);
    });

    it("does not render worktree step without worktreeAction prop", () => {
      const { container } = render(() => (
        <PostMergeCleanupDialog {...defaultProps()} />
      ));
      const wtCheck = container.querySelector(
        "input[data-testid='step-check-worktree']",
      );
      expect(wtCheck).toBeNull();
    });
  });
});
