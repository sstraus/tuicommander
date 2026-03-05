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

  it("calls steps in order: dirty-check → switch → pull → delete-local → delete-remote", async () => {
    const config = makeConfig();
    await executeCleanup(config);

    // Verify invoke call order
    const calls = mockInvoke.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toEqual([
      "run_git_command",   // status --porcelain (dirty check)
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

  it("calls dirty check then switch_branch with correct args", async () => {
    const config = makeConfig({
      steps: [
        { id: "switch", checked: true },
        { id: "pull", checked: false },
        { id: "delete-local", checked: false },
        { id: "delete-remote", checked: false },
      ],
    });
    await executeCleanup(config);

    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      repoPath: "/repo",
      args: ["status", "--porcelain"],
    });
    expect(mockInvoke).toHaveBeenCalledWith("switch_branch", {
      repoPath: "/repo",
      branch: "main",
      force: false,
      stash: false,
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
      repoPath: "/repo",
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
      repoPath: "/repo",
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
    // Dirty check succeeds, but switch itself fails
    mockInvoke
      .mockResolvedValueOnce({ stdout: "", stderr: "" }) // dirty check
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
    // dirty check, switch, pull, delete-local succeed, delete-remote fails with "does not exist"
    mockInvoke
      .mockResolvedValueOnce({ stdout: "", stderr: "" })  // dirty check
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

  it("pre-checks dirty state before switch and reports clear error", async () => {
    // First invoke: run_git_command (status --porcelain) returns non-empty
    mockInvoke.mockResolvedValueOnce({ stdout: "M src/foo.ts\n", stderr: "" });

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

    expect(onStepDone).toHaveBeenCalledWith("switch", "error", "Working directory has uncommitted changes — commit or stash first");
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
