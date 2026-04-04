import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("repoDefaultsStore", () => {
  let store: typeof import("../../stores/repoDefaults").repoDefaultsStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/repoDefaults")).repoDefaultsStore;
  });

  describe("initial state", () => {
    it("has sensible default values", () => {
      testInScope(() => {
        expect(store.state.baseBranch).toBe("automatic");
        expect(store.state.copyIgnoredFiles).toBe(false);
        expect(store.state.copyUntrackedFiles).toBe(false);
        expect(store.state.setupScript).toBe("");
        expect(store.state.runScript).toBe("");
      });
    });
  });

  describe("hydrate()", () => {
    it("loads defaults from backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        base_branch: "main",
        copy_ignored_files: true,
        copy_untracked_files: true,
        setup_script: "npm install",
        run_script: "npm run dev",
      });

      await store.hydrate();

      testInScope(() => {
        expect(store.state.baseBranch).toBe("main");
        expect(store.state.copyIgnoredFiles).toBe(true);
        expect(store.state.copyUntrackedFiles).toBe(true);
        expect(store.state.setupScript).toBe("npm install");
        expect(store.state.runScript).toBe("npm run dev");
      });
    });

    it("keeps defaults when backend returns null", async () => {
      mockInvoke.mockResolvedValueOnce(null);
      await store.hydrate();

      testInScope(() => {
        expect(store.state.baseBranch).toBe("automatic");
        expect(store.state.copyIgnoredFiles).toBe(false);
      });
    });

    it("keeps defaults when backend fails", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("backend error"));
      await store.hydrate();

      testInScope(() => {
        expect(store.state.baseBranch).toBe("automatic");
      });
    });
  });

  describe("setters", () => {
    it("setBaseBranch updates state and persists", () => {
      testInScope(() => {
        store.setBaseBranch("main");
        expect(store.state.baseBranch).toBe("main");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.objectContaining({
          config: expect.objectContaining({ base_branch: "main" }),
        }));
      });
    });

    it("setCopyIgnoredFiles updates state and persists", () => {
      testInScope(() => {
        store.setCopyIgnoredFiles(true);
        expect(store.state.copyIgnoredFiles).toBe(true);
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.any(Object));
      });
    });

    it("setCopyUntrackedFiles updates state and persists", () => {
      testInScope(() => {
        store.setCopyUntrackedFiles(true);
        expect(store.state.copyUntrackedFiles).toBe(true);
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.objectContaining({
          config: expect.objectContaining({ copy_untracked_files: true }),
        }));
      });
    });

    it("setSetupScript updates state and persists", () => {
      testInScope(() => {
        store.setSetupScript("npm install");
        expect(store.state.setupScript).toBe("npm install");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.objectContaining({
          config: expect.objectContaining({ setup_script: "npm install" }),
        }));
      });
    });

    it("setRunScript updates state and persists", () => {
      testInScope(() => {
        store.setRunScript("npm run dev");
        expect(store.state.runScript).toBe("npm run dev");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.objectContaining({
          config: expect.objectContaining({ run_script: "npm run dev" }),
        }));
      });
    });

    it("save includes full config with all fields", () => {
      testInScope(() => {
        store.setBaseBranch("develop");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", {
          config: {
            base_branch: "develop",
            copy_ignored_files: false,
            copy_untracked_files: false,
            setup_script: "",
            run_script: "",
            archive_script: "",
            worktree_storage: "sibling",
            prompt_on_create: true,
            delete_branch_on_remove: true,
            auto_archive_merged: false,
            orphan_cleanup: "ask",
            pr_merge_strategy: "merge",
            after_merge: "archive",
            auto_fetch_interval_minutes: 0,
            auto_delete_on_pr_close: "off",
          },
        });
      });
    });
  });
});
