import { describe, it, expect, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const mockInvoke = vi.mocked(invoke);

describe("get_initials Rust command", () => {
  it("calls the Rust get_initials command", async () => {
    mockInvoke.mockResolvedValueOnce("MR");
    const result = await invoke<string>("get_initials", { name: "my-repo" });
    expect(mockInvoke).toHaveBeenCalledWith("get_initials", { name: "my-repo" });
    expect(result).toBe("MR");
  });
});

describe("repositoriesStore.add() uses initials from caller", () => {
  it("stores initials passed from RepoInfo", async () => {
    // Reset modules to get a fresh store
    const { repositoriesStore } = await import("../../stores/repositories");

    repositoriesStore.add({ path: "/test-repo", displayName: "my-repo", initials: "MR" });

    const repo = repositoriesStore.get("/test-repo");
    expect(repo).toBeDefined();
    expect(repo!.initials).toBe("MR");
  });

  it("defaults to empty string when initials not provided", async () => {
    const { repositoriesStore } = await import("../../stores/repositories");

    repositoriesStore.add({ path: "/fallback-repo", displayName: "some-repo" });

    const repo = repositoriesStore.get("/fallback-repo");
    expect(repo).toBeDefined();
    expect(repo!.initials).toBe("");
  });
});
