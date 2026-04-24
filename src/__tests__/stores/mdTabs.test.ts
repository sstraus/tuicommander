import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("mdTabsStore", () => {
  let store: typeof import("../../stores/mdTabs").mdTabsStore;
  let uiStore: typeof import("../../stores/ui").uiStore;
  let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
  let resolveRepoForCwd: typeof import("../../stores/mdTabs").resolveRepoForCwd;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    const mdTabsMod = await import("../../stores/mdTabs");
    store = mdTabsMod.mdTabsStore;
    resolveRepoForCwd = mdTabsMod.resolveRepoForCwd;
    uiStore = (await import("../../stores/ui")).uiStore;
    repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
    repositoriesStore._testSetHydrated(true);
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

  describe("resolveRepoForCwd()", () => {
    it("returns null for null/undefined/empty input", () => {
      testInScope(() => {
        expect(resolveRepoForCwd(null)).toBeNull();
        expect(resolveRepoForCwd(undefined)).toBeNull();
        expect(resolveRepoForCwd("")).toBeNull();
      });
    });

    it("returns null when no repos registered", () => {
      testInScope(() => {
        expect(resolveRepoForCwd("/some/path")).toBeNull();
      });
    });

    it("returns exact match when cwd equals a registered repo path", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        expect(resolveRepoForCwd("/Gits/alpha")).toBe("/Gits/alpha");
      });
    });

    it("returns the prefix-matching repo when cwd is nested inside it", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        expect(resolveRepoForCwd("/Gits/alpha/src/lib/foo.rs")).toBe("/Gits/alpha");
      });
    });

    it("picks the longest-prefix match for nested repos", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits", displayName: "root" });
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        expect(resolveRepoForCwd("/Gits/alpha/src/main.rs")).toBe("/Gits/alpha");
      });
    });

    it("does not match a path that merely shares a name prefix with a repo", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        // /Gits/alpha-sibling must NOT match /Gits/alpha
        expect(resolveRepoForCwd("/Gits/alpha-sibling/src")).toBeNull();
      });
    });

    it("resolves Windows drive-letter CWD to registered Windows repo", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
        expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane\\src")).toBe("C:\\DATA\\repos\\arcane");
      });
    });

    it("does not match Windows path that merely shares a name prefix", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
        expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane-fork\\src")).toBeNull();
      });
    });

    it("matches mixed separators (forward slash CWD against backslash repo)", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
        expect(resolveRepoForCwd("C:/DATA/repos/arcane/src/lib")).toBe("C:\\DATA\\repos\\arcane");
      });
    });

    it("returns exact match for Windows drive-letter repo", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
        expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane")).toBe("C:\\DATA\\repos\\arcane");
      });
    });
  });

  describe("openUiTab() repo routing", () => {
    it("scopes tab to the origin repo, NOT the active repo, when originRepoPath is given", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");

        const id = store.openUiTab("wiz-panel", "MCF", "<p/>", false, undefined, true, "/Gits/alpha/src");
        const tab = store.get(id);
        expect(tab?.repoPath).toBe("/Gits/alpha");
      });
    });

    it("scopes pinned tabs to the origin repo as well (MCP caller always wins)", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");

        const id = store.openUiTab("wiz-pinned", "Pinned", "<p/>", true, undefined, true, "/Gits/alpha");
        const tab = store.get(id);
        expect(tab?.repoPath).toBe("/Gits/alpha");
      });
    });

    it("falls back to active repo for unpinned tabs when origin cannot be resolved", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");

        const id = store.openUiTab("wiz-unknown", "X", "<p/>", false, undefined, true, "/not/a/registered/repo");
        const tab = store.get(id);
        expect(tab?.repoPath).toBe("/Gits/beta");
      });
    });

    it("leaves pinned tabs globally scoped (no repoPath) when origin is unresolved", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");

        const id = store.openUiTab("wiz-global", "G", "<p/>", true);
        const tab = store.get(id);
        expect(tab?.repoPath).toBeUndefined();
      });
    });
  });

  describe("evictNonPinnedPluginPanelsForOtherRepos() (story 1283-1d9b)", () => {
    // Without eviction, every visited repo leaves a stale non-pinned plugin-panel
    // entry in state.tabs. getVisibleIds already hides them, but the HTML is
    // retained forever. Eviction runs on repo switch, keyed by repoPath.
    it("evicts non-pinned plugin-panel tabs belonging to other repos", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/alpha");
        const aId = store.openUiTab("plug-a", "A", "<p/>", false, undefined, true, "/Gits/alpha");
        repositoriesStore.setActive("/Gits/beta");
        const bId = store.openUiTab("plug-b", "B", "<p/>", false, undefined, true, "/Gits/beta");
        expect(store.getCount()).toBe(2);

        // Switch back to alpha — beta's non-pinned tab must be gone, alpha's still here.
        store.evictNonPinnedPluginPanelsForOtherRepos("/Gits/alpha");
        expect(store.get(aId)).toBeDefined();
        expect(store.get(bId)).toBeUndefined();
      });
    });

    it("preserves pinned plugin-panel tabs regardless of repo", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");
        const pinnedId = store.openUiTab("plug-pin", "Pin", "<p/>", true, undefined, true, "/Gits/beta");

        store.evictNonPinnedPluginPanelsForOtherRepos("/Gits/alpha");
        expect(store.get(pinnedId)).toBeDefined();
      });
    });

    it("does not touch non-plugin-panel tabs (file/virtual/pr-diff untouched)", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");
        const fileId = store.add("/Gits/beta", "docs/README.md");
        const diffId = store.addPrDiff("/Gits/beta", 42, "PR title", "diff");

        store.evictNonPinnedPluginPanelsForOtherRepos("/Gits/alpha");
        expect(store.get(fileId)).toBeDefined();
        expect(store.get(diffId)).toBeDefined();
      });
    });

    it("preserves plugin-panel tabs with no repoPath (globally scoped)", () => {
      testInScope(() => {
        // Pinned + no originRepoPath leaves repoPath undefined → globally visible.
        const globalId = store.openUiTab("plug-global", "Global", "<p/>", true);
        store.evictNonPinnedPluginPanelsForOtherRepos("/Gits/alpha");
        expect(store.get(globalId)).toBeDefined();
      });
    });

    it("clears activeId if it pointed at an evicted tab", () => {
      testInScope(() => {
        repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
        repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
        repositoriesStore.setActive("/Gits/beta");
        const bId = store.openUiTab("plug-b", "B", "<p/>", false, undefined, true, "/Gits/beta");
        expect(store.state.activeId).toBe(bId);

        store.evictNonPinnedPluginPanelsForOtherRepos("/Gits/alpha");
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  describe("closeUiTab()", () => {
    it("removes a plugin-panel tab by pluginId", () => {
      testInScope(() => {
        store.openUiTab("wiz-status", "Status", "<p>ok</p>", true);
        expect(store.getCount()).toBe(1);
        store.closeUiTab("wiz-status");
        expect(store.getCount()).toBe(0);
      });
    });

    it("is a no-op when pluginId does not exist", () => {
      testInScope(() => {
        store.openUiTab("wiz-a", "A", "<p>a</p>", true);
        store.closeUiTab("nonexistent");
        expect(store.getCount()).toBe(1);
      });
    });
  });
});
