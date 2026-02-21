import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

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
      createRoot((dispose) => {
        expect(store.state.baseBranch).toBe("automatic");
        expect(store.state.copyIgnoredFiles).toBe(false);
        expect(store.state.copyUntrackedFiles).toBe(false);
        expect(store.state.setupScript).toBe("");
        expect(store.state.runScript).toBe("");
        dispose();
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

      createRoot((dispose) => {
        expect(store.state.baseBranch).toBe("main");
        expect(store.state.copyIgnoredFiles).toBe(true);
        expect(store.state.copyUntrackedFiles).toBe(true);
        expect(store.state.setupScript).toBe("npm install");
        expect(store.state.runScript).toBe("npm run dev");
        dispose();
      });
    });

    it("keeps defaults when backend returns null", async () => {
      mockInvoke.mockResolvedValueOnce(null);
      await store.hydrate();

      createRoot((dispose) => {
        expect(store.state.baseBranch).toBe("automatic");
        expect(store.state.copyIgnoredFiles).toBe(false);
        dispose();
      });
    });

    it("keeps defaults when backend fails", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("backend error"));
      await store.hydrate();

      createRoot((dispose) => {
        expect(store.state.baseBranch).toBe("automatic");
        dispose();
      });
    });
  });

  describe("setters", () => {
    it("setBaseBranch updates state and persists", () => {
      createRoot((dispose) => {
        store.setBaseBranch("main");
        expect(store.state.baseBranch).toBe("main");
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.objectContaining({
          config: expect.objectContaining({ base_branch: "main" }),
        }));
        dispose();
      });
    });

    it("setCopyIgnoredFiles updates state and persists", () => {
      createRoot((dispose) => {
        store.setCopyIgnoredFiles(true);
        expect(store.state.copyIgnoredFiles).toBe(true);
        expect(mockInvoke).toHaveBeenCalledWith("save_repo_defaults", expect.any(Object));
        dispose();
      });
    });

    it("setCopyUntrackedFiles updates state and persists", () => {
      createRoot((dispose) => {
        store.setCopyUntrackedFiles(true);
        expect(store.state.copyUntrackedFiles).toBe(true);
        dispose();
      });
    });

    it("setSetupScript updates state and persists", () => {
      createRoot((dispose) => {
        store.setSetupScript("npm install");
        expect(store.state.setupScript).toBe("npm install");
        dispose();
      });
    });

    it("setRunScript updates state and persists", () => {
      createRoot((dispose) => {
        store.setRunScript("npm run dev");
        expect(store.state.runScript).toBe("npm run dev");
        dispose();
      });
    });
  });
});
