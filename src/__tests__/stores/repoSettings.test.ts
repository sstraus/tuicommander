import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

// Mock repoDefaultsStore so getEffective tests are deterministic
const mockDefaults = {
  baseBranch: "automatic",
  copyIgnoredFiles: false,
  copyUntrackedFiles: false,
  setupScript: "",
  runScript: "",
};

vi.mock("../../stores/repoDefaults", () => ({
  repoDefaultsStore: { state: mockDefaults },
}));

describe("repoSettingsStore", () => {
  let store: typeof import("../../stores/repoSettings").repoSettingsStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    // Reset mock defaults to known state
    Object.assign(mockDefaults, {
      baseBranch: "automatic",
      copyIgnoredFiles: false,
      copyUntrackedFiles: false,
      setupScript: "",
      runScript: "",
    });

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    vi.doMock("../../stores/repoDefaults", () => ({
      repoDefaultsStore: { state: mockDefaults },
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
    it("creates settings for new repo with null overridable fields (inheriting from global)", () => {
      createRoot((dispose) => {
        const settings = store.getOrCreate("/repo", "my-repo");
        expect(settings.path).toBe("/repo");
        expect(settings.displayName).toBe("my-repo");
        // Overridable fields default to null (inherit from global defaults)
        expect(settings.baseBranch).toBeNull();
        expect(settings.copyIgnoredFiles).toBeNull();
        expect(settings.copyUntrackedFiles).toBeNull();
        expect(settings.setupScript).toBeNull();
        expect(settings.runScript).toBeNull();
        // Non-overridable fields remain non-nullable
        expect(settings.color).toBe("");
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

    it("can set overridable fields back to null (inherit)", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" });
        store.update("/repo", { baseBranch: null });
        expect(store.get("/repo")?.baseBranch).toBeNull();
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

  describe("getEffective()", () => {
    it("returns global defaults for a new repo with null overrides", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        const effective = store.getEffective("/repo");
        expect(effective.baseBranch).toBe("automatic"); // from global default
        expect(effective.copyIgnoredFiles).toBe(false);
        expect(effective.copyUntrackedFiles).toBe(false);
        expect(effective.setupScript).toBe("");
        expect(effective.runScript).toBe("");
        dispose();
      });
    });

    it("uses per-repo override when set", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", copyIgnoredFiles: true });

        mockDefaults.baseBranch = "develop"; // global default is different
        const effective = store.getEffective("/repo");
        expect(effective.baseBranch).toBe("main"); // repo override wins
        expect(effective.copyIgnoredFiles).toBe(true);
        dispose();
      });
    });

    it("falls back to global default when field is null", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        // baseBranch is null (inherit) but global says "develop"
        mockDefaults.baseBranch = "develop";
        const effective = store.getEffective("/repo");
        expect(effective.baseBranch).toBe("develop");
        dispose();
      });
    });

    it("returns non-nullable effective settings", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        const effective = store.getEffective("/repo");
        // All fields must be non-null
        expect(effective.baseBranch).not.toBeNull();
        expect(effective.copyIgnoredFiles).not.toBeNull();
        expect(effective.setupScript).not.toBeNull();
        dispose();
      });
    });

    it("returns undefined for unknown repo", () => {
      createRoot((dispose) => {
        expect(store.getEffective("/unknown")).toBeUndefined();
        dispose();
      });
    });

    it("preserves non-overridable fields (path, displayName, color)", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { color: "#ff0000" });
        const effective = store.getEffective("/repo");
        expect(effective!.path).toBe("/repo");
        expect(effective!.displayName).toBe("my-repo");
        expect(effective!.color).toBe("#ff0000");
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

    it("returns false for unknown repos", async () => {
      await createRoot(async (dispose) => {
        expect(await store.hasCustomSettings("/unknown")).toBe(false);
        dispose();
      });
    });
  });

  describe("reset()", () => {
    it("resets overridable fields to null (inherit from global)", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
        store.reset("/repo");
        expect(store.get("/repo")?.baseBranch).toBeNull();
        expect(store.get("/repo")?.setupScript).toBeNull();
        expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
        dispose();
      });
    });

    it("preserves non-overridable fields (displayName, color)", () => {
      createRoot((dispose) => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { color: "#ff0000" });
        store.reset("/repo");
        expect(store.get("/repo")?.displayName).toBe("my-repo");
        expect(store.get("/repo")?.color).toBe("#ff0000");
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
            copyIgnoredFiles: null,
            copyUntrackedFiles: null,
            setupScript: null,
            runScript: null,
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
