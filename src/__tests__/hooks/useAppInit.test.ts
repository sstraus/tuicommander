import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { listen } from "@tauri-apps/api/event";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { initApp, type AppInitDeps } from "../../hooks/useAppInit";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
}

function createMockDeps(overrides: Partial<AppInitDeps> = {}): AppInitDeps {
  return {
    pty: {
      listActiveSessions: vi.fn().mockResolvedValue([]),
      close: vi.fn().mockResolvedValue(undefined),
    },
    setLazygitAvailable: vi.fn(),
    setQuitDialogVisible: vi.fn(),
    setStatusInfo: vi.fn(),
    setCurrentRepoPath: vi.fn(),
    setCurrentBranch: vi.fn(),
    handleBranchSelect: vi.fn().mockResolvedValue(undefined),
    refreshAllBranchStats: vi.fn(),
    getDefaultFontSize: () => 14,
    stores: {
      hydrate: vi.fn().mockResolvedValue(undefined),
      startPolling: vi.fn(),
      stopPolling: vi.fn(),
      startPrNotificationTimer: vi.fn(),
      loadFontFromConfig: vi.fn(),
      refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
      startUserActivityListening: vi.fn(),
    },
    detectBinary: vi.fn().mockResolvedValue({ path: null, version: null }),
    applyPlatformClass: vi.fn().mockReturnValue("macos"),
    onCloseRequested: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe("initApp", () => {
  beforeEach(() => {
    resetStores();
  });

  it("hydrates stores and detects platform", async () => {
    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.applyPlatformClass).toHaveBeenCalled();
    expect(deps.stores.hydrate).toHaveBeenCalled();
    expect(deps.stores.loadFontFromConfig).toHaveBeenCalled();
  });

  it("detects lazygit binary", async () => {
    const deps = createMockDeps({
      detectBinary: vi.fn().mockResolvedValue({ path: "/usr/bin/lazygit", version: "0.40" }),
    });

    await initApp(deps);

    expect(deps.setLazygitAvailable).toHaveBeenCalledWith(true);
  });

  it("handles lazygit detection failure", async () => {
    const deps = createMockDeps({
      detectBinary: vi.fn().mockRejectedValue(new Error("failed")),
    });

    await initApp(deps);

    expect(deps.setLazygitAvailable).toHaveBeenCalledWith(false);
  });

  it("re-adopts surviving PTY sessions", async () => {
    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/repo" },
          { session_id: "sess-2", cwd: "/other" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    expect(terminalsStore.getCount()).toBe(2);
    const ids = terminalsStore.getIds();
    expect(terminalsStore.get(ids[0])?.sessionId).toBe("sess-1");
    expect(terminalsStore.get(ids[1])?.sessionId).toBe("sess-2");
  });

  it("matches surviving sessions to repos by cwd", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/repo" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    const branch = repositoriesStore.get("/repo")?.branches["main"];
    expect(branch?.terminals.length).toBe(1);
  });

  it("restores active repo/branch visual state without creating terminals", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setActiveBranch("/repo", "main");

    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.setCurrentRepoPath).toHaveBeenCalledWith("/repo");
    expect(deps.setCurrentBranch).toHaveBeenCalledWith("main");
    // Lazy restore: no terminals created on startup, no handleBranchSelect called
    expect(deps.handleBranchSelect).not.toHaveBeenCalled();
    expect(terminalsStore.getCount()).toBe(0);
  });

  it("does not create terminals when repos exist but no active branch (lazy restore)", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    // No setBranch/setActiveBranch, so activeBranch is undefined

    const deps = createMockDeps();
    await initApp(deps);

    // Lazy restore: no terminals created on startup
    expect(terminalsStore.getCount()).toBe(0);
  });

  it("reports hydration failures in status", async () => {
    const deps = createMockDeps({
      stores: {
        hydrate: vi.fn().mockRejectedValue(new Error("hydration failed")),
        startPolling: vi.fn(),
        stopPolling: vi.fn(),
        startPrNotificationTimer: vi.fn(),
        loadFontFromConfig: vi.fn(),
        refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
        startUserActivityListening: vi.fn(),
      },
    });

    await initApp(deps);

    expect(deps.setStatusInfo).toHaveBeenCalledWith(expect.stringContaining("failed to load"));
  });

  it("starts GitHub polling", async () => {
    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.stores.startPolling).toHaveBeenCalled();
  });

  it("refreshes all branch stats", async () => {
    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.refreshAllBranchStats).toHaveBeenCalled();
  });

  it("matches surviving session to worktree by cwd", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt-feature" });

    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/repo/wt-feature" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    const branch = repositoriesStore.get("/repo")?.branches["feature"];
    expect(branch?.terminals.length).toBe(1);
  });

  it("restores active branch with surviving sessions", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setActiveBranch("/repo", "main");

    // Add a terminal that will be cleared and re-adopted
    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/repo" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    expect(deps.setCurrentRepoPath).toHaveBeenCalledWith("/repo");
    expect(deps.setCurrentBranch).toHaveBeenCalledWith("main");
    // Should activate an existing terminal, not call handleBranchSelect
    const ids = terminalsStore.getIds();
    expect(ids.length).toBe(1);
  });

  it("calls handleBranchSelect when surviving sessions have no valid terminals", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setActiveBranch("/repo", "main");

    // Surviving session CWD doesn't match the branch
    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/other" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    // The session is adopted but not matched to main branch terminals
    // So handleBranchSelect should be called to create a proper terminal
    expect(deps.handleBranchSelect).toHaveBeenCalledWith("/repo", "main");
  });

  it("registers beforeunload handler to close PTY sessions", async () => {
    const addListenerSpy = vi.spyOn(window, "addEventListener");
    const deps = createMockDeps();
    await initApp(deps);

    expect(addListenerSpy).toHaveBeenCalledWith("beforeunload", expect.any(Function));
    addListenerSpy.mockRestore();
  });

  it("refreshes dictation config", async () => {
    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.stores.refreshDictationConfig).toHaveBeenCalled();
  });

  it("onCloseRequested prevents close when active terminals exist", async () => {
    let capturedCallback: ((event: { preventDefault: () => void }) => void) | null = null;
    const deps = createMockDeps({
      onCloseRequested: vi.fn((cb: (event: { preventDefault: () => void }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(undefined);
      }) as AppInitDeps["onCloseRequested"],
    });

    await initApp(deps);

    // Add a terminal with a session
    terminalsStore.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: "/tmp", awaitingInput: null });

    const preventDefaultSpy = vi.fn();
    capturedCallback!({ preventDefault: preventDefaultSpy });
    expect(preventDefaultSpy).toHaveBeenCalled();
    expect(deps.setQuitDialogVisible).toHaveBeenCalledWith(true);
  });

  it("onCloseRequested allows close when no active terminals", async () => {
    let capturedCallback: ((event: { preventDefault: () => void }) => void) | null = null;
    const deps = createMockDeps({
      onCloseRequested: vi.fn((cb: (event: { preventDefault: () => void }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(undefined);
      }) as AppInitDeps["onCloseRequested"],
    });

    await initApp(deps);

    const preventDefaultSpy = vi.fn();
    capturedCallback!({ preventDefault: preventDefaultSpy });
    expect(preventDefaultSpy).not.toHaveBeenCalled();
    expect(deps.setQuitDialogVisible).not.toHaveBeenCalled();
  });

  it("beforeunload closes all PTY sessions", async () => {
    const closeSpy = vi.fn().mockResolvedValue(undefined);
    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/tmp" },
        ]),
        close: closeSpy,
      },
    });

    await initApp(deps);

    // Terminals should be re-adopted — trigger beforeunload
    window.dispatchEvent(new Event("beforeunload"));
    expect(closeSpy).toHaveBeenCalledWith("sess-1");
  });

  it("removes splash screen after hydration", async () => {
    const splash = document.createElement("div");
    splash.id = "splash";
    document.body.appendChild(splash);

    const deps = createMockDeps();
    await initApp(deps);

    expect(document.getElementById("splash")).toBeNull();
  });

  it("removes splash screen even when hydration fails", async () => {
    const splash = document.createElement("div");
    splash.id = "splash";
    document.body.appendChild(splash);

    const deps = createMockDeps({
      stores: {
        hydrate: vi.fn().mockRejectedValue(new Error("hydration failed")),
        startPolling: vi.fn(),
        stopPolling: vi.fn(),
        startPrNotificationTimer: vi.fn(),
        loadFontFromConfig: vi.fn(),
        refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
        startUserActivityListening: vi.fn(),
      },
    });

    await initApp(deps);

    expect(document.getElementById("splash")).toBeNull();
  });

  it("clears stale terminals from previous session", async () => {
    // Pre-populate stale terminals
    terminalsStore.add({ sessionId: null, fontSize: 14, name: "stale", cwd: "/old", awaitingInput: null });
    expect(terminalsStore.getCount()).toBe(1);

    const deps = createMockDeps();
    await initApp(deps);

    // Stale terminal should be removed, and a new fallback terminal created
    const ids = terminalsStore.getIds();
    for (const id of ids) {
      expect(terminalsStore.get(id)?.name).not.toBe("stale");
    }
  });

  it("repo-changed event triggers debounced refreshAllBranchStats", async () => {
    vi.useFakeTimers();

    // Capture the "repo-changed" listener callback
    const listenMock = vi.mocked(listen);
    let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
    listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "repo-changed") {
        repoChangedCallback = handler as typeof repoChangedCallback;
      }
      return Promise.resolve(vi.fn());
    }) as unknown as typeof listen);

    const deps = createMockDeps();
    await initApp(deps);

    // refreshAllBranchStats is called once during init
    expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(1);

    // Simulate repo-changed event
    expect(repoChangedCallback).not.toBeNull();
    repoChangedCallback!({ payload: { repo_path: "/repo" } });

    // Should not fire immediately (debounced)
    expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(1);

    // After debounce period (500ms), should fire
    await vi.advanceTimersByTimeAsync(500);
    expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2);

    vi.useRealTimers();
  });

  it("repo-changed debounce coalesces rapid events", async () => {
    vi.useFakeTimers();

    const listenMock = vi.mocked(listen);
    let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
    listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "repo-changed") {
        repoChangedCallback = handler as typeof repoChangedCallback;
      }
      return Promise.resolve(vi.fn());
    }) as unknown as typeof listen);

    const deps = createMockDeps();
    await initApp(deps);

    // Fire 5 rapid events
    for (let i = 0; i < 5; i++) {
      repoChangedCallback!({ payload: { repo_path: "/repo" } });
    }

    // After debounce (500ms), should only have called refreshAllBranchStats once more (not 5 times)
    await vi.advanceTimersByTimeAsync(500);
    expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2); // 1 init + 1 debounced

    vi.useRealTimers();
  });
});
