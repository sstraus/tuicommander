import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope } from "../helpers/store";

describe("mdTabsStore", () => {
  let store: typeof import("../../stores/mdTabs").mdTabsStore;
  let uiStore: typeof import("../../stores/ui").uiStore;

  beforeEach(async () => {
    vi.resetModules();
    store = (await import("../../stores/mdTabs")).mdTabsStore;
    uiStore = (await import("../../stores/ui")).uiStore;
  });

  describe("add()", () => {
    it("adds a markdown tab and sets it active", () => {
      testInScope(() => {
        const id = store.add("/repo", "docs/README.md");
        expect(id).toBe("md-1");
        expect(store.state.activeId).toBe(id);
        const tab = store.get(id);
        expect(tab?.type).toBe("file");
        if (tab?.type === "file") {
          expect(tab.filePath).toBe("docs/README.md");
          expect(tab.fileName).toBe("README.md");
        }
      });
    });

    it("deduplicates by repoPath + filePath", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "README.md");
        const id2 = store.add("/repo", "README.md");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
      });
    });

    it("allows same file from different repos", () => {
      testInScope(() => {
        const id1 = store.add("/repo1", "README.md");
        const id2 = store.add("/repo2", "README.md");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
      });
    });

    it("stores fsRoot when provided (worktree path)", () => {
      testInScope(() => {
        const id = store.add("/repo", "README.md", "/repo/.worktrees/feature");
        const tab = store.get(id);
        expect(tab?.type).toBe("file");
        if (tab?.type === "file") {
          expect(tab.repoPath).toBe("/repo");
          expect(tab.fsRoot).toBe("/repo/.worktrees/feature");
        }
      });
    });

    it("defaults fsRoot to repoPath when not provided", () => {
      testInScope(() => {
        const id = store.add("/repo", "README.md");
        const tab = store.get(id);
        if (tab?.type === "file") {
          expect(tab.fsRoot).toBe("/repo");
        }
      });
    });

    it("deduplicates by fsRoot + filePath (same repo, different worktrees)", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
        const id2 = store.add("/repo", "README.md", "/repo/.worktrees/feat-b");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
      });
    });

    it("deduplicates same worktree + filePath", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
        const id2 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
      });
    });
  });

  // -------------------------------------------------------------------------
  // Virtual tabs
  // -------------------------------------------------------------------------
  describe("addVirtual()", () => {
    it("adds a virtual tab and sets it active", () => {
      testInScope(() => {
        const id = store.addVirtual("Active Plan", "plan:file?path=/foo.md");
        expect(store.state.activeId).toBe(id);
        const tab = store.get(id);
        expect(tab?.type).toBe("virtual");
        if (tab?.type === "virtual") {
          expect(tab.title).toBe("Active Plan");
          expect(tab.contentUri).toBe("plan:file?path=/foo.md");
        }
      });
    });

    it("deduplicates by contentUri", () => {
      testInScope(() => {
        const id1 = store.addVirtual("Plan", "plan:file?path=/foo.md");
        const id2 = store.addVirtual("Plan", "plan:file?path=/foo.md");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
      });
    });

    it("allows different contentUris as separate tabs", () => {
      testInScope(() => {
        const id1 = store.addVirtual("Plan A", "plan:file?path=/a.md");
        const id2 = store.addVirtual("Plan B", "plan:file?path=/b.md");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
      });
    });

    it("can coexist with file tabs", () => {
      testInScope(() => {
        store.add("/repo", "a.md");
        store.addVirtual("Stories", "stories:detail?id=1");
        expect(store.getCount()).toBe(2);
      });
    });

    it("sets repoPath on virtual tab when provided", () => {
      testInScope(() => {
        const id = store.addVirtual("Plan", "plan:file?path=/repo1/plan.md", "/repo1");
        const tab = store.get(id);
        expect(tab?.repoPath).toBe("/repo1");
      });
    });

    it("virtual tab without repoPath has no repoPath field", () => {
      testInScope(() => {
        const id = store.addVirtual("Global", "stories:detail?id=1");
        const tab = store.get(id);
        expect(tab?.repoPath).toBeUndefined();
      });
    });
  });

  // -------------------------------------------------------------------------
  // remove()
  // -------------------------------------------------------------------------
  describe("remove()", () => {
    it("removes a tab and selects another", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "a.md");
        const id2 = store.add("/repo", "b.md");
        store.remove(id2);
        expect(store.get(id2)).toBeUndefined();
        expect(store.state.activeId).toBe(id1);
      });
    });

    it("clears activeId when last tab removed", () => {
      testInScope(() => {
        const id = store.add("/repo", "a.md");
        store.remove(id);
        expect(store.state.activeId).toBeNull();
      });
    });

    it("removes virtual tabs correctly", () => {
      testInScope(() => {
        const id = store.addVirtual("Plan", "plan:x");
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
      });
    });
  });

  // -------------------------------------------------------------------------
  // clearForRepo()
  // -------------------------------------------------------------------------
  describe("clearForRepo()", () => {
    it("removes only tabs for specified repo", () => {
      testInScope(() => {
        store.add("/repo1", "a.md");
        const id2 = store.add("/repo2", "b.md");
        store.clearForRepo("/repo1");
        expect(store.getCount()).toBe(1);
        expect(store.get(id2)).toBeDefined();
      });
    });

    it("clears activeId when active tab is removed", () => {
      testInScope(() => {
        store.add("/repo1", "a.md");
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBeNull();
      });
    });

    it("keeps activeId when active tab is NOT removed", () => {
      testInScope(() => {
        store.add("/repo1", "a.md");
        const id2 = store.add("/repo2", "b.md");
        expect(store.state.activeId).toBe(id2);
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBe(id2);
      });
    });

    it("does not remove virtual tabs when clearing a repo", () => {
      testInScope(() => {
        store.add("/repo1", "a.md");
        const vId = store.addVirtual("Stories", "stories:x");
        store.clearForRepo("/repo1");
        expect(store.get(vId)).toBeDefined();
      });
    });
  });

  // -------------------------------------------------------------------------
  // clearAll()
  // -------------------------------------------------------------------------
  describe("clearAll()", () => {
    it("removes all tabs including virtual", () => {
      testInScope(() => {
        store.add("/repo", "a.md");
        store.add("/repo", "b.md");
        store.addVirtual("Plan", "plan:x");
        store.clearAll();
        expect(store.getCount()).toBe(0);
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  // -------------------------------------------------------------------------
  // getForRepo()
  // -------------------------------------------------------------------------
  describe("getForRepo()", () => {
    it("filters tabs by repo, excludes virtual tabs", () => {
      testInScope(() => {
        store.add("/repo1", "a.md");
        store.add("/repo1", "b.md");
        store.add("/repo2", "c.md");
        store.addVirtual("Plan", "plan:x");
        expect(store.getForRepo("/repo1")).toHaveLength(2);
        expect(store.getForRepo("/repo2")).toHaveLength(1);
      });
    });
  });

  // -------------------------------------------------------------------------
  // Auto-show markdown panel
  // -------------------------------------------------------------------------
  // -------------------------------------------------------------------------
  // PR Diff tabs
  // -------------------------------------------------------------------------
  describe("addPrDiff()", () => {
    it("adds a pr-diff tab and sets it active", () => {
      testInScope(() => {
        const id = store.addPrDiff("/repo", 42, "Fix bug", "diff --git a/f.ts b/f.ts\n-old\n+new");
        expect(store.state.activeId).toBe(id);
        const tab = store.get(id);
        expect(tab?.type).toBe("pr-diff");
        if (tab?.type === "pr-diff") {
          expect(tab.prNumber).toBe(42);
          expect(tab.prTitle).toBe("Fix bug");
          expect(tab.diff).toContain("diff --git");
        }
      });
    });

    it("deduplicates by repoPath + prNumber", () => {
      testInScope(() => {
        const id1 = store.addPrDiff("/repo", 42, "Fix bug", "diff1");
        const id2 = store.addPrDiff("/repo", 42, "Fix bug", "diff2");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
      });
    });

    it("allows different PR numbers as separate tabs", () => {
      testInScope(() => {
        const id1 = store.addPrDiff("/repo", 42, "Fix A", "diff1");
        const id2 = store.addPrDiff("/repo", 43, "Fix B", "diff2");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
      });
    });

    it("updates diff content when reopening same PR", () => {
      testInScope(() => {
        store.addPrDiff("/repo", 42, "Fix bug", "old diff");
        const id = store.addPrDiff("/repo", 42, "Fix bug", "new diff");
        const tab = store.get(id);
        if (tab?.type === "pr-diff") {
          expect(tab.diff).toBe("new diff");
        }
      });
    });
  });

  describe("does not auto-show markdown file browser", () => {
    it("does not open file browser when adding a file tab", () => {
      testInScope(() => {
        expect(uiStore.state.markdownPanelVisible).toBe(false);
        store.add("/repo", "README.md");
        expect(uiStore.state.markdownPanelVisible).toBe(false);
      });
    });

    it("does not open file browser when adding a virtual tab", () => {
      testInScope(() => {
        expect(uiStore.state.markdownPanelVisible).toBe(false);
        store.addVirtual("Plan", "plan:file?path=/foo.md");
        expect(uiStore.state.markdownPanelVisible).toBe(false);
      });
    });

    it("does not open file browser when adding claude usage tab", () => {
      testInScope(() => {
        expect(uiStore.state.markdownPanelVisible).toBe(false);
        store.addClaudeUsage();
        expect(uiStore.state.markdownPanelVisible).toBe(false);
      });
    });
  });

  describe("openUiTab()", () => {
    it("creates a new plugin-panel tab with the given id", () => {
      testInScope(() => {
        const tabId = store.openUiTab("wiz-coverage", "Coverage Report", "<h1>Report</h1>", true);
        expect(tabId).toBeTruthy();
        expect(store.state.activeId).toBe(tabId);
        const tab = store.get(tabId);
        expect(tab?.type).toBe("plugin-panel");
        if (tab?.type === "plugin-panel") {
          expect(tab.pluginId).toBe("wiz-coverage");
          expect(tab.title).toBe("Coverage Report");
          expect(tab.html).toBe("<h1>Report</h1>");
          expect(tab.pinned).toBe(true);
        }
      });
    });

    it("deduplicates by pluginId — same id reuses existing tab", () => {
      testInScope(() => {
        const id1 = store.openUiTab("wiz-report", "Report v1", "<p>v1</p>", true);
        const id2 = store.openUiTab("wiz-report", "Report v2", "<p>v2</p>", true);
        expect(id1).toBe(id2);
        expect(store.getCount()).toBe(1);
        const tab = store.get(id1);
        if (tab?.type === "plugin-panel") {
          expect(tab.title).toBe("Report v2");
          expect(tab.html).toBe("<p>v2</p>");
        }
      });
    });

    it("respects pinned=false", () => {
      testInScope(() => {
        const tabId = store.openUiTab("wiz-temp", "Temp", "<p>temp</p>", false);
        const tab = store.get(tabId);
        expect(tab?.pinned).toBe(false);
      });
    });

    it("different ids create separate tabs", () => {
      testInScope(() => {
        store.openUiTab("wiz-a", "Tab A", "<p>A</p>", true);
        store.openUiTab("wiz-b", "Tab B", "<p>B</p>", true);
        expect(store.getCount()).toBe(2);
      });
    });

    it("focus=true (default) sets the new tab as active", () => {
      testInScope(() => {
        const id = store.openUiTab("wiz-x", "X", "<p/>", true, undefined, true);
        expect(store.state.activeId).toBe(id);
      });
    });

    it("focus=false does not change activeId when creating a new tab", () => {
      testInScope(() => {
        const existing = store.add("/repo", "README.md");
        expect(store.state.activeId).toBe(existing);
        store.openUiTab("wiz-bg", "BG", "<p/>", true, undefined, false);
        expect(store.state.activeId).toBe(existing);
      });
    });

    it("focus=false does not change activeId when updating an existing tab", () => {
      testInScope(() => {
        store.openUiTab("wiz-mc", "MC v1", "<p>v1</p>", true);
        const other = store.add("/repo", "README.md");
        expect(store.state.activeId).toBe(other);
        store.openUiTab("wiz-mc", "MC v2", "<p>v2</p>", true, undefined, false);
        expect(store.state.activeId).toBe(other);
        // content still updated
        const tab = store.get(store.getIds().find((id) => store.get(id)?.type === "plugin-panel")!);
        if (tab?.type === "plugin-panel") expect(tab.html).toBe("<p>v2</p>");
      });
    });
  });
});
