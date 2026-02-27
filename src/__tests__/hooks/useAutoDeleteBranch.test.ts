import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { createRoot } from "solid-js";

const {
  mockSetOnPrTerminal,
  mockGetEffective,
  mockGet,
  mockBumpRevision,
  mockConfirm,
} = vi.hoisted(() => ({
  mockSetOnPrTerminal: vi.fn(),
  mockGetEffective: vi.fn(),
  mockGet: vi.fn(),
  mockBumpRevision: vi.fn(),
  mockConfirm: vi.fn(),
}));

vi.mock("../../stores/github", () => ({
  githubStore: {
    setOnPrTerminal: mockSetOnPrTerminal,
  },
}));

vi.mock("../../stores/repoSettings", () => ({
  repoSettingsStore: {
    getEffective: mockGetEffective,
  },
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    get: mockGet,
    bumpRevision: mockBumpRevision,
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

import { useAutoDeleteBranch } from "../../hooks/useAutoDeleteBranch";

/** Simulate the prTerminal callback by capturing what setOnPrTerminal received */
function getCapturedCallback(): (repoPath: string, branch: string, prNumber: number, type: "merged" | "closed") => void {
  return mockSetOnPrTerminal.mock.calls[0][0];
}

describe("useAutoDeleteBranch", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(undefined);
    mockConfirm.mockResolvedValue(true);
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
    mockGet.mockReturnValue({
      branches: {
        "feature/x": { name: "feature/x", isMain: false },
      },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function setup(): (() => void) | undefined {
    let dispose: (() => void) | undefined;
    createRoot((d) => {
      dispose = d;
      useAutoDeleteBranch({ confirm: mockConfirm });
    });
    return dispose;
  }

  it("registers prTerminal callback on mount", () => {
    const dispose = setup();
    expect(mockSetOnPrTerminal).toHaveBeenCalledWith(expect.any(Function));
    dispose?.();
  });

  it("unregisters callback on cleanup", () => {
    const dispose = setup();
    dispose?.();
    // Last call should be null (cleanup)
    const calls = mockSetOnPrTerminal.mock.calls;
    expect(calls[calls.length - 1][0]).toBeNull();
  });

  it("does nothing when setting is off", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "merged");
    await vi.waitFor(() => {});

    expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
    expect(mockConfirm).not.toHaveBeenCalled();
    dispose?.();
  });

  it("auto-deletes silently when setting is auto and worktree is clean", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "check_worktree_dirty") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "merged");

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
        repoPath: "/repo1",
        branchName: "feature/x",
      });
    });

    expect(mockConfirm).not.toHaveBeenCalled();
    expect(mockBumpRevision).toHaveBeenCalledWith("/repo1");
    dispose?.();
  });

  it("falls back to ask when auto mode and worktree is dirty", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "check_worktree_dirty") return Promise.resolve(true);
      return Promise.resolve(undefined);
    });
    mockConfirm.mockResolvedValue(true);
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "merged");

    await vi.waitFor(() => {
      expect(mockConfirm).toHaveBeenCalledWith(expect.objectContaining({
        title: "Delete local branch?",
      }));
    });

    expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
      repoPath: "/repo1",
      branchName: "feature/x",
    });
    dispose?.();
  });

  it("shows confirm dialog when setting is ask", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
    mockConfirm.mockResolvedValue(true);
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "closed");

    await vi.waitFor(() => {
      expect(mockConfirm).toHaveBeenCalledWith(expect.objectContaining({
        message: expect.stringContaining("PR #42 was closed"),
      }));
    });

    expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
      repoPath: "/repo1",
      branchName: "feature/x",
    });
    dispose?.();
  });

  it("does not delete when user cancels confirm dialog", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
    mockConfirm.mockResolvedValue(false);
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "merged");

    await vi.waitFor(() => {
      expect(mockConfirm).toHaveBeenCalled();
    });

    expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
    dispose?.();
  });

  it("never deletes the main branch", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
    mockGet.mockReturnValue({
      branches: {
        main: { name: "main", isMain: true },
      },
    });
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "main", 99, "merged");
    await vi.waitFor(() => {});

    expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
    expect(mockConfirm).not.toHaveBeenCalled();
    dispose?.();
  });

  it("skips branch that does not exist locally", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
    mockGet.mockReturnValue({
      branches: {
        "other-branch": { name: "other-branch", isMain: false },
      },
    });
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "nonexistent-branch", 42, "merged");
    await vi.waitFor(() => {});

    expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
    dispose?.();
  });

  it("deduplicates same PR transition", async () => {
    mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "check_worktree_dirty") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });
    const dispose = setup();
    const cb = getCapturedCallback();

    cb("/repo1", "feature/x", 42, "merged");
    cb("/repo1", "feature/x", 42, "merged"); // duplicate

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", expect.anything());
    });

    const deleteCalls = mockInvoke.mock.calls.filter(
      (c) => c[0] === "delete_local_branch",
    );
    expect(deleteCalls).toHaveLength(1);
    dispose?.();
  });
});
