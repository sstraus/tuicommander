import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

describe("diffTabsStore", () => {
  let store: typeof import("../../stores/diffTabs").diffTabsStore;

  beforeEach(async () => {
    vi.resetModules();
    store = (await import("../../stores/diffTabs")).diffTabsStore;
  });

  describe("add()", () => {
    it("adds a diff tab and sets it active", () => {
      createRoot((dispose) => {
        const id = store.add("/repo", "src/main.ts", "M");
        expect(id).toBe("diff-1");
        expect(store.state.activeId).toBe(id);
        expect(store.get(id)?.filePath).toBe("src/main.ts");
        expect(store.get(id)?.fileName).toBe("main.ts");
        expect(store.get(id)?.status).toBe("M");
        dispose();
      });
    });

    it("deduplicates by repoPath + filePath", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo", "src/main.ts", "M");
        const id2 = store.add("/repo", "src/main.ts", "M");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
        dispose();
      });
    });

    it("allows same file from different repos", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo1", "src/main.ts", "M");
        const id2 = store.add("/repo2", "src/main.ts", "M");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes a tab and selects another", () => {
      createRoot((dispose) => {
        const id1 = store.add("/repo", "a.ts", "M");
        const id2 = store.add("/repo", "b.ts", "A");
        store.remove(id2);
        expect(store.get(id2)).toBeUndefined();
        expect(store.state.activeId).toBe(id1);
        dispose();
      });
    });

    it("clears activeId when last tab removed", () => {
      createRoot((dispose) => {
        const id = store.add("/repo", "a.ts", "M");
        store.remove(id);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });
  });

  describe("clearForRepo()", () => {
    it("removes only tabs for specified repo", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.ts", "M");
        const id2 = store.add("/repo2", "b.ts", "A");
        store.clearForRepo("/repo1");
        expect(store.getCount()).toBe(1);
        expect(store.get(id2)).toBeDefined();
        dispose();
      });
    });

    it("clears activeId when active tab is removed", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.ts", "M");
        // Active is now the repo1 tab
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });

    it("keeps activeId when active tab is NOT removed", () => {
      createRoot((dispose) => {
        store.add("/repo1", "a.ts", "M");
        const id2 = store.add("/repo2", "b.ts", "A");
        // Active is now id2 (last added)
        expect(store.state.activeId).toBe(id2);
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBe(id2); // preserved
        dispose();
      });
    });
  });

  describe("clearAll()", () => {
    it("removes all tabs", () => {
      createRoot((dispose) => {
        store.add("/repo", "a.ts", "M");
        store.add("/repo", "b.ts", "A");
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
        store.add("/repo1", "a.ts", "M");
        store.add("/repo1", "b.ts", "A");
        store.add("/repo2", "c.ts", "D");
        expect(store.getForRepo("/repo1")).toHaveLength(2);
        expect(store.getForRepo("/repo2")).toHaveLength(1);
        dispose();
      });
    });
  });
});
