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

describe("GitOperationsPanel", () => {
  const defaultProps = {
    visible: true,
    repoPath: "/repo" as string | null,
    currentBranch: "feature/test" as string | null,
    repoStatus: "clean" as const,
    onClose: vi.fn(),
    onBranchChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue([]);
  });

  describe("visibility", () => {
    it("does not render when visible=false", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} visible={false} />
      ));
      const panel = container.querySelector(".git-ops-panel");
      expect(panel).toBeNull();
    });

    it("renders when visible=true", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const panel = container.querySelector(".git-ops-panel");
      expect(panel).not.toBeNull();
    });
  });

  describe("header and status", () => {
    it("shows 'Git Operations' header", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const header = container.querySelector(".git-ops-header h3");
      expect(header).not.toBeNull();
      expect(header!.textContent).toBe("Git Operations");
    });

    it("shows current branch name", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const branch = container.querySelector(".git-ops-branch");
      expect(branch).not.toBeNull();
      expect(branch!.textContent).toBe("feature/test");
    });

    it("shows 'No branch' when currentBranch is null", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} currentBranch={null} />
      ));
      const branch = container.querySelector(".git-ops-branch");
      expect(branch).not.toBeNull();
      expect(branch!.textContent).toBe("No branch");
    });

    it("shows repo status", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const state = container.querySelector(".git-ops-state");
      expect(state).not.toBeNull();
      expect(state!.textContent).toBe("clean");
    });

    it("applies status-specific CSS class", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="dirty" />
      ));
      const state = container.querySelector(".git-ops-state-dirty");
      expect(state).not.toBeNull();
    });
  });

  describe("close button", () => {
    it("calls onClose when clicked", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onClose={onClose} />
      ));
      const closeBtn = container.querySelector(".git-ops-close")!;
      fireEvent.click(closeBtn);
      expect(onClose).toHaveBeenCalledOnce();
    });
  });

  describe("quick action buttons", () => {
    it("shows Pull, Push, Fetch buttons", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("↓Pull");
      expect(labels).toContain("↑Push");
      expect(labels).toContain("⟳Fetch");
    });

    it("disables quick action buttons when no repoPath", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const quickActionsSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Quick Actions"
      );
      expect(quickActionsSection).not.toBeNull();
      const quickActionsButtons = quickActionsSection!
        .parentElement!.querySelectorAll(".git-ops-btn");
      for (const btn of Array.from(quickActionsButtons)) {
        expect((btn as HTMLButtonElement).disabled).toBe(true);
      }
    });

    it("disables quick action buttons during merge", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const quickActionsSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Quick Actions"
      );
      const quickActionsButtons = quickActionsSection!
        .parentElement!.querySelectorAll(".git-ops-btn");
      for (const btn of Array.from(quickActionsButtons)) {
        expect((btn as HTMLButtonElement).disabled).toBe(true);
      }
    });

    it("clicking Pull writes git pull command to terminal", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↓Pull")!;
      fireEvent.click(pullBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git pull");
    });

    it("clicking Push writes git push command to terminal", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↑Push")!;
      fireEvent.click(pushBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git push");
    });

    it("clicking Fetch writes git fetch command to terminal", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const fetchBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "⟳Fetch")!;
      fireEvent.click(fetchBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git fetch --all");
    });

    it("calls onClose after executing a quick action", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onClose={onClose} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↓Pull")!;
      fireEvent.click(pullBtn);
      expect(onClose).toHaveBeenCalledOnce();
    });

    it("calls onBranchChange after executing a quick action", () => {
      const onBranchChange = vi.fn();
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} onBranchChange={onBranchChange} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↓Pull")!;
      fireEvent.click(pullBtn);
      expect(onBranchChange).toHaveBeenCalledOnce();
    });

    it("does nothing when repoPath is null", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      // Even with disabled buttons, try clicking - the handleOperation guard should return
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↓Pull")!;
      fireEvent.click(pullBtn);
      expect(mockWrite).not.toHaveBeenCalled();
    });
  });

  describe("merge section", () => {
    it("shows merge section when repoStatus is 'merge'", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      expect(mergeSection).not.toBeNull();
      const title = mergeSection!.querySelector(".git-ops-section-title");
      expect(title!.textContent).toBe("Merge in Progress");
    });

    it("shows merge section when repoStatus is 'conflict'", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="conflict" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      expect(mergeSection).not.toBeNull();
    });

    it("does not show merge section when repoStatus is 'clean'", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="clean" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      expect(mergeSection).toBeNull();
    });

    it("does not show merge section when repoStatus is 'dirty'", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="dirty" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      expect(mergeSection).toBeNull();
    });

    it("does not show merge section when repoStatus is 'unknown'", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="unknown" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      expect(mergeSection).toBeNull();
    });

    it("shows warning text about resolving conflicts", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const warning = container.querySelector(".git-ops-warning");
      expect(warning).not.toBeNull();
      expect(warning!.textContent).toContain("Resolve conflicts");
    });

    it("shows merge operation buttons (Abort, Continue, Accept Ours, Accept Theirs)", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      const buttons = mergeSection!.querySelectorAll(".git-ops-btn");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("✕Abort Merge");
      expect(labels).toContain("→Continue Merge");
      expect(labels).toContain("◀Accept Ours");
      expect(labels).toContain("▶Accept Theirs");
    });

    it("clicking Abort Merge writes merge --abort command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      const buttons = mergeSection!.querySelectorAll(".git-ops-btn");
      const abortBtn = Array.from(buttons).find((b) => b.textContent?.includes("Abort"))!;
      fireEvent.click(abortBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git merge --abort");
    });

    it("clicking Continue Merge writes merge --continue command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      const buttons = mergeSection!.querySelectorAll(".git-ops-btn");
      const continueBtn = Array.from(buttons).find((b) => b.textContent?.includes("Continue"))!;
      fireEvent.click(continueBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git merge --continue");
    });

    it("clicking Accept Ours writes checkout --ours command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      const buttons = mergeSection!.querySelectorAll(".git-ops-btn");
      const oursBtn = Array.from(buttons).find((b) => b.textContent?.includes("Ours"))!;
      fireEvent.click(oursBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git checkout --ours");
    });

    it("clicking Accept Theirs writes checkout --theirs command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const mergeSection = container.querySelector(".git-ops-merge-section");
      const buttons = mergeSection!.querySelectorAll(".git-ops-btn");
      const theirsBtn = Array.from(buttons).find((b) => b.textContent?.includes("Theirs"))!;
      fireEvent.click(theirsBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git checkout --theirs");
    });
  });

  describe("branch operations", () => {
    it("shows branch selector", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const select = container.querySelector(".git-ops-branch-select select");
      expect(select).not.toBeNull();
    });

    it("shows branch operation buttons (Merge, Checkout)", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("⊕Merge");
      expect(labels).toContain("⎇Checkout");
    });

    it("disables branch operation buttons when no branch selected", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const branchSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Branch Operations"
      );
      const branchButtons = branchSection!.parentElement!.querySelectorAll(
        ".git-ops-buttons .git-ops-btn"
      );
      for (const btn of Array.from(branchButtons)) {
        expect((btn as HTMLButtonElement).disabled).toBe(true);
      }
    });

    it("populates branches from invoke when panel becomes visible", async () => {
      mockInvoke.mockResolvedValue([
        { name: "main", is_current: false, is_remote: false },
        { name: "feature/test", is_current: true, is_remote: false },
        { name: "origin/main", is_current: false, is_remote: true },
      ]);

      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));

      // Wait for the async fetchBranches to resolve
      await vi.waitFor(() => {
        const options = container.querySelectorAll(".git-ops-branch-select option");
        // Should have "Select a branch..." + "main" (current branch filtered out in <For>)
        expect(options.length).toBeGreaterThanOrEqual(2);
      });

      const options = container.querySelectorAll(".git-ops-branch-select option");
      const optionTexts = Array.from(options).map((o) => o.textContent);
      expect(optionTexts).toContain("Select a branch...");
      expect(optionTexts).toContain("main");
      // Remote branches should be excluded
      expect(optionTexts).not.toContain("origin/main");
    });

    it("handles branch fetch error gracefully", async () => {
      const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      mockInvoke.mockRejectedValue(new Error("fetch failed"));

      render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));

      await vi.waitFor(() => {
        expect(errorSpy).toHaveBeenCalledWith("Failed to fetch branches:", expect.any(Error));
      });

      errorSpy.mockRestore();
    });

    it("does not fetch branches when repoPath is null", () => {
      render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      // Should not call invoke when no repoPath
      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it("disables branch selector during merge", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoStatus="merge" />
      ));
      const select = container.querySelector(".git-ops-branch-select select") as HTMLSelectElement;
      expect(select.disabled).toBe(true);
    });
  });

  describe("stash operations", () => {
    it("shows Stash and Pop buttons", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const stashSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Stash"
      );
      expect(stashSection).not.toBeNull();

      const buttons = stashSection!.parentElement!.querySelectorAll(".git-ops-btn");
      const labels = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(labels).toContain("⊡Stash");
      expect(labels).toContain("⊞Pop");
    });

    it("clicking Stash writes git stash command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const stashSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Stash"
      );
      const buttons = stashSection!.parentElement!.querySelectorAll(".git-ops-btn");
      const stashBtn = Array.from(buttons).find((b) => b.textContent?.includes("Stash") && !b.textContent?.includes("Pop"))!;
      fireEvent.click(stashBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git stash");
      // Verify it's not "git stash pop"
      expect(mockWrite.mock.calls[0][0]).not.toContain("git stash pop");
    });

    it("clicking Pop writes git stash pop command", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const stashSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Stash"
      );
      const buttons = stashSection!.parentElement!.querySelectorAll(".git-ops-btn");
      const popBtn = Array.from(buttons).find((b) => b.textContent?.includes("Pop"))!;
      fireEvent.click(popBtn);
      expect(mockWrite).toHaveBeenCalledOnce();
      expect(mockWrite.mock.calls[0][0]).toContain("git stash pop");
    });

    it("disables stash buttons when no repoPath", () => {
      const { container } = render(() => (
        <GitOperationsPanel {...defaultProps} repoPath={null} />
      ));
      const sectionTitles = container.querySelectorAll(".git-ops-section-title");
      const stashSection = Array.from(sectionTitles).find(
        (t) => t.textContent === "Stash"
      );
      const buttons = stashSection!.parentElement!.querySelectorAll(".git-ops-btn");
      for (const btn of Array.from(buttons)) {
        expect((btn as HTMLButtonElement).disabled).toBe(true);
      }
    });
  });

  describe("without onBranchChange callback", () => {
    it("does not fail when onBranchChange is not provided", () => {
      const { container } = render(() => (
        <GitOperationsPanel
          visible={true}
          repoPath="/repo"
          currentBranch="main"
          repoStatus="clean"
          onClose={vi.fn()}
        />
      ));
      const buttons = container.querySelectorAll(".git-ops-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.trim() === "↓Pull")!;
      // Should not throw when onBranchChange is undefined
      expect(() => fireEvent.click(pullBtn)).not.toThrow();
    });
  });
});
