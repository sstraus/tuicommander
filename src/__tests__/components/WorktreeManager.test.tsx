import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "@tauri-apps/api/core";
import { WorktreeManager } from "../../components/WorktreeManager";
import { worktreeManagerStore } from "../../stores/worktreeManager";
import { repositoriesStore } from "../../stores/repositories";

describe("WorktreeManager", () => {
  beforeEach(() => {
    worktreeManagerStore.close();
    vi.clearAllMocks();
    // Clear repos — reset to empty
    for (const path of Object.keys(repositoriesStore.state.repositories)) {
      repositoriesStore.remove(path);
    }
  });

  it("renders nothing when store is closed", () => {
    const { container } = render(() => <WorktreeManager />);
    expect(container.innerHTML).toBe("");
  });

  it("renders overlay when store is open", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);
    expect(container.querySelector("[class*='overlay']")).not.toBeNull();
    expect(container.querySelector("[class*='panel']")).not.toBeNull();
  });

  it("shows empty state when no worktrees exist", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);
    expect(container.querySelector("[class*='empty']")?.textContent).toContain("No worktrees found");
  });

  it("lists worktrees from repositories store", () => {
    repositoriesStore.add({ path: "/repo", displayName: "MyRepo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setBranch("/repo", "feature-a", { worktreePath: "/repo/.wt/feature-a" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const rows = container.querySelectorAll("[class*='row']");
    expect(rows.length).toBe(2);

    // feature-a should be first (non-main sorted before main)
    const branchTexts = Array.from(rows).map((r) =>
      r.querySelector("[class*='branch']")?.textContent,
    );
    expect(branchTexts[0]).toBe("feature-a");
    expect(branchTexts[1]).toBe("main");
  });

  it("shows main badge on main worktree rows", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='mainBadge']")?.textContent).toBe("main");
  });

  it("shows dirty stats for worktrees with changes", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "feature-x", { worktreePath: "/repo/.wt/x", additions: 5, deletions: 3 });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const stats = container.querySelector("[class*='stats']");
    expect(stats?.textContent).toContain("+5");
    expect(stats?.textContent).toContain("-3");
  });

  it("shows 'clean' for worktrees with no changes", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "feature-y", { worktreePath: "/repo/.wt/y", additions: 0, deletions: 0 });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='statsClean']")?.textContent).toBe("clean");
  });

  it("closes on escape key", () => {
    worktreeManagerStore.open();
    render(() => <WorktreeManager />);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("closes on backdrop click", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const overlay = container.querySelector("[class*='overlay']");
    if (overlay) fireEvent.click(overlay);
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("does not close when clicking inside the panel", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const panel = container.querySelector("[class*='panel']");
    if (panel) fireEvent.click(panel);
    expect(worktreeManagerStore.state.isOpen).toBe(true);
  });

  it("shows footer with worktree count", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setBranch("/repo", "feat", { worktreePath: "/repo/.wt/feat" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='footer']")?.textContent).toContain("2 worktree(s)");
  });

  it("shows repo name badge", () => {
    repositoriesStore.add({ path: "/home/user/my-project", displayName: "my-project" });
    repositoriesStore.setBranch("/home/user/my-project", "feat", { worktreePath: "/home/user/my-project/.wt/feat" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='repo']")?.textContent).toBe("my-project");
  });

  describe("repo filter pills", () => {
    it("shows repo pill buttons including All", () => {
      repositoriesStore.add({ path: "/repo-a", displayName: "Alpha" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.add({ path: "/repo-b", displayName: "Beta" });
      repositoriesStore.setBranch("/repo-b", "dev", { worktreePath: "/repo-b" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager />);

      const pills = container.querySelectorAll("[class*='filterPill']");
      // "All" + "Alpha" + "Beta"
      expect(pills.length).toBe(3);
      expect(pills[0].textContent).toBe("All");
    });

    it("filters rows by selected repo", () => {
      repositoriesStore.add({ path: "/repo-a", displayName: "Alpha" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.add({ path: "/repo-b", displayName: "Beta" });
      repositoriesStore.setBranch("/repo-b", "dev", { worktreePath: "/repo-b" });

      worktreeManagerStore.open();
      worktreeManagerStore.setRepoFilter("/repo-a");
      const { container } = render(() => <WorktreeManager />);

      const rows = container.querySelectorAll("[class*='row']");
      expect(rows.length).toBe(1);
      expect(rows[0].querySelector("[class*='branch']")?.textContent).toBe("main");
    });

    it("shows all repos when filter is null (All)", () => {
      repositoriesStore.add({ path: "/repo-a", displayName: "Alpha" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.add({ path: "/repo-b", displayName: "Beta" });
      repositoriesStore.setBranch("/repo-b", "dev", { worktreePath: "/repo-b" });

      worktreeManagerStore.open();
      // repoFilter is null by default
      const { container } = render(() => <WorktreeManager />);

      const rows = container.querySelectorAll("[class*='row']");
      expect(rows.length).toBe(2);
    });
  });

  describe("text search filter", () => {
    it("filters rows by branch name substring (case-insensitive)", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature-auth", { worktreePath: "/repo/.wt/auth" });
      repositoriesStore.setBranch("/repo", "feature-billing", { worktreePath: "/repo/.wt/billing" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      worktreeManagerStore.open();
      worktreeManagerStore.setTextFilter("AUTH");
      const { container } = render(() => <WorktreeManager />);

      const rows = container.querySelectorAll("[class*='row']");
      expect(rows.length).toBe(1);
      expect(rows[0].querySelector("[class*='branch']")?.textContent).toBe("feature-auth");
    });

    it("shows search input", () => {
      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager />);

      const searchInput = container.querySelector("[class*='searchInput']");
      expect(searchInput).not.toBeNull();
    });
  });

  describe("row actions", () => {
    const mockActions = {
      onOpenTerminal: vi.fn(),
      onDelete: vi.fn(),
      onMergeAndArchive: vi.fn(),
    };

    it("shows action buttons on non-main worktree rows", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature-x", { worktreePath: "/repo/.wt/x" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager actions={mockActions} />);

      const row = container.querySelector("[class*='row']");
      expect(row?.querySelector("[class*='actionBtn']")).not.toBeNull();
    });

    it("disables delete and merge on main worktree rows", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager actions={mockActions} />);

      const row = container.querySelector("[class*='mainRow']");
      const deleteBtn = row?.querySelector("[data-action='delete']") as HTMLButtonElement | null;
      const mergeBtn = row?.querySelector("[data-action='merge']") as HTMLButtonElement | null;
      expect(deleteBtn?.disabled).toBe(true);
      expect(mergeBtn?.disabled).toBe(true);
    });

    it("calls onOpenTerminal when terminal button is clicked", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feat", { worktreePath: "/repo/.wt/feat" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager actions={mockActions} />);

      const termBtn = container.querySelector("[data-action='terminal']")!;
      fireEvent.click(termBtn);
      expect(mockActions.onOpenTerminal).toHaveBeenCalledWith("/repo", "feat");
    });

    it("calls onDelete when delete button is clicked", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feat", { worktreePath: "/repo/.wt/feat" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager actions={mockActions} />);

      const deleteBtn = container.querySelector("[data-action='delete']")!;
      fireEvent.click(deleteBtn);
      expect(mockActions.onDelete).toHaveBeenCalledWith("/repo", "feat");
    });

    it("calls onMergeAndArchive when merge button is clicked", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feat", { worktreePath: "/repo/.wt/feat" });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager actions={mockActions} />);

      const mergeBtn = container.querySelector("[data-action='merge']")!;
      fireEvent.click(mergeBtn);
      expect(mockActions.onMergeAndArchive).toHaveBeenCalledWith("/repo", "feat");
    });
  });

  describe("composing filters", () => {
    it("applies repo filter AND text filter together", () => {
      repositoriesStore.add({ path: "/repo-a", displayName: "Alpha" });
      repositoriesStore.setBranch("/repo-a", "feature-auth", { worktreePath: "/repo-a/.wt/auth" });
      repositoriesStore.setBranch("/repo-a", "feature-billing", { worktreePath: "/repo-a/.wt/billing" });
      repositoriesStore.add({ path: "/repo-b", displayName: "Beta" });
      repositoriesStore.setBranch("/repo-b", "feature-auth", { worktreePath: "/repo-b/.wt/auth" });

      worktreeManagerStore.open();
      worktreeManagerStore.setRepoFilter("/repo-a");
      worktreeManagerStore.setTextFilter("auth");
      const { container } = render(() => <WorktreeManager />);

      const rows = container.querySelectorAll("[class*='row']");
      expect(rows.length).toBe(1);
      expect(rows[0].querySelector("[class*='branch']")?.textContent).toBe("feature-auth");
      expect(rows[0].querySelector("[class*='repo']")?.textContent).toBe("Alpha");
    });
  });

  describe("orphan worktrees", () => {
    it("calls detect_orphan_worktrees for each repo when panel opens", async () => {
      repositoriesStore.add({ path: "/repo-a", displayName: "A" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.add({ path: "/repo-b", displayName: "B" });
      repositoriesStore.setBranch("/repo-b", "dev", { worktreePath: "/repo-b" });

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "detect_orphan_worktrees") return [];
        return undefined;
      });

      worktreeManagerStore.open();
      render(() => <WorktreeManager />);

      // Wait for async effects
      await vi.waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("detect_orphan_worktrees", { repoPath: "/repo-a" });
        expect(invoke).toHaveBeenCalledWith("detect_orphan_worktrees", { repoPath: "/repo-b" });
      });
    });

    it("displays orphan rows with orphan badge", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "detect_orphan_worktrees") return ["/repo/.wt/stale-branch"];
        return undefined;
      });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager />);

      await vi.waitFor(() => {
        expect(container.querySelector("[class*='orphanBadge']")).not.toBeNull();
      });

      const orphanBadge = container.querySelector("[class*='orphanBadge']");
      expect(orphanBadge?.textContent).toBe("orphan");
    });

    it("shows prune button on orphan rows", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "detect_orphan_worktrees") return ["/repo/.wt/stale"];
        return undefined;
      });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager />);

      await vi.waitFor(() => {
        const pruneBtn = container.querySelector("[class*='pruneBtn']");
        expect(pruneBtn).not.toBeNull();
        expect(pruneBtn?.textContent).toBe("Prune");
      });
    });

    it("calls remove_orphan_worktree and removes row on prune click", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "detect_orphan_worktrees") return ["/repo/.wt/stale"];
        return undefined;
      });

      worktreeManagerStore.open();
      const { container } = render(() => <WorktreeManager />);

      await vi.waitFor(() => {
        expect(container.querySelector("[class*='pruneBtn']")).not.toBeNull();
      });

      const pruneBtn = container.querySelector("[class*='pruneBtn']")!;
      fireEvent.click(pruneBtn);

      await vi.waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("remove_orphan_worktree", {
          repoPath: "/repo",
          worktreePath: "/repo/.wt/stale",
        });
      });

      // After pruning, the orphan row should be gone
      await vi.waitFor(() => {
        expect(container.querySelector("[class*='orphanBadge']")).toBeNull();
      });
    });
  });
});
