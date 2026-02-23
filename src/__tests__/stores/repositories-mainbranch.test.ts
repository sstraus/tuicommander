import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import type { repositoriesStore as StoreType } from "../../stores/repositories";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

let store: typeof StoreType;

describe("setBranch isMain defaulting", () => {
  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/repositories")).repositoriesStore;
    store._testSetHydrated(true);
  });

  it("defaults isMain=true for main branches", () => {
    createRoot((dispose) => {
      store.add({ path: "/repo", displayName: "repo" });

      for (const name of ["main", "master", "develop", "development", "dev"]) {
        store.setBranch("/repo", name);
        expect(store.get("/repo")!.branches[name].isMain).toBe(true);
      }

      dispose();
    });
  });

  it("is case-insensitive", () => {
    createRoot((dispose) => {
      store.add({ path: "/repo", displayName: "repo" });

      for (const name of ["Main", "MASTER", "Develop", "DEVELOPMENT", "DEV"]) {
        store.setBranch("/repo", name);
        expect(store.get("/repo")!.branches[name].isMain).toBe(true);
      }

      dispose();
    });
  });

  it("defaults isMain=false for feature branches", () => {
    createRoot((dispose) => {
      store.add({ path: "/repo", displayName: "repo" });

      for (const name of ["feature/foo", "feature/main", "bugfix/master-fix"]) {
        store.setBranch("/repo", name);
        expect(store.get("/repo")!.branches[name].isMain).toBe(false);
      }

      dispose();
    });
  });

  it("defaults isMain=false for other branches", () => {
    createRoot((dispose) => {
      store.add({ path: "/repo", displayName: "repo" });

      for (const name of ["staging", "release/1.0", "hotfix/urgent"]) {
        store.setBranch("/repo", name);
        expect(store.get("/repo")!.branches[name].isMain).toBe(false);
      }

      dispose();
    });
  });

  it("respects explicit isMain from caller (Rust backend value)", () => {
    createRoot((dispose) => {
      store.add({ path: "/repo", displayName: "repo" });

      // Override: feature branch marked as main by backend
      store.setBranch("/repo", "feature/custom", { isMain: true });
      expect(store.get("/repo")!.branches["feature/custom"].isMain).toBe(true);

      // Override: main branch explicitly marked not-main
      store.setBranch("/repo", "main", { isMain: false });
      expect(store.get("/repo")!.branches["main"].isMain).toBe(false);

      dispose();
    });
  });
});
