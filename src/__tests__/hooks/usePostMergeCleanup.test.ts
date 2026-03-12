import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";

const {
  mockRemoveBranch,
  mockBumpRevision,
  mockGetBranches,
} = vi.hoisted(() => ({
  mockRemoveBranch: vi.fn(),
  mockBumpRevision: vi.fn(),
  mockGetBranches: vi.fn(),
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    removeBranch: mockRemoveBranch,
    bumpRevision: mockBumpRevision,
    get: mockGetBranches,
  },
}));

vi.mock("../../stores/appLogger", () => ({
  appLogger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

import { executeCleanup, type CleanupConfig } from "../../hooks/usePostMergeCleanup";

function makeConfig(overrides?: Partial<CleanupConfig>): CleanupConfig {
  return {
    repoPath: "/repo",
    branchName: "feature/login",
    baseBranch: "main",
    steps: [
      { id: "switch", checked: true },
      { id: "pull", checked: true },
      { id: "delete-local", checked: true },
      { id: "delete-remote", checked: true },
    ],
    onStepStart: vi.fn(),
    onStepDone: vi.fn(),
    closeTerminalsForBranch: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe("executeCleanup", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Default: run_git_command returns { stdout, stderr }, other commands return undefined
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "run_git_command") return Promise.resolve({ stdout: "", stderr: "" });
      return Promise.resolve(undefined);
    });
    mockGetBranches.mockReturnValue({
      branches: { "feature/login": { terminals: [] } },
    });
  });

  it("calls steps in order: switch → pull → delete-local → delete-remote", async () => {
    const config = makeConfig();
    await executeCleanup(config);

    // Verify invoke call order
    const calls = mockInvoke.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toEqual([
      "switch_branch",
      "run_git_command",   // pull
      "delete_local_branch",
      "run_git_command",   // push --delete
    ]);
  });

  it("skips unchecked steps", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: true },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    const calls = mockInvoke.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toEqual(["delete_local_branch"]);
  });

  it("calls switch_branch with stash: true", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("switch_branch", {
      repoPath: "/repo",
      branch: "main",
      force: false,
      stash: true,
    });
  });

  it("calls run_git_command with pull --ff-only", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: true },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      path: "/repo",
      args: ["pull", "--ff-only"],
    });
  });

  it("calls closeTerminalsForBranch then delete_local_branch", async () => {
    const closeTerminals = vi.fn().mockResolvedValue(undefined);
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: true },
        { id: "delete-remote", checked: false },
      ],
      closeTerminalsForBranch: closeTerminals,
    });
    await executeCleanup(config);

    expect(closeTerminals).toHaveBeenCalledWith("/repo", "feature/login");
    expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
      repoPath: "/repo",
      branchName: "feature/login",
    });
    // closeTerminals was called before delete
    const closeOrder = closeTerminals.mock.invocationCallOrder[0];
    const deleteOrder = mockInvoke.mock.invocationCallOrder[0];
    expect(closeOrder).toBeLessThan(deleteOrder);
  });

  it("calls run_git_command with push --delete for remote branch", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: true },
      ],
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      path: "/repo",
      args: ["push", "origin", "--delete", "feature/login"],
    });
  });

  it("reports per-step status via callbacks", async () => {
    const onStepStart = vi.fn();
    const onStepDone = vi.fn();
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      onStepStart,
      onStepDone,
    });
    await executeCleanup(config);

    expect(onStepStart).toHaveBeenCalledWith("switch");
    expect(onStepDone).toHaveBeenCalledWith("switch", "success", undefined);
  });

  it("reports error status when a step fails", async () => {
    mockInvoke
      .mockRejectedValueOnce(new Error("branch checkout conflict"));
    const onStepStart = vi.fn();
    const onStepDone = vi.fn();
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: true },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      onStepStart,
      onStepDone,
    });
    await executeCleanup(config);

    expect(onStepDone).toHaveBeenCalledWith("switch", "error", "branch checkout conflict");
    // pull should not have been attempted after switch error
    expect(onStepStart).not.toHaveBeenCalledWith("pull");
  });

  it("handles 'remote ref does not exist' as success", async () => {
    mockInvoke
      .mockResolvedValueOnce(undefined)                    // switch
      .mockResolvedValueOnce({ stdout: "", stderr: "" })   // pull
      .mockResolvedValueOnce(undefined)                    // delete-local
      .mockRejectedValueOnce("error: remote ref does not exist"); // delete-remote

    const onStepDone = vi.fn();
    const config = makeConfig({ onStepDone });
    await executeCleanup(config);

    expect(onStepDone).toHaveBeenCalledWith("delete-remote", "success", undefined);
  });

  it("calls removeBranch and bumpRevision after local branch delete", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: true },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockRemoveBranch).toHaveBeenCalledWith("/repo", "feature/login");
    expect(mockBumpRevision).toHaveBeenCalledWith("/repo");
  });

  it("runs stash pop after switch when unstash is true", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      unstash: true,
    });
    await executeCleanup(config);

    const calls = mockInvoke.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toEqual(["switch_branch", "run_git_command"]);
    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      path: "/repo",
      args: ["stash", "pop"],
    });
  });

  it("does not run stash pop when unstash is false", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      unstash: false,
    });
    await executeCleanup(config);

    const calls = mockInvoke.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toEqual(["switch_branch"]);
  });

  it("handles no remote tracking branch gracefully", async () => {
    mockInvoke.mockRejectedValueOnce("error: unable to delete 'feature/login': remote ref does not exist");

    const onStepDone = vi.fn();
    const config = makeConfig({
      steps: [
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: true },
      ],
      onStepDone,
    });
    await executeCleanup(config);

    expect(onStepDone).toHaveBeenCalledWith("delete-remote", "success", undefined);
  });

  it("calls finalize_merged_worktree for worktree step", async () => {
    const config = makeConfig({
      steps: [
        { id: "worktree", checked: true },
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      worktreeAction: "archive",
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("finalize_merged_worktree", {
      repoPath: "/repo",
      branchName: "feature/login",
      action: "archive",
      archiveScript: null,
    });
  });

  it("calls finalize_merged_worktree with 'delete' action", async () => {
    const config = makeConfig({
      steps: [
        { id: "worktree", checked: true },
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
      worktreeAction: "delete",
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("finalize_merged_worktree", {
      repoPath: "/repo",
      branchName: "feature/login",
      action: "delete",
      archiveScript: null,
    });
  });

  it("skips worktree step when worktreeAction is not set", async () => {
    const config = makeConfig({
      steps: [
        { id: "worktree", checked: true },
        { id: "switch", checked: false },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockInvoke).not.toHaveBeenCalledWith("finalize_merged_worktree", expect.anything());
  });

  it("calls bumpRevision at the end even without local delete", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockBumpRevision).toHaveBeenCalledWith("/repo");
  });
});
