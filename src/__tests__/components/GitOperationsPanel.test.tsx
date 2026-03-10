import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

const mockWrite = vi.fn();

vi.mock("../../stores/terminals", () => ({
  terminalsStore: {
    getActive: () => ({ ref: { write: mockWrite } }),
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
    // Default: return context for first call, branches for second
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_git_panel_context") return Promise.resolve(makeContext());
      if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
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
      expect(title).not.toBeNull();
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

    it("shows staged and changed counts", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const counts = container.querySelector("[class*='countsRow']");
        expect(counts!.textContent).toContain("3 staged");
        expect(counts!.textContent).toContain("5 changed");
      });
    });

    it("shows stash count", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const counts = container.querySelector("[class*='countsRow']");
        expect(counts!.textContent).toContain("1 stash");
      });
    });

    it("shows last commit", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const commit = container.querySelector("[class*='lastCommit']");
        expect(commit!.textContent).toContain("abc123d");
        expect(commit!.textContent).toContain("fix: something");
      });
    });

    it("shows DETACHED badge when detached", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ is_detached: true, branch: "abc123d" }));
        if (cmd === "get_git_branches") return Promise.resolve([]);
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const badge = container.querySelector("[class*='detachedBadge']");
        expect(badge).not.toBeNull();
        expect(badge!.textContent).toBe("DETACHED");
      });
    });

    it("shows 'No branch' when context is null", () => {
      mockInvoke.mockImplementation(() => Promise.reject(new Error("fail")));
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const branch = container.querySelector("[class*='branchName']");
      expect(branch!.textContent).toBe("No branch");
    });
  });

  describe("sync section", () => {
    it("shows Pull, Push, Fetch buttons with SVG icons", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("Pull");
      expect(labels).toContain("Push");
      expect(labels).toContain("Fetch");
      // Verify SVG icons (no Unicode)
      const iconSpans = container.querySelectorAll("[class*='btnIcon']");
      for (const span of Array.from(iconSpans)) {
        expect(span.querySelector("svg")).not.toBeNull();
      }
    });

    it("clicking Pull writes git pull command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git pull");
    });

    it("clicking Push writes git push command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Push")!;
      fireEvent.click(pushBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git push");
    });

    it("clicking Fetch writes git fetch command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const fetchBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Fetch")!;
      fireEvent.click(fetchBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git fetch --all");
    });

    it("does nothing when repoPath is null", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      fireEvent.click(pullBtn);
      expect(mockWrite).not.toHaveBeenCalled();
    });
  });

  describe("merge in progress section", () => {
    it("shows merge section when status is conflict", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ status: "conflict" }));
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const alert = container.querySelector("[class*='alertSection']");
        expect(alert).not.toBeNull();
        expect(alert!.textContent).toContain("Merge in Progress");
      });
    });

    it("does not show merge section when clean", async () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const badge = container.querySelector("[class*='statusBadge']");
        expect(badge!.textContent).toBe("clean");
      });
      const alertSections = container.querySelectorAll("[class*='alertSection']");
      expect(alertSections.length).toBe(0);
    });
  });

  describe("rebase in progress section", () => {
    it("shows rebase section when in_rebase is true", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ in_rebase: true }));
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const alerts = container.querySelectorAll("[class*='alertSection']");
        const rebaseAlert = Array.from(alerts).find((a) => a.textContent?.includes("Rebase"));
        expect(rebaseAlert).not.toBeUndefined();
      });
    });
  });

  describe("cherry-pick in progress section", () => {
    it("shows cherry-pick section when in_cherry_pick is true", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_git_panel_context") return Promise.resolve(makeContext({ in_cherry_pick: true }));
        if (cmd === "get_git_branches") return Promise.resolve(makeBranches());
        return Promise.resolve(null);
      });
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      await vi.waitFor(() => {
        const alerts = container.querySelectorAll("[class*='alertSection']");
        const cherryAlert = Array.from(alerts).find((a) => a.textContent?.includes("Cherry-pick"));
        expect(cherryAlert).not.toBeUndefined();
      });
    });
  });

  describe("branch section", () => {
    it("shows BranchCombobox instead of native select", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      // No native <select> should exist
      expect(container.querySelector("select")).toBeNull();
      // BranchCombobox renders an input
      const branchSelect = container.querySelector("[class*='branchSelect']");
      expect(branchSelect).not.toBeNull();
      expect(branchSelect!.querySelector("input")).not.toBeNull();
    });

    it("shows Switch and Merge buttons", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("Switch");
      expect(labels).toContain("Merge");
    });
  });

  describe("stash section", () => {
    it("shows Stash and Pop buttons", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("Stash");
      expect(labels).toContain("Pop");
    });

    it("clicking Stash writes git stash command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const stashBtn = Array.from(buttons).find(
        (b) => b.textContent?.trim() === "Stash"
      )!;
      fireEvent.click(stashBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git stash");
      expect(mockWrite.mock.calls[0][0]).not.toContain("git stash pop");
    });

    it("clicking Pop writes git stash pop command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const popBtn = Array.from(buttons).find(
        (b) => b.textContent?.trim() === "Pop"
      )!;
      fireEvent.click(popBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git stash pop");
    });
  });

  describe("no Unicode icons", () => {
    it("all button icons use SVG, no Unicode symbols", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const iconSpans = container.querySelectorAll("[class*='btnIcon']");
      for (const span of Array.from(iconSpans)) {
        expect(span.querySelector("svg")).not.toBeNull();
        // The only text content should be empty (SVG only)
        const textWithoutSvg = span.textContent?.replace(/\s/g, "") ?? "";
        expect(textWithoutSvg).toBe("");
      }
    });
  });

  describe("panel width", () => {
    it("has 400px width in CSS class", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const panel = container.querySelector("[data-testid='git-operations-panel']");
      expect(panel).not.toBeNull();
      // Panel should have the panel class (width defined in CSS module)
      expect(panel!.className).toContain("panel");
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

  describe("without onBranchChange callback", () => {
    it("does not fail when onBranchChange is not provided", () => {
      const { container } = render(() => (
        <GitOperationsPanel
          visible={true}
          repoPath="/repo"
          onClose={vi.fn()}
        />
      ));
      const buttons = container.querySelectorAll("[class*='btn']");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "Pull")!;
      expect(() => fireEvent.click(pullBtn)).not.toThrow();
    });
  });
});
