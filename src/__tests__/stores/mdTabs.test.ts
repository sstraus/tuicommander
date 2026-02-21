import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

describe("mdTabsStore", () => {
  let store: typeof import("../../stores/mdTabs").mdTabsStore;

  beforeEach(async () => {
    vi.resetModules();
    store = (await import("../../stores/mdTabs")).mdTabsStore;
  });

  describe("add()", () => {
    it("adds a markdown tab and sets it active", () => {
      createRoot((dispose) => {
        const id = store.add("/repo", "docs/README.md");
        expect(id).toBe("md-1");
        expect(store.state.activeId).toBe(id);
        const tab = store.get(id);
        expect(tab?.type).toBe("file");
        if (tab?.type === "file") {
          expect(tab.filePath).toBe("docs/README.md");
          expect(tab.fileName).toBe("README.md");
        }
        dispose();
      });
    });

    it("deduplicates by repoPath + filePath", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo", "README.md");
        const id2 = store.add("/repo", "README.md");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
        dispose();
      });
    });

    it("allows same file from different repos", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo1", "README.md");
        const id2 = store.add("/repo2", "README.md");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
        dispose();
      });
    });
  });

  // -------------------------------------------------------------------------
  // Virtual tabs
  // -------------------------------------------------------------------------
  describe("addVirtual()", () => {
    it("adds a virtual tab and sets it active", () => {
      createRoot((dispose) => {
        const id = store.addVirtual("Active Plan", "plan:file?path=/foo.md");
        expect(store.state.activeId).toBe(id);
        const tab = store.get(id);
        expect(tab?.type).toBe("virtual");
        if (tab?.type === "virtual") {
          expect(tab.title).toBe("Active Plan");
          expect(tab.contentUri).toBe("plan:file?path=/foo.md");
        }
        dispose();
      });
    });

    it("deduplicates by contentUri", () => {
      createRoot((dispose) => {
        const id1 = store.addVirtual("Plan", "plan:file?path=/foo.md");
        const id2 = store.addVirtual("Plan", "plan:file?path=/foo.md");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
        dispose();
      });
    });

    it("allows different contentUris as separate tabs", () => {
      createRoot((dispose) => {
        const id1 = store.addVirtual("Plan A", "plan:file?path=/a.md");
        const id2 = store.addVirtual("Plan B", "plan:file?path=/b.md");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
        dispose();
      });
    });

    it("can coexist with file tabs", () => {
      createRoot((dispose) => {
        store.add("/repo", "a.md");
        store.addVirtual("Stories", "stories:detail?id=1");
        expect(store.getCount()).toBe(2);
        dispose();
      });
    });
  });

  // -------------------------------------------------------------------------
  // remove()
  // -------------------------------------------------------------------------
  describe("remove()", () => {
    it("removes a tab and selects another", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo", "a.md");
        const id2 = store.add("/repo", "b.md");
        store.remove(id2);
        expect(store.get(id2)).toBeUndefined();
        expect(store.state.activeId).toBe(id1);
        dispose();
      });
    });

    it("clears activeId when last tab removed", () => {
      createRoot((dispose) => {
        const id = store.add("/repo", "a.md");
        store.remove(id);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });

    it("removes virtual tabs correctly", () => {
      createRoot((dispose) => {
        const id = store.addVirtual("Plan", "plan:x");
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
        dispose();
      });
    });
  });

  // -------------------------------------------------------------------------
  // clearForRepo()
  // -------------------------------------------------------------------------
  describe("clearForRepo()", () => {
    it("removes only tabs for specified repo", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        const id2 = store.add("/repo2", "b.md");
        store.clearForRepo("/repo1");
        expect(store.getCount()).toBe(1);
        expect(store.get(id2)).toBeDefined();
        dispose();
      });
    });

    it("clears activeId when active tab is removed", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });

    it("keeps activeId when active tab is NOT removed", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        const id2 = store.add("/repo2", "b.md");
        expect(store.state.activeId).toBe(id2);
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBe(id2);
        dispose();
      });
    });

    it("does not remove virtual tabs when clearing a repo", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        const vId = store.addVirtual("Stories", "stories:x");
        store.clearForRepo("/repo1");
        expect(store.get(vId)).toBeDefined();
        dispose();
      });
    });
  });

  // -------------------------------------------------------------------------
  // clearAll()
  // -------------------------------------------------------------------------
  describe("clearAll()", () => {
    it("removes all tabs including virtual", () => {
      createRoot((dispose) => {
        store.add("/repo", "a.md");
        store.add("/repo", "b.md");
        store.addVirtual("Plan", "plan:x");
        store.clearAll();
        expect(store.getCount()).toBe(0);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });
  });

  // -------------------------------------------------------------------------
  // getForRepo()
  // -------------------------------------------------------------------------
  describe("getForRepo()", () => {
    it("filters tabs by repo, excludes virtual tabs", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        store.add("/repo1", "b.md");
        store.add("/repo2", "c.md");
        store.addVirtual("Plan", "plan:x");
        expect(store.getForRepo("/repo1")).toHaveLength(2);
        expect(store.getForRepo("/repo2")).toHaveLength(1);
        dispose();
      });
    });
  });
});
