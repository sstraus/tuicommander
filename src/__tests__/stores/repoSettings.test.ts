import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

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
  archiveScript: "",
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
      archiveScript: "",
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
      testInScope(() => {
        expect(store.get("/unknown")).toBeUndefined();
      });
    });
  });

  describe("getOrCreate()", () => {
    it("creates settings for new repo with null overridable fields (inheriting from global)", () => {
      testInScope(() => {
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
      });
    });

    it("returns existing settings", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" });
        const settings = store.getOrCreate("/repo", "my-repo");
        expect(settings.baseBranch).toBe("main");
      });
    });

    it("persists via invoke", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_settings", {
          config: expect.objectContaining({
            repos: expect.objectContaining({
              "/repo": expect.objectContaining({ path: "/repo" }),
            }),
          }),
        });
      });
    });
  });

  describe("update()", () => {
    it("updates existing settings", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
        expect(store.get("/repo")?.baseBranch).toBe("main");
        expect(store.get("/repo")?.setupScript).toBe("npm install");
      });
    });

    it("can set overridable fields back to null (inherit)", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" });
        store.update("/repo", { baseBranch: null });
        expect(store.get("/repo")?.baseBranch).toBeNull();
      });
    });

    it("ignores updates for unknown repos", () => {
      testInScope(() => {
        store.update("/unknown", { baseBranch: "main" }); // Should not throw
      });
    });
  });

  describe("getEffective()", () => {
    it("returns global defaults for a new repo with null overrides", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        expect(effective!.baseBranch).toBe("automatic"); // from global default
        expect(effective!.copyIgnoredFiles).toBe(false);
        expect(effective!.copyUntrackedFiles).toBe(false);
        expect(effective!.setupScript).toBe("");
        expect(effective!.runScript).toBe("");
      });
    });

    it("uses per-repo override when set", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", copyIgnoredFiles: true });

        mockDefaults.baseBranch = "develop"; // global default is different
        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        expect(effective!.baseBranch).toBe("main"); // repo override wins
        expect(effective!.copyIgnoredFiles).toBe(true);
      });
    });

    it("falls back to global default when field is null", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        // baseBranch is null (inherit) but global says "develop"
        mockDefaults.baseBranch = "develop";
        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        expect(effective!.baseBranch).toBe("develop");
      });
    });

    it("returns non-nullable effective settings", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        // All fields must be non-null
        expect(effective!.baseBranch).not.toBeNull();
        expect(effective!.copyIgnoredFiles).not.toBeNull();
        expect(effective!.setupScript).not.toBeNull();
      });
    });

    it("returns undefined for unknown repo", () => {
      testInScope(() => {
        expect(store.getEffective("/unknown")).toBeUndefined();
      });
    });

    it("returns archiveScript from global default when not overridden", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        mockDefaults.archiveScript = "cleanup.sh";
        const effective = store.getEffective("/repo");
        expect(effective!.archiveScript).toBe("cleanup.sh");
      });
    });

    it("uses per-repo archiveScript override when set", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { archiveScript: "my-cleanup.sh" });
        mockDefaults.archiveScript = "global-cleanup.sh";
        const effective = store.getEffective("/repo");
        expect(effective!.archiveScript).toBe("my-cleanup.sh");
      });
    });

    it("preserves non-overridable fields (path, displayName, color)", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { color: "#ff0000" });
        const effective = store.getEffective("/repo");
        expect(effective!.path).toBe("/repo");
        expect(effective!.displayName).toBe("my-repo");
        expect(effective!.color).toBe("#ff0000");
      });
    });
  });

  describe("three-tier getEffective() with local config", () => {
    it("uses .tuic.json values when per-repo setting is null", async () => {
      await testInScopeAsync(async () => {
        // Simulate Tauri returning a local config from .tuic.json
        mockInvoke.mockImplementation(async (cmd: string) => {
          if (cmd === "load_repo_local_config") {
            return { base_branch: "develop", setup_script: "make setup" };
          }
          return undefined;
        });

        store.getOrCreate("/repo", "my-repo");
        await store.loadLocalConfig("/repo");

        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        // .tuic.json overrides global default for non-script fields
        expect(effective!.baseBranch).toBe("develop");
        // SECURITY: script fields from .tuic.json are intentionally NOT merged
        // (a malicious repo could inject shell commands via committed .tuic.json)
        expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
        // Global default still applies for fields not in .tuic.json
        expect(effective!.runScript).toBe(""); // from global default
      });
    });

    it("per-repo setting overrides .tuic.json", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation(async (cmd: string) => {
          if (cmd === "load_repo_local_config") {
            return { base_branch: "develop", setup_script: "make setup" };
          }
          return undefined;
        });

        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main" }); // per-repo override
        await store.loadLocalConfig("/repo");

        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        // per-repo overrides .tuic.json
        expect(effective!.baseBranch).toBe("main");
        // SECURITY: script fields from .tuic.json are NOT merged — falls back to global default
        expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
      });
    });

    it("returns undefined for missing .tuic.json (no local config cached)", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        // No loadLocalConfig called — should fall back to two-tier
        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        expect(effective!.baseBranch).toBe("automatic"); // global default
      });
    });

    it("handles null from Tauri (no .tuic.json file)", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation(async (cmd: string) => {
          if (cmd === "load_repo_local_config") return null;
          return undefined;
        });

        store.getOrCreate("/repo", "my-repo");
        await store.loadLocalConfig("/repo");

        const effective = store.getEffective("/repo");
        expect(effective).toBeDefined();
        expect(effective!.baseBranch).toBe("automatic"); // global default
      });
    });
  });

  describe("remove()", () => {
    it("removes settings", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.remove("/repo");
        expect(store.get("/repo")).toBeUndefined();
      });
    });

    it("clears activeRepoPath if removed", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.setActiveRepo("/repo");
        store.remove("/repo");
        expect(store.state.activeRepoPath).toBeNull();
      });
    });
  });

  describe("hasCustomSettings()", () => {
    it("returns false for defaults", async () => {
      await testInScopeAsync(async () => {
        store.getOrCreate("/repo", "my-repo");
        mockInvoke.mockResolvedValueOnce(false);
        expect(await store.hasCustomSettings("/repo")).toBe(false);
        expect(mockInvoke).toHaveBeenCalledWith("check_has_custom_settings", { path: "/repo" });
      });
    });

    it("returns false for unknown repos", async () => {
      await testInScopeAsync(async () => {
        expect(await store.hasCustomSettings("/unknown")).toBe(false);
      });
    });
  });

  describe("reset()", () => {
    it("resets overridable fields to null (inherit from global)", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
        store.reset("/repo");
        expect(store.get("/repo")?.baseBranch).toBeNull();
        expect(store.get("/repo")?.setupScript).toBeNull();
        expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
      });
    });

    it("preserves non-overridable fields (displayName, color)", () => {
      testInScope(() => {
        store.getOrCreate("/repo", "my-repo");
        store.update("/repo", { color: "#ff0000" });
        store.reset("/repo");
        expect(store.get("/repo")?.displayName).toBe("my-repo");
        expect(store.get("/repo")?.color).toBe("#ff0000");
      });
    });
  });

  describe("getAll()", () => {
    it("returns all settings", () => {
      testInScope(() => {
        store.getOrCreate("/repo1", "repo1");
        store.getOrCreate("/repo2", "repo2");
        expect(store.getAll()).toHaveLength(2);
      });
    });
  });

  describe("setActiveRepo()", () => {
    it("sets active repo path", () => {
      testInScope(() => {
        store.setActiveRepo("/repo");
        expect(store.state.activeRepoPath).toBe("/repo");
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

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.get("/repo")?.baseBranch).toBe("main");
        expect(mockInvoke).toHaveBeenCalledWith("load_repo_settings");
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-repo-settings", JSON.stringify({
        "/repo": { path: "/repo", displayName: "my-repo", baseBranch: "main" },
      }));
      mockInvoke.mockResolvedValueOnce(undefined); // save_repo_settings migration
      mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repo_settings

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-repo-settings")).toBeNull();
      });
    });
  });
});
