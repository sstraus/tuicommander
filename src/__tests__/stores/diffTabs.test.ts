import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope } from "../helpers/store";

describe("diffTabsStore", () => {
  let store: typeof import("../../stores/diffTabs").diffTabsStore;

  beforeEach(async () => {
    vi.resetModules();
    store = (await import("../../stores/diffTabs")).diffTabsStore;
  });

  describe("add()", () => {
    it("adds a diff tab and sets it active", () => {
      testInScope(() => {
        const id = store.add("/repo", "src/main.ts", "M");
        expect(id).toBe("diff-1");
        expect(store.state.activeId).toBe(id);
        expect(store.get(id)?.filePath).toBe("src/main.ts");
        expect(store.get(id)?.fileName).toBe("main.ts");
        expect(store.get(id)?.status).toBe("M");
      });
    });

    it("deduplicates by repoPath + filePath", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "src/main.ts", "M");
        const id2 = store.add("/repo", "src/main.ts", "M");
        expect(id2).toBe(id1);
        expect(store.getCount()).toBe(1);
      });
    });

    it("allows same file from different repos", () => {
      testInScope(() => {
        const id1 = store.add("/repo1", "src/main.ts", "M");
        const id2 = store.add("/repo2", "src/main.ts", "M");
        expect(id1).not.toBe(id2);
        expect(store.getCount()).toBe(2);
      });
    });
  });

  describe("remove()", () => {
    it("removes a tab and selects another", () => {
      testInScope(() => {
        const id1 = store.add("/repo", "a.ts", "M");
        const id2 = store.add("/repo", "b.ts", "A");
        store.remove(id2);
        expect(store.get(id2)).toBeUndefined();
        expect(store.state.activeId).toBe(id1);
      });
    });

    it("clears activeId when last tab removed", () => {
      testInScope(() => {
        const id = store.add("/repo", "a.ts", "M");
        store.remove(id);
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  describe("clearForRepo()", () => {
    it("removes only tabs for specified repo", () => {
      testInScope(() => {
        store.add("/repo1", "a.ts", "M");
        const id2 = store.add("/repo2", "b.ts", "A");
        store.clearForRepo("/repo1");
        expect(store.getCount()).toBe(1);
        expect(store.get(id2)).toBeDefined();
      });
    });

    it("clears activeId when active tab is removed", () => {
      testInScope(() => {
        store.add("/repo1", "a.ts", "M");
        // Active is now the repo1 tab
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBeNull();
      });
    });

    it("keeps activeId when active tab is NOT removed", () => {
      testInScope(() => {
        store.add("/repo1", "a.ts", "M");
        const id2 = store.add("/repo2", "b.ts", "A");
        // Active is now id2 (last added)
        expect(store.state.activeId).toBe(id2);
        store.clearForRepo("/repo1");
        expect(store.state.activeId).toBe(id2); // preserved
      });
    });
  });

  describe("clearAll()", () => {
    it("removes all tabs", () => {
      testInScope(() => {
        store.add("/repo", "a.ts", "M");
        store.add("/repo", "b.ts", "A");
        store.clearAll();
        expect(store.getCount()).toBe(0);
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  describe("getForRepo()", () => {
    it("filters tabs by repo", () => {
      testInScope(() => {
        store.add("/repo1", "a.ts", "M");
        store.add("/repo1", "b.ts", "A");
        store.add("/repo2", "c.ts", "D");
        expect(store.getForRepo("/repo1")).toHaveLength(2);
        expect(store.getForRepo("/repo2")).toHaveLength(1);
      });
    });
  });
});
