import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("repoSettingsStore", () => {
  let store: typeof import("../../stores/repoSettings").repoSettingsStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/repoSettings")).repoSettingsStore;
  });

  describe("get()", () => {
    it("returns undefined for unknown repo", () => {
      createRoot((dispose) => {
        expect(store.get("/unknown")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("getOrCreate()", () => {
    it("creates settings for new repo", () => {
      createRoot((dispose) => {
        const settings = store.getOrCreate("/repo", "my-repo");
        expect(settings.path).toBe("/repo");
        expect(settings.displayName).toBe("my-repo");
        expect(settings.baseBranch).toBe("automatic");
        dispose();
      });
    });

    it("returns existing settings", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" });
        const settings = store.getOrCreate("/repo", "my-repo");
        expect(settings.baseBranch).toBe("main");
        dispose();
      });
    });

    it("persists via invoke", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_settings", {
          config: expect.objectContaining({
            repos: expect.objectContaining({
              "/repo": expect.objectContaining({ path: "/repo" }),
            }),
          }),
        });
        dispose();
      });
    });
  });

  describe("update()", () => {
    it("updates existing settings", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
        expect(store.get("/repo")?.baseBranch).toBe("main");
        expect(store.get("/repo")?.setupScript).toBe("npm install");
        dispose();
      });
    });

    it("ignores updates for unknown repos", () => {
      createRoot((dispose) => {
        store.update("/unknown", { baseBranch: "main" }); // Should not throw
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes settings", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.remove("/repo");
        expect(store.get("/repo")).toBeUndefined();
        dispose();
      });
    });

    it("clears activeRepoPath if removed", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.setActiveRepo("/repo");
        store.remove("/repo");
        expect(store.state.activeRepoPath).toBeNull();
        dispose();
      });
    });
  });

  describe("hasCustomSettings()", () => {
    it("returns false for defaults", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        mockInvoke.mockResolvedValueOnce(false);
        expect(await store.hasCustomSettings("/repo")).toBe(false);
        expect(mockInvoke).toHaveBeenCalledWith("check_has_custom_settings", { path: "/repo" });
        dispose();
      });
    });

    it("returns true when baseBranch changed", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns true when setupScript set", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { setupScript: "npm install" });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns true when runScript set", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { runScript: "npm start" });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns true when copyIgnoredFiles enabled", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { copyIgnoredFiles: true });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns true when copyUntrackedFiles enabled", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { copyUntrackedFiles: true });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns true when multiple fields changed", async () => {
      await createRoot(async (dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "develop", setupScript: "make build" });
        mockInvoke.mockResolvedValueOnce(true);
        expect(await store.hasCustomSettings("/repo")).toBe(true);
        dispose();
      });
    });

    it("returns false for unknown repos", async () => {
      await createRoot(async (dispose) => {
        expect(await store.hasCustomSettings("/unknown")).toBe(false);
        dispose();
      });
    });
  });

  describe("reset()", () => {
    it("resets to defaults", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
        store.reset("/repo");
        expect(store.get("/repo")?.baseBranch).toBe("automatic");
        expect(store.get("/repo")?.setupScript).toBe("");
        dispose();
      });
    });
  });

  describe("getAll()", () => {
    it("returns all settings", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo1", "repo1");
        store.getOrCreate("/repo2", "repo2");
        expect(store.getAll()).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("setActiveRepo()", () => {
    it("sets active repo path", () => {
      createRoot((dispose) => {
        store.setActiveRepo("/repo");
        expect(store.state.activeRepoPath).toBe("/repo");
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads settings from Rust backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        repos: {
          "/repo": {
            path: "/repo",
            displayName: "my-repo",
            baseBranch: "main",
            copyIgnoredFiles: false,
            copyUntrackedFiles: false,
            setupScript: "",
            runScript: "",
          },
        },
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.get("/repo")?.baseBranch).toBe("main");
        expect(mockInvoke).toHaveBeenCalledWith("load_repo_settings");
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-repo-settings", JSON.stringify({
        "/repo": { path: "/repo", displayName: "my-repo", baseBranch: "main" },
      }));
      mockInvoke.mockResolvedValueOnce(undefined); // save_repo_settings migration
      mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repo_settings

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-repo-settings")).toBeNull();
        dispose();
      });
    });
  });
});
