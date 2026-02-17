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
