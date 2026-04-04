import { describe, it, expect, beforeEach } from "vitest";
import { createTabManager, makeBranchKey, type BaseTab } from "../../stores/tabManager";
import { testInScope } from "../helpers/store";

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
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        expect(mgr.get("t1")).toBeDefined();
        expect(mgr.state.activeId).toBe("t1");
      });
    });

    it("_nextId returns incrementing ids with prefix", () => {
      testInScope(() => {
        const id1 = mgr._nextId("diff");
        const id2 = mgr._nextId("diff");
        expect(id1).toBe("diff-1");
        expect(id2).toBe("diff-2");
      });
    });
  });

  describe("remove()", () => {
    it("removes a tab by id", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.remove("t1");
        expect(mgr.get("t1")).toBeUndefined();
        expect(mgr.get("t2")).toBeDefined();
      });
    });

    it("falls back to last remaining tab when active tab is removed", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        expect(mgr.state.activeId).toBe("t2");
        mgr.remove("t2");
        expect(mgr.state.activeId).toBe("t1");
      });
    });

    it("sets activeId to null when last tab is removed", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        mgr.remove("t1");
        expect(mgr.state.activeId).toBeNull();
      });
    });

    it("does not change activeId when a non-active tab is removed", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.setActive("t1");
        mgr.remove("t2");
        expect(mgr.state.activeId).toBe("t1");
      });
    });
  });

  describe("getVisibleIds()", () => {
    it("pinned tabs are always visible regardless of branch", () => {
      testInScope(() => {
        mgr._addTab(makeTab("pinned", { pinned: true, branchKey: "/repo|main" }));
        mgr._addTab(makeTab("other", { branchKey: "/repo|feature" }));

        const visible = mgr.getVisibleIds("/repo|feature");
        expect(visible).toContain("pinned");
      });
    });

    it("unscoped tabs (no branchKey) are always visible", () => {
      testInScope(() => {
        mgr._addTab(makeTab("global")); // no branchKey, no pinned
        mgr._addTab(makeTab("branch-scoped", { branchKey: "/repo|feature" }));

        const visible = mgr.getVisibleIds("/repo|main");
        expect(visible).toContain("global");
        expect(visible).not.toContain("branch-scoped");
      });
    });

    it("branch-scoped tabs are visible only in their branch", () => {
      testInScope(() => {
        mgr._addTab(makeTab("in-main", { branchKey: "/repo|main" }));
        mgr._addTab(makeTab("in-feature", { branchKey: "/repo|feature" }));

        const visibleInMain = mgr.getVisibleIds("/repo|main");
        expect(visibleInMain).toContain("in-main");
        expect(visibleInMain).not.toContain("in-feature");

        const visibleInFeature = mgr.getVisibleIds("/repo|feature");
        expect(visibleInFeature).toContain("in-feature");
        expect(visibleInFeature).not.toContain("in-main");
      });
    });

    it("returns empty array when no tabs exist", () => {
      testInScope(() => {
        expect(mgr.getVisibleIds("/repo|main")).toEqual([]);
      });
    });

    it("returns all unscoped and pinned tabs when branchKey is null", () => {
      testInScope(() => {
        mgr._addTab(makeTab("global")); // unscoped
        mgr._addTab(makeTab("pinned", { pinned: true }));
        mgr._addTab(makeTab("scoped", { branchKey: "/repo|main" }));

        const visible = mgr.getVisibleIds(null);
        expect(visible).toContain("global");
        expect(visible).toContain("pinned");
        expect(visible).not.toContain("scoped");
      });
    });

    it("repo-scoped pinned tab is visible only in matching repo", () => {
      testInScope(() => {
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
      });
    });

    it("repo-scoped tab is hidden when branchKey is null", () => {
      testInScope(() => {
        mgr._addTab(makeTab("repo-scoped", { pinned: true, repoPath: "/repo1" }));

        const visible = mgr.getVisibleIds(null);
        expect(visible).not.toContain("repo-scoped");
      });
    });
  });

  describe("setPinned()", () => {
    it("sets a tab as pinned", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1", { branchKey: "/repo|main" }));
        mgr.setPinned("t1", true);
        expect(mgr.get("t1")!.pinned).toBe(true);
      });
    });

    it("unpins a pinned tab", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1", { pinned: true }));
        mgr.setPinned("t1", false);
        expect(mgr.get("t1")!.pinned).toBe(false);
      });
    });

    it("does nothing for an unknown tab id", () => {
      testInScope(() => {
        // Should not throw
        expect(() => mgr.setPinned("nonexistent", true)).not.toThrow();
      });
    });
  });

  describe("clearAll()", () => {
    it("removes all tabs and clears activeId", () => {
      testInScope(() => {
        mgr._addTab(makeTab("t1"));
        mgr._addTab(makeTab("t2"));
        mgr.clearAll();
        expect(mgr.getCount()).toBe(0);
        expect(mgr.state.activeId).toBeNull();
      });
    });

    it("preserves counter after clearAll", () => {
      testInScope(() => {
        mgr._nextId("x"); // counter = 1
        mgr.clearAll();
        const next = mgr._nextId("x");
        expect(next).toBe("x-2"); // counter continues from 2
      });
    });
  });

  describe("getIds / getCount / getActive", () => {
    it("getIds returns all tab ids", () => {
      testInScope(() => {
        mgr._addTab(makeTab("a"));
        mgr._addTab(makeTab("b"));
        expect(mgr.getIds()).toEqual(expect.arrayContaining(["a", "b"]));
        expect(mgr.getIds()).toHaveLength(2);
      });
    });

    it("getCount returns correct count", () => {
      testInScope(() => {
        expect(mgr.getCount()).toBe(0);
        mgr._addTab(makeTab("a"));
        expect(mgr.getCount()).toBe(1);
      });
    });

    it("getActive returns the active tab", () => {
      testInScope(() => {
        mgr._addTab(makeTab("a"));
        mgr._addTab(makeTab("b"));
        mgr.setActive("a");
        expect(mgr.getActive()?.id).toBe("a");
      });
    });

    it("getActive returns undefined when no active tab", () => {
      testInScope(() => {
        expect(mgr.getActive()).toBeUndefined();
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
