import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock @tauri-apps/api/core before importing invoke
const mockTauriInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockTauriInvoke(...args),
}));

// Mock transport to simulate Tauri mode
vi.mock("../transport", () => ({
  isTauri: () => true,
}));

// Import after mocks are set up
const { invoke, _inflight_TEST_ONLY } = await import("../invoke");

describe("invoke in-flight dedup", () => {
  beforeEach(() => {
    mockTauriInvoke.mockReset();
    _inflight_TEST_ONLY.clear();
  });

  it("concurrent identical read-only calls return the same promise", async () => {
    let resolveFirst!: (val: unknown) => void;
    mockTauriInvoke.mockImplementationOnce(
      () => new Promise((r) => { resolveFirst = r; }),
    );

    const args = { repoPath: "/repo" };
    const p1 = invoke("get_repo_summary", args);
    const p2 = invoke("get_repo_summary", args);

    // Same promise returned — only one Tauri call
    expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

    resolveFirst({ status: "ok" });
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toEqual({ status: "ok" });
    expect(r2).toEqual({ status: "ok" });
  });

  it("different args get separate promises", async () => {
    mockTauriInvoke.mockResolvedValueOnce("a").mockResolvedValueOnce("b");

    const p1 = invoke("get_repo_summary", { repoPath: "/repo1" });
    const p2 = invoke("get_repo_summary", { repoPath: "/repo2" });

    expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
    expect(await p1).toBe("a");
    expect(await p2).toBe("b");
  });

  it("non-dedup commands bypass dedup (write commands)", async () => {
    mockTauriInvoke.mockResolvedValueOnce("first").mockResolvedValueOnce("second");

    const p1 = invoke("git_stage_files", { path: "/repo", files: ["a.ts"] });
    const p2 = invoke("git_stage_files", { path: "/repo", files: ["a.ts"] });

    // Both should result in separate calls — no dedup for mutations
    expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
    expect(await p1).toBe("first");
    expect(await p2).toBe("second");
  });

  it("promise cleared from cache after settle (resolved)", async () => {
    mockTauriInvoke.mockResolvedValueOnce("ok");

    await invoke("get_repo_summary", { repoPath: "/repo" });
    expect(_inflight_TEST_ONLY.size).toBe(0);
  });

  it("promise cleared from cache after settle (rejected)", async () => {
    mockTauriInvoke.mockRejectedValueOnce(new Error("fail"));

    await expect(invoke("get_repo_summary", { repoPath: "/repo" })).rejects.toThrow("fail");
    expect(_inflight_TEST_ONLY.size).toBe(0);
  });

  it("rejected promises propagate to all subscribers", async () => {
    let rejectFirst!: (err: unknown) => void;
    mockTauriInvoke.mockImplementationOnce(
      () => new Promise((_r, rej) => { rejectFirst = rej; }),
    );

    const args = { repoPath: "/repo" };
    const p1 = invoke("get_repo_info", args);
    const p2 = invoke("get_repo_info", args);

    expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

    rejectFirst(new Error("network"));
    await expect(p1).rejects.toThrow("network");
    await expect(p2).rejects.toThrow("network");
  });

  it("commands with no args are deduped correctly", async () => {
    let resolveFirst!: (val: unknown) => void;
    mockTauriInvoke.mockImplementationOnce(
      () => new Promise((r) => { resolveFirst = r; }),
    );

    const p1 = invoke("fetch_plugin_registry");
    const p2 = invoke("fetch_plugin_registry");

    expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

    resolveFirst([]);
    expect(await p1).toEqual([]);
    expect(await p2).toEqual([]);
  });

  // Regression for #1377-9e24: ensure each git read command is in DEDUP_COMMANDS
  // so that repo-changed fan-out doesn't spawn N parallel git processes.
  it.each([
    "get_diff_stats",
    "get_changed_files",
    "get_file_diff",
    "get_git_branches",
    "get_merged_branches",
    "get_recent_commits",
    "get_remote_url",
  ])("%s is dedupable", async (cmd) => {
    let resolveFirst!: (val: unknown) => void;
    mockTauriInvoke.mockImplementationOnce(
      () => new Promise((r) => { resolveFirst = r; }),
    );

    const args = { path: "/repo" };
    const p1 = invoke(cmd, args);
    const p2 = invoke(cmd, args);

    expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

    resolveFirst("ok");
    expect(await p1).toBe("ok");
    expect(await p2).toBe("ok");
  });

  it("get_file_diff dedups by file+scope+untracked tuple", async () => {
    mockTauriInvoke
      .mockResolvedValueOnce("diff-a-staged")
      .mockResolvedValueOnce("diff-a-unstaged")
      .mockResolvedValueOnce("diff-b-staged");

    const a = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "staged", untracked: false });
    const b = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "staged", untracked: false });
    const c = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "unstaged", untracked: false });
    const d = invoke("get_file_diff", { path: "/r", file: "b.ts", scope: "staged", untracked: false });

    // a/b dedupe; c, d are distinct keys → 3 underlying calls
    expect(mockTauriInvoke).toHaveBeenCalledTimes(3);
    expect(await a).toBe("diff-a-staged");
    expect(await b).toBe("diff-a-staged");
    expect(await c).toBe("diff-a-unstaged");
    expect(await d).toBe("diff-b-staged");
  });

  it("after settle, a new call creates a fresh promise", async () => {
    mockTauriInvoke.mockResolvedValueOnce("first").mockResolvedValueOnce("second");

    const r1 = await invoke("get_repo_summary", { repoPath: "/repo" });
    const r2 = await invoke("get_repo_summary", { repoPath: "/repo" });

    expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
    expect(r1).toBe("first");
    expect(r2).toBe("second");
  });
});
