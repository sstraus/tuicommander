import { describe, it, expect, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import { editorTabsStore } from "../../stores/editorTabs";

describe("editorTabsStore", () => {
  beforeEach(() => {
    editorTabsStore.clearAll();
  });

  describe("add()", () => {
    it("adds a new tab and returns its id", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "src/main.ts");
        expect(id).toMatch(/^edit-\d+$/);
        expect(editorTabsStore.getCount()).toBe(1);
        dispose();
      });
    });

    it("sets the new tab as active", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "src/main.ts");
        expect(editorTabsStore.state.activeId).toBe(id);
        dispose();
      });
    });

    it("deduplicates: returns existing id when same repoPath+filePath", () => {
      createRoot((dispose) => {
        const id1 = editorTabsStore.add("/repo", "src/main.ts");
        const id2 = editorTabsStore.add("/repo", "src/main.ts");
        expect(id1).toBe(id2);
        expect(editorTabsStore.getCount()).toBe(1);
        dispose();
      });
    });

    it("deduplicates: activates the existing tab", () => {
      createRoot((dispose) => {
        const id1 = editorTabsStore.add("/repo", "src/main.ts");
        editorTabsStore.add("/repo", "src/other.ts");
        editorTabsStore.add("/repo", "src/main.ts"); // re-add first
        expect(editorTabsStore.state.activeId).toBe(id1);
        dispose();
      });
    });

    it("different repoPath creates separate tabs", () => {
      createRoot((dispose) => {
        editorTabsStore.add("/repo-a", "src/main.ts");
        editorTabsStore.add("/repo-b", "src/main.ts");
        expect(editorTabsStore.getCount()).toBe(2);
        dispose();
      });
    });

    it("sets fileName to basename of filePath", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "src/components/Foo.tsx");
        expect(editorTabsStore.get(id)?.fileName).toBe("Foo.tsx");
        dispose();
      });
    });

    it("sets isDirty to false on add", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "file.ts");
        expect(editorTabsStore.get(id)?.isDirty).toBe(false);
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes the tab", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.remove(id);
        expect(editorTabsStore.get(id)).toBeUndefined();
        dispose();
      });
    });

    it("falls back to last remaining tab as active", () => {
      createRoot((dispose) => {
        const id1 = editorTabsStore.add("/repo", "a.ts");
        const id2 = editorTabsStore.add("/repo", "b.ts");
        editorTabsStore.remove(id2); // remove active
        expect(editorTabsStore.state.activeId).toBe(id1);
        dispose();
      });
    });

    it("sets activeId to null when last tab removed", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.remove(id);
        expect(editorTabsStore.state.activeId).toBeNull();
        dispose();
      });
    });

    it("does not change activeId when removing a non-active tab", () => {
      createRoot((dispose) => {
        const id1 = editorTabsStore.add("/repo", "a.ts");
        const id2 = editorTabsStore.add("/repo", "b.ts");
        editorTabsStore.setActive(id2);
        editorTabsStore.remove(id1); // remove non-active
        expect(editorTabsStore.state.activeId).toBe(id2);
        dispose();
      });
    });
  });

  describe("clearForRepo()", () => {
    it("removes all tabs for the given repo", () => {
      createRoot((dispose) => {
        editorTabsStore.add("/repo-a", "a.ts");
        editorTabsStore.add("/repo-a", "b.ts");
        const bId = editorTabsStore.add("/repo-b", "c.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.getCount()).toBe(1);
        expect(editorTabsStore.getIds()[0]).toBe(bId);
        dispose();
      });
    });

    it("sets activeId to null when active tab was in cleared repo", () => {
      createRoot((dispose) => {
        editorTabsStore.add("/repo-a", "a.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.state.activeId).toBeNull();
        dispose();
      });
    });

    it("does not change activeId when active tab is in another repo", () => {
      createRoot((dispose) => {
        editorTabsStore.add("/repo-a", "a.ts");
        const id2 = editorTabsStore.add("/repo-b", "b.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.state.activeId).toBe(id2);
        dispose();
      });
    });
  });

  describe("getActive()", () => {
    it("returns undefined when no active tab", () => {
      createRoot((dispose) => {
        expect(editorTabsStore.getActive()).toBeUndefined();
        dispose();
      });
    });

    it("returns the active tab data", () => {
      createRoot((dispose) => {
        editorTabsStore.add("/repo", "main.ts");
        const active = editorTabsStore.getActive();
        expect(active?.filePath).toBe("main.ts");
        dispose();
      });
    });
  });

  describe("setDirty()", () => {
    it("marks a tab dirty", () => {
      createRoot((dispose) => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.setDirty(id, true);
        expect(editorTabsStore.get(id)?.isDirty).toBe(true);
        dispose();
      });
    });

    it("ignores unknown tab id", () => {
      createRoot((dispose) => {
        expect(() => editorTabsStore.setDirty("nonexistent", true)).not.toThrow();
        dispose();
      });
    });
  });
});
