import { describe, it, expect, beforeEach } from "vitest";
import { editorTabsStore } from "../../stores/editorTabs";
import { testInScope } from "../helpers/store";

describe("editorTabsStore", () => {
  beforeEach(() => {
    editorTabsStore.clearAll();
  });

  describe("add()", () => {
    it("adds a new tab and returns its id", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "src/main.ts");
        expect(id).toMatch(/^edit-\d+$/);
        expect(editorTabsStore.getCount()).toBe(1);
      });
    });

    it("sets the new tab as active", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "src/main.ts");
        expect(editorTabsStore.state.activeId).toBe(id);
      });
    });

    it("deduplicates: returns existing id when same repoPath+filePath", () => {
      testInScope(() => {
        const id1 = editorTabsStore.add("/repo", "src/main.ts");
        const id2 = editorTabsStore.add("/repo", "src/main.ts");
        expect(id1).toBe(id2);
        expect(editorTabsStore.getCount()).toBe(1);
      });
    });

    it("deduplicates: activates the existing tab", () => {
      testInScope(() => {
        const id1 = editorTabsStore.add("/repo", "src/main.ts");
        editorTabsStore.add("/repo", "src/other.ts");
        editorTabsStore.add("/repo", "src/main.ts"); // re-add first
        expect(editorTabsStore.state.activeId).toBe(id1);
      });
    });

    it("different repoPath creates separate tabs", () => {
      testInScope(() => {
        editorTabsStore.add("/repo-a", "src/main.ts");
        editorTabsStore.add("/repo-b", "src/main.ts");
        expect(editorTabsStore.getCount()).toBe(2);
      });
    });

    it("sets fileName to basename of filePath", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "src/components/Foo.tsx");
        expect(editorTabsStore.get(id)?.fileName).toBe("Foo.tsx");
      });
    });

    it("sets isDirty to false on add", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "file.ts");
        expect(editorTabsStore.get(id)?.isDirty).toBe(false);
      });
    });
  });

  describe("remove()", () => {
    it("removes the tab", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.remove(id);
        expect(editorTabsStore.get(id)).toBeUndefined();
      });
    });

    it("falls back to last remaining tab as active", () => {
      testInScope(() => {
        const id1 = editorTabsStore.add("/repo", "a.ts");
        const id2 = editorTabsStore.add("/repo", "b.ts");
        editorTabsStore.remove(id2); // remove active
        expect(editorTabsStore.state.activeId).toBe(id1);
      });
    });

    it("sets activeId to null when last tab removed", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.remove(id);
        expect(editorTabsStore.state.activeId).toBeNull();
      });
    });

    it("does not change activeId when removing a non-active tab", () => {
      testInScope(() => {
        const id1 = editorTabsStore.add("/repo", "a.ts");
        const id2 = editorTabsStore.add("/repo", "b.ts");
        editorTabsStore.setActive(id2);
        editorTabsStore.remove(id1); // remove non-active
        expect(editorTabsStore.state.activeId).toBe(id2);
      });
    });
  });

  describe("clearForRepo()", () => {
    it("removes all tabs for the given repo", () => {
      testInScope(() => {
        editorTabsStore.add("/repo-a", "a.ts");
        editorTabsStore.add("/repo-a", "b.ts");
        const bId = editorTabsStore.add("/repo-b", "c.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.getCount()).toBe(1);
        expect(editorTabsStore.getIds()[0]).toBe(bId);
      });
    });

    it("sets activeId to null when active tab was in cleared repo", () => {
      testInScope(() => {
        editorTabsStore.add("/repo-a", "a.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.state.activeId).toBeNull();
      });
    });

    it("does not change activeId when active tab is in another repo", () => {
      testInScope(() => {
        editorTabsStore.add("/repo-a", "a.ts");
        const id2 = editorTabsStore.add("/repo-b", "b.ts");
        editorTabsStore.clearForRepo("/repo-a");
        expect(editorTabsStore.state.activeId).toBe(id2);
      });
    });
  });

  describe("getActive()", () => {
    it("returns undefined when no active tab", () => {
      testInScope(() => {
        expect(editorTabsStore.getActive()).toBeUndefined();
      });
    });

    it("returns the active tab data", () => {
      testInScope(() => {
        editorTabsStore.add("/repo", "main.ts");
        const active = editorTabsStore.getActive();
        expect(active?.filePath).toBe("main.ts");
      });
    });
  });

  describe("setDirty()", () => {
    it("marks a tab dirty", () => {
      testInScope(() => {
        const id = editorTabsStore.add("/repo", "file.ts");
        editorTabsStore.setDirty(id, true);
        expect(editorTabsStore.get(id)?.isDirty).toBe(true);
      });
    });

    it("ignores unknown tab id", () => {
      testInScope(() => {
        expect(() => editorTabsStore.setDirty("nonexistent", true)).not.toThrow();
      });
    });
  });

  describe("worktree fsRoot support", () => {
    const REPO = "/Users/dev/myrepo";
    const WORKTREE = "/Users/dev/myrepo/.claude/worktrees/feat-x";
    const BRANCH_KEY = `${REPO}|feat-x`;

    it("tab opened with worktree path as repoPath is invisible (the bug)", () => {
      testInScope(() => {
        editorTabsStore.add(WORKTREE, "src/main.ts");
        const visible = editorTabsStore.getVisibleIds(BRANCH_KEY);
        // Worktree path doesn't match canonical repo in branchKey → filtered out
        expect(visible).toHaveLength(0);
      });
    });

    it("tab with canonical repoPath + fsRoot opt is visible", () => {
      testInScope(() => {
        editorTabsStore.add(REPO, "src/main.ts", undefined, { fsRoot: WORKTREE });
        const visible = editorTabsStore.getVisibleIds(BRANCH_KEY);
        expect(visible).toHaveLength(1);
      });
    });

    it("fsRoot defaults to repoPath when omitted", () => {
      testInScope(() => {
        editorTabsStore.add(REPO, "src/main.ts");
        const tab = editorTabsStore.getActive();
        expect(tab?.fsRoot).toBe(REPO);
      });
    });

    it("same file from different worktrees creates separate tabs", () => {
      testInScope(() => {
        const WORKTREE_B = "/Users/dev/myrepo/.claude/worktrees/feat-y";
        const id1 = editorTabsStore.add(REPO, "src/main.ts", undefined, { fsRoot: WORKTREE });
        const id2 = editorTabsStore.add(REPO, "src/main.ts", undefined, { fsRoot: WORKTREE_B });
        expect(id1).not.toBe(id2);
        expect(editorTabsStore.getCount()).toBe(2);
      });
    });

    it("same file + same fsRoot deduplicates", () => {
      testInScope(() => {
        const id1 = editorTabsStore.add(REPO, "src/main.ts", undefined, { fsRoot: WORKTREE });
        const id2 = editorTabsStore.add(REPO, "src/main.ts", undefined, { fsRoot: WORKTREE });
        expect(id1).toBe(id2);
        expect(editorTabsStore.getCount()).toBe(1);
      });
    });
  });
});
