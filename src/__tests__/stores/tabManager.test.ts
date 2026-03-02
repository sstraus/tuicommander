import { describe, it, expect, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import { createTabManager, makeBranchKey, type BaseTab } from "../../stores/tabManager";

// A minimal tab type for tests
interface TestTab extends BaseTab {
  id: string;
  label: string;
  pinned?: boolean;
  branchKey?: string;
}

function makeTab(id: string, overrides: Partial<TestTab> = {}): TestTab {
  return { id, label: `Tab ${id}`, ...overrides };
}

describe("createTabManager", () => {
  let mgr: ReturnType<typeof createTabManager<TestTab>>;

  beforeEach(() => {
    // Each test gets a fresh manager (no module-level singleton in tabManager.ts)
    mgr = createTabManager<TestTab>();
  });

  describe("_addTab / _nextId", () => {
    it("adds a tab and sets it active", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        expect(mgr.get("t1")).toBeDefined();
        expect(mgr.state.activeId).toBe("t1");
        dispose();
      });
    });

    it("_nextId returns incrementing ids with prefix", () => {
      createRoot((dispose) => {
        const id1 = mgr._nextId("diff");
        const id2 = mgr._nextId("diff");
        expect(id1).toBe("diff-1");
        expect(id2).toBe("diff-2");
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes a tab by id", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.remove("t1");
        expect(mgr.get("t1")).toBeUndefined();
        expect(mgr.get("t2")).toBeDefined();
        dispose();
      });
    });

    it("falls back to last remaining tab when active tab is removed", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        expect(mgr.state.activeId).toBe("t2");
        mgr.remove("t2");
        expect(mgr.state.activeId).toBe("t1");
        dispose();
      });
    });

    it("sets activeId to null when last tab is removed", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        mgr.remove("t1");
        expect(mgr.state.activeId).toBeNull();
        dispose();
      });
    });

    it("does not change activeId when a non-active tab is removed", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.setActive("t1");
        mgr.remove("t2");
        expect(mgr.state.activeId).toBe("t1");
        dispose();
      });
    });
  });

  describe("getVisibleIds()", () => {
    it("pinned tabs are always visible regardless of branch", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("pinned", { pinned: true, branchKey: "/repo|main" }));
        mgr._addTab(makeTab("other", { branchKey: "/repo|feature" }));

        const visible = mgr.getVisibleIds("/repo|feature");
        expect(visible).toContain("pinned");
        dispose();
      });
    });

    it("unscoped tabs (no branchKey) are always visible", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("global")); // no branchKey, no pinned
        mgr._addTab(makeTab("branch-scoped", { branchKey: "/repo|feature" }));

        const visible = mgr.getVisibleIds("/repo|main");
        expect(visible).toContain("global");
        expect(visible).not.toContain("branch-scoped");
        dispose();
      });
    });

    it("branch-scoped tabs are visible only in their branch", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("in-main", { branchKey: "/repo|main" }));
        mgr._addTab(makeTab("in-feature", { branchKey: "/repo|feature" }));

        const visibleInMain = mgr.getVisibleIds("/repo|main");
        expect(visibleInMain).toContain("in-main");
        expect(visibleInMain).not.toContain("in-feature");

        const visibleInFeature = mgr.getVisibleIds("/repo|feature");
        expect(visibleInFeature).toContain("in-feature");
        expect(visibleInFeature).not.toContain("in-main");
        dispose();
      });
    });

    it("returns empty array when no tabs exist", () => {
      createRoot((dispose) => {
        expect(mgr.getVisibleIds("/repo|main")).toEqual([]);
        dispose();
      });
    });

    it("returns all unscoped and pinned tabs when branchKey is null", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("global")); // unscoped
        mgr._addTab(makeTab("pinned", { pinned: true }));
        mgr._addTab(makeTab("scoped", { branchKey: "/repo|main" }));

        const visible = mgr.getVisibleIds(null);
        expect(visible).toContain("global");
        expect(visible).toContain("pinned");
        expect(visible).not.toContain("scoped");
        dispose();
      });
    });

    it("repo-scoped pinned tab is visible only in matching repo", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("plan-repo1", { pinned: true, repoPath: "/repo1" }));
        mgr._addTab(makeTab("plan-repo2", { pinned: true, repoPath: "/repo2" }));
        mgr._addTab(makeTab("global-pinned", { pinned: true })); // no repoPath

        const visibleInRepo1 = mgr.getVisibleIds("/repo1|main");
        expect(visibleInRepo1).toContain("plan-repo1");
        expect(visibleInRepo1).not.toContain("plan-repo2");
        expect(visibleInRepo1).toContain("global-pinned");

        const visibleInRepo2 = mgr.getVisibleIds("/repo2|feature");
        expect(visibleInRepo2).not.toContain("plan-repo1");
        expect(visibleInRepo2).toContain("plan-repo2");
        expect(visibleInRepo2).toContain("global-pinned");
        dispose();
      });
    });

    it("repo-scoped tab is hidden when branchKey is null", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("repo-scoped", { pinned: true, repoPath: "/repo1" }));

        const visible = mgr.getVisibleIds(null);
        expect(visible).not.toContain("repo-scoped");
        dispose();
      });
    });
  });

  describe("setPinned()", () => {
    it("sets a tab as pinned", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1", { branchKey: "/repo|main" }));
        mgr.setPinned("t1", true);
        expect(mgr.get("t1")!.pinned).toBe(true);
        dispose();
      });
    });

    it("unpins a pinned tab", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1", { pinned: true }));
        mgr.setPinned("t1", false);
        expect(mgr.get("t1")!.pinned).toBe(false);
        dispose();
      });
    });

    it("does nothing for an unknown tab id", () => {
      createRoot((dispose) => {
        // Should not throw
        expect(() => mgr.setPinned("nonexistent", true)).not.toThrow();
        dispose();
      });
    });
  });

  describe("clearAll()", () => {
    it("removes all tabs and clears activeId", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.clearAll();
        expect(mgr.getCount()).toBe(0);
        expect(mgr.state.activeId).toBeNull();
        dispose();
      });
    });

    it("preserves counter after clearAll", () => {
      createRoot((dispose) => {
        mgr._nextId("x"); // counter = 1
        mgr.clearAll();
        const next = mgr._nextId("x");
        expect(next).toBe("x-2"); // counter continues from 2
        dispose();
      });
    });
  });

  describe("getIds / getCount / getActive", () => {
    it("getIds returns all tab ids", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("a"));
        mgr._addTab(makeTab("b"));
        expect(mgr.getIds()).toEqual(expect.arrayContaining(["a", "b"]));
        expect(mgr.getIds()).toHaveLength(2);
        dispose();
      });
    });

    it("getCount returns correct count", () => {
      createRoot((dispose) => {
        expect(mgr.getCount()).toBe(0);
        mgr._addTab(makeTab("a"));
        expect(mgr.getCount()).toBe(1);
        dispose();
      });
    });

    it("getActive returns the active tab", () => {
      createRoot((dispose) => {
        mgr._addTab(makeTab("a"));
        mgr._addTab(makeTab("b"));
        mgr.setActive("a");
        expect(mgr.getActive()?.id).toBe("a");
        dispose();
      });
    });

    it("getActive returns undefined when no active tab", () => {
      createRoot((dispose) => {
        expect(mgr.getActive()).toBeUndefined();
        dispose();
      });
    });
  });

  describe("makeBranchKey()", () => {
    it("combines repoPath and branchName with pipe separator", () => {
      expect(makeBranchKey("/repo/path", "main")).toBe("/repo/path|main");
    });

    it("handles branches with slashes", () => {
      expect(makeBranchKey("/repo", "feature/my-feature")).toBe("/repo|feature/my-feature");
    });
  });
});
