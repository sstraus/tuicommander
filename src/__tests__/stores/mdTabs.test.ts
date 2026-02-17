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
        expect(store.get(id)?.filePath).toBe("docs/README.md");
        expect(store.get(id)?.fileName).toBe("README.md");
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
  });

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
  });

  describe("clearAll()", () => {
    it("removes all tabs", () => {
      createRoot((dispose) => {
        store.add("/repo", "a.md");
        store.add("/repo", "b.md");
        store.clearAll();
        expect(store.getCount()).toBe(0);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });
  });

  describe("getForRepo()", () => {
    it("filters tabs by repo", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.md");
        store.add("/repo1", "b.md");
        store.add("/repo2", "c.md");
        expect(store.getForRepo("/repo1")).toHaveLength(2);
        expect(store.getForRepo("/repo2")).toHaveLength(1);
        dispose();
      });
    });
  });
});
