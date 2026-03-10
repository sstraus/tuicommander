import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

const mockWrite = vi.fn();
const mockBumpRevision = vi.fn();

vi.mock("../../stores/terminals", () => ({
  terminalsStore: {
    getActive: () => ({ ref: { write: mockWrite } }),
  },
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    getRevision: () => 0,
    bumpRevision: (...args: unknown[]) => mockBumpRevision(...args),
  },
}));

import { GitOperationsPanel } from "../../components/GitOperationsPanel/GitOperationsPanel";

/** Default GitPanelContext for tests */
const makeContext = (overrides: Record<string, unknown> = {}) => ({
  branch: "feature/test",
  is_detached: false,
  status: "clean",
  ahead: 2,
  behind: 1,
  staged_count: 3,
  changed_count: 5,
  stash_count: 1,
  last_commit: { hash: "abc123def", short_hash: "abc123d", subject: "fix: something" },
  in_rebase: false,
  in_cherry_pick: false,
  ...overrides,
});

const makeBranches = () => [
  { name: "main", is_current: false, is_remote: false },
  { name: "feature/test", is_current: true, is_remote: false },
  { name: "origin/main", is_current: false, is_remote: true },
];

describe("GitOperationsPanel", () => {
  const defaultProps = {
    visible: true,
    repoPath: "/repo" as string | null,
    onClose: vi.fn(),
    onBranchChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_git_panel_context") return Promise.resolve(makeContext());
      if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
      if (cmd === "run_git_command") return Promise.resolve({ success: true, stdout: "Done\n", stderr: "", exit_code: 0 });
      return Promise.resolve(null);
    });
  });

  describe("visibility", () => {
    it("does not render when visible=false", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} visible={false} />
      ));
      expect(container.querySelector("[data-testid='git-operations-panel']")).toBeNull();
    });

    it("renders when visible=true", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      expect(container.querySelector("[data-testid='git-operations-panel']")).not.toBeNull();
    });
  });

  describe("header", () => {
    it("shows 'Git Operations' title", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const title = container.querySelector("[class*='headerTitle']");
      expect(title!.textContent).toBe("Git Operations");
    });

    it("close button calls onClose", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onClose={onClose} />
      ));
      const closeBtn = container.querySelector("[class*='closeBtn']")!;
      fireEvent.click(closeBtn);
      expect(onClose).toHaveBeenCalledOnce();
    });
  });

  describe("status card", () => {
    it("shows branch name from context", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const branch = container.querySelector("[class*='branchName']");
        expect(branch!.textContent).toBe("feature/test");
      });
    });

    it("shows status badge", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const badge = container.querySelector("[class*='statusBadge']");
        expect(badge!.textContent).toBe("clean");
      });
    });

    it("shows ahead/behind counts", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const counts = container.querySelector("[class*='countsRow']");
        expect(counts!.textContent).toContain("\u21912");
        expect(counts!.textContent).toContain("\u21931");
      });
    });

    it("shows last commit", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const commit = container.querySelector("[class*='lastCommit']");
        expect(commit!.textContent).toContain("abc123d");
      });
    });

    it("shows DETACHED badge when detached", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ is_detached: true }));
        if (cmd === "get_git_branches") return Promise.resolve([]);
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        expect(container.querySelector("[class*='detachedBadge']")).not.toBeNull();
      });
    });
  });

  describe("background execution", () => {
    it("clicking Pull invokes run_git_command with pull args", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["pull"],
        });
      });
    });

    it("clicking Push invokes run_git_command with push args", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Push")!;
      fireEvent.click(pushBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["push"],
        });
      });
    });

    it("clicking Fetch invokes run_git_command with fetch args", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const fetchBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Fetch")!;
      fireEvent.click(fetchBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["fetch", "--all"],
        });
      });
    });

    it("does NOT write to terminal for sync operations", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", expect.any(Object));
      });
      expect(mockWrite).not.toHaveBeenCalled();
    });

    it("does NOT close panel after operation", async () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onClose={onClose} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", expect.any(Object));
      });
      expect(onClose).not.toHaveBeenCalled();
    });

    it("bumps revision on success", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);

      await vi.waitFor(() => {
        expect(mockBumpRevision).toHaveBeenCalledWith("/repo");
      });
    });

    it("shows success feedback bar after successful operation", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);

      await vi.waitFor(() => {
        const fb = container.querySelector("[data-testid='feedback-bar']");
        expect(fb).not.toBeNull();
        expect(fb!.textContent).toContain("Done");
        expect(fb!.className).toContain("feedbackSuccess");
      });
    });

    it("shows error feedback bar on failure", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext());
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        if (cmd === "run_git_command") return Promise.resolve({
          success: false, stdout: "", stderr: "fatal: no upstream\n", exit_code: 1,
        });
        return Promise.resolve(null);
      });

      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Push")!;
      fireEvent.click(pushBtn);

      await vi.waitFor(() => {
        const fb = container.querySelector("[data-testid='feedback-bar']");
        expect(fb).not.toBeNull();
        expect(fb!.textContent).toContain("fatal: no upstream");
        expect(fb!.className).toContain("feedbackError");
      });
    });

    it("does not bump revision on error", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext());
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        if (cmd === "run_git_command") return Promise.resolve({
          success: false, stdout: "", stderr: "error\n", exit_code: 1,
        });
        return Promise.resolve(null);
      });

      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Push")!;
      fireEvent.click(pushBtn);

      await vi.waitFor(() => {
        expect(container.querySelector("[data-testid='feedback-bar']")).not.toBeNull();
      });
      expect(mockBumpRevision).not.toHaveBeenCalled();
    });

    it("does nothing when repoPath is null", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);
      // Should not call run_git_command (only get_git_panel_context/branches on mount)
      expect(mockInvoke).not.toHaveBeenCalledWith("run_git_command", expect.any(Object));
    });
  });

  describe("stash operations via background execution", () => {
    it("clicking Stash invokes run_git_command with stash args", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const stashBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Stash")!;
      fireEvent.click(stashBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["stash"],
        });
      });
    });

    it("clicking Pop invokes run_git_command with stash pop args", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const popBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pop")!;
      fireEvent.click(popBtn);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["stash", "pop"],
        });
      });
    });
  });

  describe("merge/rebase/cherry-pick use terminal injection", () => {
    it("merge Abort writes to terminal (not run_git_command)", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ status: "conflict" }));
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        expect(container.querySelector("[class*='alertSection']")).not.toBeNull();
      });
      const alertBtns = container.querySelector("[class*='alertSection']")!.querySelectorAll("[class*='btn']");
      const abortBtn = Array.from(alertBtns).find((b) => b.textContent?.includes("Abort"))!;
      fireEvent.click(abortBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git merge --abort");
    });
  });

  describe("SVG icons", () => {
    it("all button icons use SVG, no Unicode symbols", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const iconSpans = container.querySelectorAll("[class*='btnIcon']");
      for (const span of Array.from(iconSpans)) {
        expect(span.querySelector("svg")).not.toBeNull();
      }
    });
  });

  describe("create branch form", () => {
    it("New button toggles the inline form", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      expect(container.querySelector("[data-testid='new-branch-form']")).toBeNull();
      const newBtn = container.querySelector("[data-testid='new-branch-toggle']")!;
      fireEvent.click(newBtn);
      expect(container.querySelector("[data-testid='new-branch-form']")).not.toBeNull();
      // Toggle off
      fireEvent.click(newBtn);
      expect(container.querySelector("[data-testid='new-branch-form']")).toBeNull();
    });

    it("shows error for empty branch name", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);
      // Button is disabled when name is empty, so no error shown via click
      // Enter key on empty input
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.keyDown(input, { key: "Enter" });
      const error = container.querySelector("[data-testid='new-branch-error']");
      expect(error).not.toBeNull();
      expect(error!.textContent).toContain("required");
    });

    it("shows error for invalid branch name", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "bad branch name.." } });
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);
      const error = container.querySelector("[data-testid='new-branch-error']");
      expect(error).not.toBeNull();
      expect(error!.textContent).toContain("Invalid");
    });

    it("shows error for duplicate branch name", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      // Wait for context and branches to load (branch name appears in status card)
      await vi.waitFor(() => {
        const branch = container.querySelector("[class*='branchName']");
        expect(branch!.textContent).toBe("feature/test");
      });
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "main" } });
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);
      const error = container.querySelector("[data-testid='new-branch-error']");
      expect(error).not.toBeNull();
      expect(error!.textContent).toContain("already exists");
    });

    it("Create calls git branch via run_git_command", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "new-feature" } });
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["branch", "new-feature"],
        });
      });
    });

    it("Create & Switch calls git checkout -b via run_git_command", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "new-feature" } });
      fireEvent.click(container.querySelector("[data-testid='create-switch-btn']")!);

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
          path: "/repo",
          args: ["checkout", "-b", "new-feature"],
        });
      });
    });

    it("Cancel collapses the form", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      expect(container.querySelector("[data-testid='new-branch-form']")).not.toBeNull();
      fireEvent.click(container.querySelector("[data-testid='cancel-branch-btn']")!);
      expect(container.querySelector("[data-testid='new-branch-form']")).toBeNull();
    });

    it("form collapses on success and shows feedback", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "new-feature" } });
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);

      await vi.waitFor(() => {
        expect(container.querySelector("[data-testid='new-branch-form']")).toBeNull();
        const fb = container.querySelector("[data-testid='feedback-bar']");
        expect(fb).not.toBeNull();
        expect(fb!.textContent).toContain("new-feature");
      });
    });

    it("form stays open on error and shows inline error", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext());
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        if (cmd === "run_git_command") return Promise.resolve({
          success: false, stdout: "", stderr: "fatal: branch already exists\n", exit_code: 128,
        });
        return Promise.resolve(null);
      });

      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      fireEvent.click(container.querySelector("[data-testid='new-branch-toggle']")!);
      const input = container.querySelector("[data-testid='new-branch-form'] input")!;
      fireEvent.input(input, { target: { value: "new-feature" } });
      fireEvent.click(container.querySelector("[data-testid='create-branch-btn']")!);

      await vi.waitFor(() => {
        const error = container.querySelector("[data-testid='new-branch-error']");
        expect(error).not.toBeNull();
        expect(error!.textContent).toContain("fatal");
      });
      // Form should still be visible
      expect(container.querySelector("[data-testid='new-branch-form']")).not.toBeNull();
    });
  });

  describe("keyboard navigation", () => {
    it("Escape key closes the panel", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onClose={onClose} />
      ));
      const panel = container.querySelector("[data-testid='git-operations-panel']")!;
      fireEvent.keyDown(panel, { key: "Escape" });
      expect(onClose).toHaveBeenCalledOnce();
    });

    it("panel has tabIndex for focus", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const panel = container.querySelector("[data-testid='git-operations-panel']")!;
      expect(panel.getAttribute("tabindex")).toBe("-1");
    });
  });

  describe("calls get_git_panel_context", () => {
    it("invokes get_git_panel_context when visible with repoPath", () => {
      render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      expect(mockInvoke).toHaveBeenCalledWith("get_git_panel_context", { path: "/repo" });
    });

    it("does not invoke when repoPath is null", () => {
      render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });
});
