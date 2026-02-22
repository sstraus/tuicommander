import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("repositoriesStore", () => {
  let store: typeof import("../../stores/repositories").repositoriesStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/repositories")).repositoriesStore;
    store._testSetHydrated(true);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("add()", () => {
    it("adds a repository", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "my-project" });
        const repo = store.get("/path/to/repo");
        expect(repo).toBeDefined();
        expect(repo!.displayName).toBe("my-project");
        expect(repo!.expanded).toBe(true);
        expect(repo!.collapsed).toBe(false);
        dispose();
      });
    });

    it("stores initials from Rust backend", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/1", displayName: "my-project", initials: "MP" });
        expect(store.get("/path/1")!.initials).toBe("MP");

        store.add({ path: "/path/2", displayName: "app", initials: "AP" });
        expect(store.get("/path/2")!.initials).toBe("AP");
        dispose();
      });
    });

    it("persists via invoke (debounced)", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        vi.advanceTimersByTime(500);
        expect(mockInvoke).toHaveBeenCalledWith("save_repositories", {
          config: expect.objectContaining({
            repos: expect.objectContaining({
              "/path/to/repo": expect.objectContaining({ displayName: "test" }),
            }),
          }),
        });
        dispose();
      });
    });

    it("does not persist terminals via invoke", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        vi.advanceTimersByTime(500);
        // Find the last save_repositories call
        const calls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        );
        const lastCall = calls[calls.length - 1];
        expect(lastCall[1].config.repos["/repo"].branches["main"].terminals).toEqual([]);
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes a repository", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        store.remove("/path/to/repo");
        expect(store.get("/path/to/repo")).toBeUndefined();
        dispose();
      });
    });

    it("clears activeRepoPath if removed repo was active", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        store.setActive("/path/to/repo");
        store.remove("/path/to/repo");
        expect(store.state.activeRepoPath).toBeNull();
        dispose();
      });
    });
  });

  describe("setActive()", () => {
    it("sets active repository", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        store.setActive("/path/to/repo");
        expect(store.state.activeRepoPath).toBe("/path/to/repo");
        dispose();
      });
    });
  });

  describe("toggleExpanded()", () => {
    it("toggles expanded state", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        expect(store.get("/path/to/repo")!.expanded).toBe(true);
        store.toggleExpanded("/path/to/repo");
        expect(store.get("/path/to/repo")!.expanded).toBe(false);
        dispose();
      });
    });
  });

  describe("branches", () => {
    it("setBranch creates a new branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "feature/test");
        const repo = store.get("/repo")!;
        expect(repo.branches["feature/test"]).toBeDefined();
        expect(repo.branches["feature/test"].name).toBe("feature/test");
        expect(repo.branches["feature/test"].isMain).toBe(false);
        dispose();
      });
    });

    it("setBranch detects main branches", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        expect(store.get("/repo")!.branches["main"].isMain).toBe(true);

        store.setBranch("/repo", "master");
        expect(store.get("/repo")!.branches["master"].isMain).toBe(true);

        store.setBranch("/repo", "develop");
        expect(store.get("/repo")!.branches["develop"].isMain).toBe(true);
        dispose();
      });
    });

    it("setBranch updates existing branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.setBranch("/repo", "main", { additions: 5, deletions: 3 });
        expect(store.get("/repo")!.branches["main"].additions).toBe(5);
        dispose();
      });
    });

    it("setActiveBranch sets the active branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.setActiveBranch("/repo", "main");
        expect(store.get("/repo")!.activeBranch).toBe("main");
        dispose();
      });
    });
  });

  describe("terminal-branch association", () => {
    it("addTerminalToBranch adds terminal", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        expect(store.get("/repo")!.branches["main"].terminals).toContain("term-1");
        dispose();
      });
    });

    it("addTerminalToBranch prevents duplicates", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        store.addTerminalToBranch("/repo", "main", "term-1");
        expect(store.get("/repo")!.branches["main"].terminals).toHaveLength(1);
        dispose();
      });
    });

    it("removeTerminalFromBranch removes terminal", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        store.removeTerminalFromBranch("/repo", "main", "term-1");
        expect(store.get("/repo")!.branches["main"].terminals).toHaveLength(0);
        dispose();
      });
    });
  });

  describe("removeBranch()", () => {
    it("removes a branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "feature");
        store.removeBranch("/repo", "feature");
        expect(store.get("/repo")!.branches["feature"]).toBeUndefined();
        dispose();
      });
    });

    it("updates activeBranch when removed branch was active", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.setBranch("/repo", "feature");
        store.setActiveBranch("/repo", "feature");
        store.removeBranch("/repo", "feature");
        expect(store.get("/repo")!.activeBranch).toBe("main");
        dispose();
      });
    });
  });

  describe("renameBranch()", () => {
    it("renames a branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "old-name");
        store.addTerminalToBranch("/repo", "old-name", "term-1");
        store.renameBranch("/repo", "old-name", "new-name");
        expect(store.get("/repo")!.branches["old-name"]).toBeUndefined();
        expect(store.get("/repo")!.branches["new-name"]).toBeDefined();
        expect(store.get("/repo")!.branches["new-name"].terminals).toContain("term-1");
        dispose();
      });
    });

    it("updates activeBranch when renamed branch was active", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "old-name");
        store.setActiveBranch("/repo", "old-name");
        store.renameBranch("/repo", "old-name", "new-name");
        expect(store.get("/repo")!.activeBranch).toBe("new-name");
        dispose();
      });
    });
  });

  describe("getActive()", () => {
    it("returns active repo", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setActive("/repo");
        expect(store.getActive()?.path).toBe("/repo");
        dispose();
      });
    });

    it("returns undefined when no active", () => {
      createRoot((dispose) => {
        expect(store.getActive()).toBeUndefined();
        dispose();
      });
    });
  });

  describe("getPaths()", () => {
    it("returns all repo paths", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo1", displayName: "test1" });
        store.add({ path: "/repo2", displayName: "test2" });
        expect(store.getPaths()).toEqual(["/repo1", "/repo2"]);
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads repos from Rust backend and clears stale terminals", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {
          "/repo": {
            path: "/repo",
            displayName: "test",
            initials: "TE",
            expanded: true,
            collapsed: false,
            branches: {
              main: {
                name: "main",
                isMain: true,
                worktreePath: "/repo",
                terminals: ["stale-term-1", "stale-term-2"],
                additions: 0,
                deletions: 0,
              },
            },
            activeBranch: "main",
          },
        },
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        const repo = store.get("/repo");
        expect(repo).toBeDefined();
        expect(repo!.branches["main"].terminals).toEqual([]);
        expect(mockInvoke).toHaveBeenCalledWith("load_repositories");
        dispose();
      });
    });

    it("migrates expanded/collapsed fields when missing", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {
          "/repo": {
            path: "/repo",
            displayName: "test",
            initials: "TE",
            branches: {
              main: {
                name: "main",
                isMain: true,
                worktreePath: "/repo",
                terminals: [],
                additions: 0,
                deletions: 0,
              },
            },
            activeBranch: "main",
            // No collapsed or expanded fields — migration should add them
          },
        },
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        const repo = store.get("/repo");
        expect(repo).toBeDefined();
        expect(repo!.collapsed).toBe(false);
        expect(repo!.expanded).toBe(true);
        dispose();
      });
    });

    it("handles hydration failure gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("load failed"));
      const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.hydrate(); // Should not throw
        expect(store.getPaths()).toEqual([]);
        debugSpy.mockRestore();
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      const staleData = {
        "/repo": {
          path: "/repo", displayName: "test", initials: "TE",
          expanded: true, collapsed: false, branches: {}, activeBranch: null,
        },
      };
      localStorage.setItem("tui-commander-repos", JSON.stringify(staleData));
      mockInvoke.mockResolvedValueOnce(undefined); // save_repositories migration
      mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repositories

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-repos")).toBeNull();
        dispose();
      });
    });
  });

  describe("toggleCollapsed()", () => {
    it("toggles collapsed state", () => {
      createRoot((dispose) => {
        store.add({ path: "/path/to/repo", displayName: "test" });
        expect(store.get("/path/to/repo")!.collapsed).toBe(false);
        store.toggleCollapsed("/path/to/repo");
        expect(store.get("/path/to/repo")!.collapsed).toBe(true);
        store.toggleCollapsed("/path/to/repo");
        expect(store.get("/path/to/repo")!.collapsed).toBe(false);
        dispose();
      });
    });
  });

  describe("isEmpty()", () => {
    it("returns true when empty", () => {
      createRoot((dispose) => {
        expect(store.isEmpty()).toBe(true);
        dispose();
      });
    });

    it("returns false when repos exist", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        expect(store.isEmpty()).toBe(false);
        dispose();
      });
    });
  });

  describe("reorderTerminals()", () => {
    it("reorders terminals in a branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        store.addTerminalToBranch("/repo", "main", "term-2");
        store.addTerminalToBranch("/repo", "main", "term-3");
        store.reorderTerminals("/repo", "main", 0, 2);
        expect(store.get("/repo")!.branches["main"].terminals).toEqual(["term-2", "term-3", "term-1"]);
        dispose();
      });
    });
  });

  describe("getActiveTerminals()", () => {
    it("returns terminals for active branch", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setActive("/repo");
        store.setBranch("/repo", "main");
        store.setActiveBranch("/repo", "main");
        store.addTerminalToBranch("/repo", "main", "term-1");
        expect(store.getActiveTerminals()).toEqual(["term-1"]);
        dispose();
      });
    });

    it("returns empty when no active repo", () => {
      createRoot((dispose) => {
        expect(store.getActiveTerminals()).toEqual([]);
        dispose();
      });
    });
  });

  describe("groups — CRUD", () => {
    it("initializes with empty groups and groupOrder", () => {
      createRoot((dispose) => {
        expect(store.state.groups).toEqual({});
        expect(store.state.groupOrder).toEqual([]);
        dispose();
      });
    });

    it("createGroup() adds a group and returns its ID", () => {
      createRoot((dispose) => {
        const id = store.createGroup("Work")!;
        expect(id).toBeTruthy();
        expect(store.state.groups[id]).toBeDefined();
        expect(store.state.groups[id].name).toBe("Work");
        expect(store.state.groups[id].color).toBe("");
        expect(store.state.groups[id].collapsed).toBe(false);
        expect(store.state.groups[id].repoOrder).toEqual([]);
        expect(store.state.groupOrder).toContain(id);
        dispose();
      });
    });

    it("createGroup() enforces unique names (case-insensitive)", () => {
      createRoot((dispose) => {
        store.createGroup("Work");
        expect(store.createGroup("work")).toBeNull();
        expect(store.createGroup("WORK")).toBeNull();
        dispose();
      });
    });

    it("deleteGroup() removes group and moves repos to ungrouped", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        const id = store.createGroup("Work")!;
        store.addRepoToGroup("/repo-a", id);
        expect(store.state.repoOrder).not.toContain("/repo-a");
        store.deleteGroup(id);
        expect(store.state.groups[id]).toBeUndefined();
        expect(store.state.groupOrder).not.toContain(id);
        expect(store.state.repoOrder).toContain("/repo-a");
        dispose();
      });
    });

    it("renameGroup() updates name", () => {
      createRoot((dispose) => {
        const id = store.createGroup("Work")!;
        expect(store.renameGroup(id, "Personal")).toBe(true);
        expect(store.state.groups[id].name).toBe("Personal");
        dispose();
      });
    });

    it("renameGroup() rejects duplicate names", () => {
      createRoot((dispose) => {
        const id1 = store.createGroup("Work")!;
        store.createGroup("Personal");
        expect(store.renameGroup(id1, "personal")).toBe(false);
        expect(store.state.groups[id1].name).toBe("Work");
        dispose();
      });
    });

    it("setGroupColor() updates color", () => {
      createRoot((dispose) => {
        const id = store.createGroup("Work")!;
        store.setGroupColor(id, "#4A9EFF");
        expect(store.state.groups[id].color).toBe("#4A9EFF");
        dispose();
      });
    });

    it("toggleGroupCollapsed() toggles collapsed flag", () => {
      createRoot((dispose) => {
        const id = store.createGroup("Work")!;
        expect(store.state.groups[id].collapsed).toBe(false);
        store.toggleGroupCollapsed(id);
        expect(store.state.groups[id].collapsed).toBe(true);
        store.toggleGroupCollapsed(id);
        expect(store.state.groups[id].collapsed).toBe(false);
        dispose();
      });
    });

    it("hydrate() loads groups from backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {},
        repoOrder: [],
        groups: {
          "g1": { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: [] },
        },
        groupOrder: ["g1"],
      });
      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.groups["g1"]).toBeDefined();
        expect(store.state.groups["g1"].name).toBe("Work");
        expect(store.state.groupOrder).toEqual(["g1"]);
        dispose();
      });
    });

    it("hydrate() migration: missing groups field initializes empty", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {
          "/repo": {
            path: "/repo", displayName: "test", initials: "TE",
            expanded: true, collapsed: false,
            branches: { main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 } },
            activeBranch: "main",
          },
        },
        repoOrder: ["/repo"],
        // No groups or groupOrder
      });
      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.groups).toEqual({});
        expect(store.state.groupOrder).toEqual([]);
        expect(store.state.repoOrder).toEqual(["/repo"]);
        dispose();
      });
    });

    it("persists groups via save", () => {
      createRoot((dispose) => {
        const id = store.createGroup("Work")!;
        vi.advanceTimersByTime(500);
        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        );
        const lastCall = saveCalls[saveCalls.length - 1];
        expect(lastCall[1].config.groups).toBeDefined();
        expect(lastCall[1].config.groups[id].name).toBe("Work");
        expect(lastCall[1].config.groupOrder).toContain(id);
        dispose();
      });
    });
  });

  describe("groups — repo assignment", () => {
    it("addRepoToGroup() moves repo from ungrouped to group", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/repo-a", gid);
        expect(store.state.groups[gid].repoOrder).toContain("/repo-a");
        expect(store.state.repoOrder).not.toContain("/repo-a");
        dispose();
      });
    });

    it("addRepoToGroup() moves repo from one group to another", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        const g1 = store.createGroup("Work")!;
        const g2 = store.createGroup("Personal")!;
        store.addRepoToGroup("/repo-a", g1);
        store.addRepoToGroup("/repo-a", g2);
        expect(store.state.groups[g1].repoOrder).not.toContain("/repo-a");
        expect(store.state.groups[g2].repoOrder).toContain("/repo-a");
        dispose();
      });
    });

    it("removeRepoFromGroup() moves repo to ungrouped", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/repo-a", gid);
        store.removeRepoFromGroup("/repo-a");
        expect(store.state.groups[gid].repoOrder).not.toContain("/repo-a");
        expect(store.state.repoOrder).toContain("/repo-a");
        dispose();
      });
    });

    it("remove() also cleans up group membership", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/repo-a", gid);
        store.remove("/repo-a");
        expect(store.state.groups[gid].repoOrder).not.toContain("/repo-a");
        dispose();
      });
    });

    it("getGroupForRepo() returns correct group or undefined", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo-a", displayName: "A" });
        store.add({ path: "/repo-b", displayName: "B" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/repo-a", gid);
        expect(store.getGroupForRepo("/repo-a")?.id).toBe(gid);
        expect(store.getGroupForRepo("/repo-b")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("groups — reordering and layout", () => {
    it("reorderRepoInGroup() reorders within group", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        store.add({ path: "/c", displayName: "C" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/a", gid);
        store.addRepoToGroup("/b", gid);
        store.addRepoToGroup("/c", gid);
        store.reorderRepoInGroup(gid, 0, 2);
        expect(store.state.groups[gid].repoOrder).toEqual(["/b", "/c", "/a"]);
        dispose();
      });
    });

    it("moveRepoBetweenGroups() moves with correct index", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        const g1 = store.createGroup("Work")!;
        const g2 = store.createGroup("Personal")!;
        store.addRepoToGroup("/a", g1);
        store.addRepoToGroup("/b", g2);
        store.moveRepoBetweenGroups("/a", g1, g2, 0);
        expect(store.state.groups[g1].repoOrder).toEqual([]);
        expect(store.state.groups[g2].repoOrder).toEqual(["/a", "/b"]);
        dispose();
      });
    });

    it("reorderGroups() reorders group display order", () => {
      createRoot((dispose) => {
        const g1 = store.createGroup("A")!;
        const g2 = store.createGroup("B")!;
        const g3 = store.createGroup("C")!;
        expect(store.state.groupOrder).toEqual([g1, g2, g3]);
        store.reorderGroups(0, 2);
        expect(store.state.groupOrder).toEqual([g2, g3, g1]);
        dispose();
      });
    });

    it("getGroupedLayout() returns groups + ungrouped split", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        store.add({ path: "/c", displayName: "C" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/a", gid);
        store.addRepoToGroup("/b", gid);
        const layout = store.getGroupedLayout();
        expect(layout.groups).toHaveLength(1);
        expect(layout.groups[0].group.id).toBe(gid);
        expect(layout.groups[0].repos).toHaveLength(2);
        expect(layout.groups[0].repos[0].path).toBe("/a");
        expect(layout.ungrouped).toHaveLength(1);
        expect(layout.ungrouped[0].path).toBe("/c");
        dispose();
      });
    });

    it("getGroupedLayout() respects groupOrder and per-group repoOrder", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        const g1 = store.createGroup("First")!;
        const g2 = store.createGroup("Second")!;
        store.addRepoToGroup("/a", g2);
        store.addRepoToGroup("/b", g1);
        const layout = store.getGroupedLayout();
        expect(layout.groups[0].group.name).toBe("First");
        expect(layout.groups[0].repos[0].path).toBe("/b");
        expect(layout.groups[1].group.name).toBe("Second");
        expect(layout.groups[1].repos[0].path).toBe("/a");
        dispose();
      });
    });
  });

  describe("park repos", () => {
    it("setPark() marks a repo as parked", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        expect(store.get("/repo")!.parked).toBe(false);
        store.setPark("/repo", true);
        expect(store.get("/repo")!.parked).toBe(true);
        dispose();
      });
    });

    it("setPark(false) unparks a repo", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setPark("/repo", true);
        store.setPark("/repo", false);
        expect(store.get("/repo")!.parked).toBe(false);
        dispose();
      });
    });

    it("getParkedRepos() returns only parked repos", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        store.add({ path: "/c", displayName: "C" });
        store.setPark("/b", true);
        const parked = store.getParkedRepos();
        expect(parked).toHaveLength(1);
        expect(parked[0].path).toBe("/b");
        dispose();
      });
    });

    it("getOrderedRepos() excludes parked repos", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        store.setPark("/b", true);
        const ordered = store.getOrderedRepos();
        expect(ordered).toHaveLength(1);
        expect(ordered[0].path).toBe("/a");
        dispose();
      });
    });

    it("getGroupedLayout() excludes parked repos from groups and ungrouped", () => {
      createRoot((dispose) => {
        store.add({ path: "/a", displayName: "A" });
        store.add({ path: "/b", displayName: "B" });
        store.add({ path: "/c", displayName: "C" });
        const gid = store.createGroup("Work")!;
        store.addRepoToGroup("/a", gid);
        store.addRepoToGroup("/b", gid);
        store.setPark("/b", true);
        store.setPark("/c", true);
        const layout = store.getGroupedLayout();
        expect(layout.groups[0].repos).toHaveLength(1);
        expect(layout.groups[0].repos[0].path).toBe("/a");
        expect(layout.ungrouped).toHaveLength(0);
        dispose();
      });
    });

    it("parked repos persist via save", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setPark("/repo", true);
        vi.advanceTimersByTime(500);
        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        );
        const lastCall = saveCalls[saveCalls.length - 1];
        expect(lastCall[1].config.repos["/repo"].parked).toBe(true);
        dispose();
      });
    });

    it("hydrate() defaults parked to false when missing", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {
          "/repo": {
            path: "/repo", displayName: "test", initials: "TE",
            expanded: true, collapsed: false,
            branches: { main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 } },
            activeBranch: "main",
          },
        },
        repoOrder: ["/repo"],
      });
      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.get("/repo")!.parked).toBe(false);
        dispose();
      });
    });
  });

  describe("hydrate guard", () => {
    it("blocks saves before hydrate completes", () => {
      createRoot((dispose) => {
        store._testSetHydrated(false);
        store.add({ path: "/repo", displayName: "test" });
        vi.advanceTimersByTime(500);

        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        ).length;
        expect(saveCalls).toBe(0);

        store._testSetHydrated(true);
        dispose();
      });
    });

    it("allows saves after hydrate completes", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        vi.advanceTimersByTime(500);

        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        ).length;
        expect(saveCalls).toBe(1);
        dispose();
      });
    });
  });

  describe("save debouncing", () => {
    it("coalesces rapid mutations into a single save call", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        store.setBranch("/repo", "feature");
        store.toggleExpanded("/repo");

        // Before debounce fires, no save_repositories should have been called
        const saveCallsBefore = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        ).length;
        expect(saveCallsBefore).toBe(0);

        // After debounce period, exactly one save call
        vi.advanceTimersByTime(500);
        const saveCallsAfter = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_repositories"
        ).length;
        expect(saveCallsAfter).toBe(1);
        dispose();
      });
    });

    it("does not save for updateBranchStats (ephemeral data)", () => {
      createRoot((dispose) => {
        store.add({ path: "/repo", displayName: "test" });
        store.setBranch("/repo", "main");
        vi.advanceTimersByTime(500);
        mockInvoke.mockClear();

        store.updateBranchStats("/repo", "main", 10, 5);
        vi.advanceTimersByTime(1000);

        expect(mockInvoke).not.toHaveBeenCalled();
        dispose();
      });
    });
  });
});
