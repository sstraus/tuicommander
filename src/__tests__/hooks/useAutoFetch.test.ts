import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const mockInvoke = vi.fn().mockResolvedValue({ success: true, stdout: "", stderr: "", exit_code: 0 });

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

const mockGetEffective = vi.fn<(path: string) => any>(() => undefined);
const mockGetOrderedRepos = vi.fn<() => any[]>(() => []);
const mockBumpRevision = vi.fn();

vi.mock("../../stores/repoSettings", () => ({
  repoSettingsStore: {
    getEffective: mockGetEffective,
  },
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    getOrderedRepos: mockGetOrderedRepos,
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

describe("useAutoFetch", () => {
  let startAutoFetch: typeof import("../../hooks/useAutoFetch").startAutoFetch;
  let stopAutoFetch: typeof import("../../hooks/useAutoFetch").stopAutoFetch;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue({ success: true, stdout: "", stderr: "", exit_code: 0 });
    mockGetEffective.mockReset().mockReturnValue(undefined);
    mockGetOrderedRepos.mockReset().mockReturnValue([]);
    mockBumpRevision.mockReset();

    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    vi.doMock("../../stores/repoSettings", () => ({
      repoSettingsStore: { getEffective: mockGetEffective },
    }));
    vi.doMock("../../stores/repositories", () => ({
      repositoriesStore: {
        getOrderedRepos: mockGetOrderedRepos,
        bumpRevision: mockBumpRevision,
      },
    }));
    vi.doMock("../../stores/appLogger", () => ({
      appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
    }));

    const mod = await import("../../hooks/useAutoFetch");
    startAutoFetch = mod.startAutoFetch;
    stopAutoFetch = mod.stopAutoFetch;
  });

  afterEach(() => {
    stopAutoFetch();
    vi.useRealTimers();
  });

  it("does nothing when no repos have auto-fetch enabled", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 0 });

    startAutoFetch();
    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

    expect(mockInvoke).not.toHaveBeenCalledWith("run_git_command", expect.anything());
  });

  it("fetches at configured interval", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();

    // Tick at 1 minute — first tick triggers fetch since lastFetchAt starts at 0
    await vi.advanceTimersByTimeAsync(60 * 1000);

    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      path: "/repo1",
      args: ["fetch", "--all"],
    });
  });

  it("bumps revision after successful fetch", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();
    await vi.advanceTimersByTimeAsync(60 * 1000);

    expect(mockBumpRevision).toHaveBeenCalledWith("/repo1");
  });

  it("does not bump revision on failed fetch", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });
    mockInvoke.mockResolvedValue({ success: false, stdout: "", stderr: "error", exit_code: 1 });

    startAutoFetch();
    await vi.advanceTimersByTimeAsync(60 * 1000);

    expect(mockBumpRevision).not.toHaveBeenCalled();
  });

  it("does not re-fetch before interval elapses", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();

    // First tick at 1min triggers fetch
    await vi.advanceTimersByTimeAsync(60 * 1000);
    expect(mockInvoke).toHaveBeenCalledTimes(1);

    mockInvoke.mockClear();

    // Advance 3 more minutes (total 4 min from first fetch) — should NOT fetch yet
    await vi.advanceTimersByTimeAsync(3 * 60 * 1000);
    expect(mockInvoke).not.toHaveBeenCalledWith("run_git_command", expect.anything());

    // Advance 2 more minutes (total 6 min from start, >5min from first fetch) — should fetch
    await vi.advanceTimersByTimeAsync(2 * 60 * 1000);
    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", {
      path: "/repo1",
      args: ["fetch", "--all"],
    });
  });

  it("handles multiple repos with different intervals", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }, { path: "/repo2" }]);
    mockGetEffective.mockImplementation((path: string) => ({
      autoFetchIntervalMinutes: path === "/repo1" ? 5 : 15,
    }));

    startAutoFetch();

    // First tick at 1min: both repos get initial fetch
    await vi.advanceTimersByTimeAsync(60 * 1000);
    const initialCalls = mockInvoke.mock.calls.filter(
      (c) => c[0] === "run_git_command",
    );
    expect(initialCalls).toHaveLength(2);

    mockInvoke.mockClear();

    // At 6 minutes total (5min since first fetch): repo1 refetches, repo2 doesn't
    await vi.advanceTimersByTimeAsync(5 * 60 * 1000);
    const calls5min = mockInvoke.mock.calls.filter(
      (c) => c[0] === "run_git_command",
    );
    expect(calls5min).toHaveLength(1);
    expect(calls5min[0][1].path).toBe("/repo1");
  });

  it("stopAutoFetch clears all timers", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();
    stopAutoFetch();

    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

    expect(mockInvoke).not.toHaveBeenCalledWith("run_git_command", expect.anything());
  });

  it("respects setting changes on restart", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();
    await vi.advanceTimersByTimeAsync(60 * 1000);
    expect(mockInvoke).toHaveBeenCalledWith("run_git_command", expect.anything());

    mockInvoke.mockClear();

    // Disable and restart
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 0 });
    stopAutoFetch();
    startAutoFetch();

    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

    expect(mockInvoke).not.toHaveBeenCalledWith("run_git_command", expect.anything());
  });

  it("does not double-fetch if startAutoFetch called multiple times", async () => {
    mockGetOrderedRepos.mockReturnValue([{ path: "/repo1" }]);
    mockGetEffective.mockReturnValue({ autoFetchIntervalMinutes: 5 });

    startAutoFetch();
    startAutoFetch(); // Second call should replace, not double

    await vi.advanceTimersByTimeAsync(60 * 1000);

    const calls = mockInvoke.mock.calls.filter(
      (c) => c[0] === "run_git_command",
    );
    expect(calls).toHaveLength(1);
  });
});
