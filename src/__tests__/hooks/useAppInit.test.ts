import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import "../mocks/tauri";
import { listen } from "@tauri-apps/api/event";
import { makeTerminal } from "../helpers/store";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { mdTabsStore } from "../../stores/mdTabs";
import { initApp, browserCreatedSessions, type AppInitDeps } from "../../hooks/useAppInit";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
  for (const id of mdTabsStore.getIds()) {
    mdTabsStore.remove(id);
  }
}

function createMockDeps(overrides: Partial<AppInitDeps> = {}): AppInitDeps {
  return {
    pty: {
      listActiveSessions: vi.fn().mockResolvedValue([]),
      close: vi.fn().mockResolvedValue(undefined),
    },
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
      startAutoFetch: vi.fn(),
      startPrNotificationTimer: vi.fn(),
      loadFontFromConfig: vi.fn(),
      refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
      startUserActivityListening: vi.fn(),
    },
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

  it("restores active repo/branch and eagerly calls handleBranchSelect", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setActiveBranch("/repo", "main");

    const deps = createMockDeps();
    await initApp(deps);

    expect(deps.setCurrentRepoPath).toHaveBeenCalledWith("/repo");
    expect(deps.setCurrentBranch).toHaveBeenCalledWith("main");
    // Eagerly restore terminals so pane layout IDs match
    expect(deps.handleBranchSelect).toHaveBeenCalledWith("/repo", "main");
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
        startAutoFetch: vi.fn(),
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

  it("snapshots agentSessionId into savedTerminals on beforeunload", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

    // Surviving session so initApp re-adopts and assigns to branch
    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-1", cwd: "/repo" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    // Set agentSessionId on the re-adopted terminal
    const termId = terminalsStore.getIds()[0];
    terminalsStore.update(termId, { agentSessionId: "abc-123-uuid" });

    // Trigger beforeunload to snapshot
    window.dispatchEvent(new Event("beforeunload"));

    const branch = repositoriesStore.get("/repo")?.branches["main"];
    expect(branch?.savedTerminals?.length).toBe(1);
    expect(branch?.savedTerminals?.[0].agentSessionId).toBe("abc-123-uuid");
  });

  it("snapshots null agentSessionId for terminals without it", async () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

    const deps = createMockDeps({
      pty: {
        listActiveSessions: vi.fn().mockResolvedValue([
          { session_id: "sess-2", cwd: "/repo" },
        ]),
        close: vi.fn().mockResolvedValue(undefined),
      },
    });

    await initApp(deps);

    window.dispatchEvent(new Event("beforeunload"));

    const branch = repositoriesStore.get("/repo")?.branches["main"];
    expect(branch?.savedTerminals?.length).toBe(1);
    expect(branch?.savedTerminals?.[0].agentSessionId).toBeNull();
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

  it("beforeunload closes browser-created PTY sessions", async () => {
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

    // Register sess-1 as browser-created (beforeunload only closes these)
    browserCreatedSessions.add("sess-1");

    // Temporarily disable Tauri flag — beforeunload only closes in browser mode
    const saved = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
    delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

    window.dispatchEvent(new Event("beforeunload"));
    expect(closeSpy).toHaveBeenCalledWith("sess-1");

    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = saved;
    browserCreatedSessions.delete("sess-1");
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
        startAutoFetch: vi.fn(),
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
    terminalsStore.add(makeTerminal({ name: "stale", cwd: "/old" }));
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

  describe("head-changed event", () => {
    function captureHeadChanged() {
      const listenMock = vi.mocked(listen);
      let headChangedCallback: ((event: { payload: { repo_path: string; branch: string } }) => void) | null = null;
      listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "head-changed") {
          headChangedCallback = handler as typeof headChangedCallback;
        }
        return Promise.resolve(vi.fn());
      }) as unknown as typeof listen);
      return { listenMock, getCallback: () => headChangedCallback };
    }

    it("renames branch entry when old branch is main checkout (worktreePath null)", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "develop", { worktreePath: null });
      repositoriesStore.setActiveBranch("/repo", "develop");
      repositoriesStore.addTerminalToBranch("/repo", "develop", "term-1");

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "ACME-00106/feature" } });

      // Old branch gone, new branch has it
      expect(repositoriesStore.get("/repo")?.branches["develop"]).toBeUndefined();
      expect(repositoriesStore.get("/repo")?.branches["ACME-00106/feature"]).toBeDefined();
      // Terminals carry over
      expect(repositoriesStore.get("/repo")?.branches["ACME-00106/feature"]?.terminals).toContain("term-1");
      // Active branch updated
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("ACME-00106/feature");
    });

    it("creates new branch entry when old branch is a worktree (worktreePath set)", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "wt-branch", { worktreePath: "/repo/.worktrees/wt-branch" });
      repositoriesStore.setActiveBranch("/repo", "wt-branch");

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "new-branch" } });

      // Old worktree branch preserved
      expect(repositoriesStore.get("/repo")?.branches["wt-branch"]).toBeDefined();
      // New branch created
      expect(repositoriesStore.get("/repo")?.branches["new-branch"]).toBeDefined();
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("new-branch");
    });

    it("sets activeBranch when target branch already exists in store", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: null });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: null });
      repositoriesStore.setActiveBranch("/repo", "feature");

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "main" } });

      // Both branches still exist (feature kept, main was pre-existing)
      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeDefined();
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("main");
    });

    it("does nothing when branch has not changed", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: null });
      repositoriesStore.setActiveBranch("/repo", "main");

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "main" } });

      // Store unchanged
      expect(Object.keys(repositoriesStore.get("/repo")?.branches ?? {})).toEqual(["main"]);
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("main");
    });

    it("moves terminals when new branch already exists in store (race with refreshAllBranchStats)", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "wip/global-config", { worktreePath: null });
      repositoriesStore.setActiveBranch("/repo", "wip/global-config");
      repositoriesStore.addTerminalToBranch("/repo", "wip/global-config", "term-1");
      repositoriesStore.addTerminalToBranch("/repo", "wip/global-config", "term-2");

      // Simulate refreshAllBranchStats creating the new branch before head-changed fires
      repositoriesStore.setBranch("/repo", "wip/memory-system-improvements", { worktreePath: null });

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "wip/memory-system-improvements" } });

      // Terminals moved to new branch
      expect(repositoriesStore.get("/repo")?.branches["wip/memory-system-improvements"]?.terminals).toContain("term-1");
      expect(repositoriesStore.get("/repo")?.branches["wip/memory-system-improvements"]?.terminals).toContain("term-2");
      // Old branch entry removed after merge
      expect(repositoriesStore.get("/repo")?.branches["wip/global-config"]).toBeUndefined();
      // Active branch updated
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("wip/memory-system-improvements");
    });

    it("renames branch entry when old branch is main worktree (worktreePath === repoPath)", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.addTerminalToBranch("/repo", "main", "term-1");

      await initApp(deps);

      getCallback()!({ payload: { repo_path: "/repo", branch: "feat/incremental-reindex" } });

      // Old branch gone — renamed, not duplicated
      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeUndefined();
      // New branch exists with terminals carried over
      expect(repositoriesStore.get("/repo")?.branches["feat/incremental-reindex"]).toBeDefined();
      expect(repositoriesStore.get("/repo")?.branches["feat/incremental-reindex"]?.terminals).toContain("term-1");
      // Active branch updated
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("feat/incremental-reindex");
      // Should NOT create a phantom entry — only one branch in sidebar
      expect(Object.keys(repositoriesStore.get("/repo")?.branches ?? {})).toEqual(["feat/incremental-reindex"]);
    });

    it("does nothing when repo is not found", async () => {
      const { getCallback } = captureHeadChanged();
      const deps = createMockDeps();

      await initApp(deps);

      // Should not throw
      expect(() =>
        getCallback()!({ payload: { repo_path: "/unknown-repo", branch: "main" } })
      ).not.toThrow();
    });
  });

  describe("session-closed event (shellState exited)", () => {
    type SessionClosedPayload = { session_id: string; reason: string; agent_type?: string | null };

    function captureSessionClosed() {
      const listenMock = vi.mocked(listen);
      let callback: ((event: { payload: SessionClosedPayload }) => void) | null = null;
      listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "session-closed") {
          callback = handler as typeof callback;
        }
        return Promise.resolve(vi.fn());
      }) as unknown as typeof listen);
      return { getCallback: () => callback };
    }

    it("sets shellState to exited on the terminal when a remote session closes", async () => {
      const { getCallback } = captureSessionClosed();
      const deps = createMockDeps();
      await initApp(deps);

      const termId = terminalsStore.add({
        sessionId: "remote-sess",
        fontSize: 14,
        name: "Agent",
        cwd: "/tmp",
        awaitingInput: null,
        isRemote: true,
      });

      getCallback()!({ payload: { session_id: "remote-sess", reason: "process_exit", agent_type: "claude" } });

      expect(terminalsStore.get(termId)?.shellState).toBe("exited");
    });

    it("does not set shellState when session_id has no matching terminal", async () => {
      const { getCallback } = captureSessionClosed();
      const deps = createMockDeps();
      await initApp(deps);

      // No terminal registered for this session — should not throw
      expect(() =>
        getCallback()!({ payload: { session_id: "unknown-sess", reason: "process_exit" } })
      ).not.toThrow();
    });
  });

  describe("session-closed auto-close path", () => {
    type SessionCreatedPayload = { session_id: string; cwd: string | null; agent_type?: string | null };
    type SessionClosedPayload = { session_id: string; reason: string; agent_type?: string | null };

    /** Captures both session-created and session-closed callbacks in a single mock pass. */
    function captureCreatedAndClosed() {
      const listenMock = vi.mocked(listen);
      let createdCb: ((event: { payload: SessionCreatedPayload }) => void) | null = null;
      let closedCb: ((event: { payload: SessionClosedPayload }) => void) | null = null;
      listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "session-created") createdCb = handler as typeof createdCb;
        if (event === "session-closed") closedCb = handler as typeof closedCb;
        return Promise.resolve(vi.fn());
      }) as unknown as typeof listen);
      return {
        getCreated: () => createdCb,
        getClosed: () => closedCb,
      };
    }

    afterEach(() => {
      vi.useRealTimers();
    });

    it("auto-removes an agent tab after AGENT_TAB_AUTOCLOSE_MS when agent_type is set", async () => {
      vi.useFakeTimers();
      const { getCreated, getClosed } = captureCreatedAndClosed();
      const deps = createMockDeps();
      await initApp(deps);

      // Register the remote tab via session-created so remoteSessionTabs is populated
      getCreated()!({ payload: { session_id: "agent-sess", cwd: null, agent_type: "claude" } });
      const termId = terminalsStore.getIds().find(id => terminalsStore.get(id)?.sessionId === "agent-sess")!;
      expect(termId).toBeDefined();

      // Fire session-closed with agent_type — triggers AGENT_TAB_AUTOCLOSE_MS (10 000ms)
      getClosed()!({ payload: { session_id: "agent-sess", reason: "process_exit", agent_type: "claude" } });

      // Tab still present before timeout
      expect(terminalsStore.get(termId)).toBeDefined();

      // Advance past the 10s agent autoclose
      vi.advanceTimersByTime(10_001);

      // Tab must be gone
      expect(terminalsStore.get(termId)).toBeUndefined();
    });

    it("auto-removes a remote tab after REMOTE_TAB_AUTOCLOSE_MS when agent_type is absent", async () => {
      vi.useFakeTimers();
      const { getCreated, getClosed } = captureCreatedAndClosed();
      const deps = createMockDeps();
      await initApp(deps);

      getCreated()!({ payload: { session_id: "remote-sess-2", cwd: null, agent_type: null } });
      const termId = terminalsStore.getIds().find(id => terminalsStore.get(id)?.sessionId === "remote-sess-2")!;
      expect(termId).toBeDefined();

      getClosed()!({ payload: { session_id: "remote-sess-2", reason: "process_exit", agent_type: null } });

      // Advancing only 10s must NOT remove the tab (REMOTE uses 30s)
      vi.advanceTimersByTime(10_001);
      expect(terminalsStore.get(termId)).toBeDefined();

      // Advance to just past 30s — tab must be gone
      vi.advanceTimersByTime(20_000);
      expect(terminalsStore.get(termId)).toBeUndefined();
    });
  });

  describe("close-html-tabs event", () => {
    function captureCloseHtmlTabs() {
      const listenMock = vi.mocked(listen);
      let callback: ((event: { payload: { tab_ids: string[] } }) => void) | null = null;
      listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "close-html-tabs") callback = handler as typeof callback;
        return Promise.resolve(vi.fn());
      }) as unknown as typeof listen);
      return { getCallback: () => callback };
    }

    it("closes mdTab UI tabs matching the emitted tab_ids", async () => {
      const { getCallback } = captureCloseHtmlTabs();
      const deps = createMockDeps();
      await initApp(deps);

      // Open two plugin tabs in mdTabsStore
      mdTabsStore.openUiTab("plugin-a", "Plugin A", "<p>a</p>", false, undefined, false);
      mdTabsStore.openUiTab("plugin-b", "Plugin B", "<p>b</p>", false, undefined, false);

      const tabsBefore = Object.values(mdTabsStore.state.tabs).filter(t => t.type === "plugin-panel");
      expect(tabsBefore).toHaveLength(2);

      // Fire close-html-tabs for one of them
      getCallback()!({ payload: { tab_ids: ["plugin-a"] } });

      const remaining = Object.values(mdTabsStore.state.tabs).filter(t => t.type === "plugin-panel");
      expect(remaining).toHaveLength(1);
      expect(remaining[0].title).toBe("Plugin B");
    });

    it("is a no-op for unknown tab_ids", async () => {
      const { getCallback } = captureCloseHtmlTabs();
      const deps = createMockDeps();
      await initApp(deps);

      mdTabsStore.openUiTab("plugin-c", "Plugin C", "<p>c</p>", false, undefined, false);

      // Should not throw for IDs that don't exist
      expect(() =>
        getCallback()!({ payload: { tab_ids: ["nonexistent-id"] } })
      ).not.toThrow();

      // Existing tab untouched
      const remaining = Object.values(mdTabsStore.state.tabs).filter(t => t.type === "plugin-panel");
      expect(remaining).toHaveLength(1);
    });
  });

  describe("session-created event (agent tab activation)", () => {
    type SessionCreatedPayload = { session_id: string; cwd: string | null; agent_type?: string | null };

    function captureSessionCreated() {
      const listenMock = vi.mocked(listen);
      let callback: ((event: { payload: SessionCreatedPayload }) => void) | null = null;
      listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "session-created") {
          callback = handler as typeof callback;
        }
        return Promise.resolve(vi.fn());
      }) as unknown as typeof listen);
      return { getCallback: () => callback };
    }

    beforeEach(() => {
      paneLayoutStore.reset();
      resetGroupCounter();
    });

    it("setActive not called when active terminal already exists", async () => {
      const { getCallback } = captureSessionCreated();
      const deps = createMockDeps();
      await initApp(deps);

      // Pre-existing active terminal
      const existingId = terminalsStore.add({ sessionId: "existing", fontSize: 14, name: "Existing", cwd: "/tmp", awaitingInput: null });
      terminalsStore.setActive(existingId);

      const setActiveSpy = vi.spyOn(terminalsStore, "setActive");
      getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

      expect(setActiveSpy).not.toHaveBeenCalled();
      setActiveSpy.mockRestore();
    });

    it("setActive called when no active terminal exists", async () => {
      const { getCallback } = captureSessionCreated();
      const deps = createMockDeps();
      await initApp(deps);

      expect(terminalsStore.state.activeId).toBeNull();

      getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

      const newId = terminalsStore.getIds().find(id => terminalsStore.get(id)?.sessionId === "new-sess");
      expect(newId).toBeDefined();
      expect(terminalsStore.state.activeId).toBe(newId);
    });

    it("setActiveGroup called with first leaf when split but no active group", async () => {
      const { getCallback } = captureSessionCreated();
      const deps = createMockDeps();
      await initApp(deps);

      // Set up split mode with two groups but no activeGroupId
      const g1 = paneLayoutStore.createGroup();
      const g2 = paneLayoutStore.createGroup();
      paneLayoutStore.setRoot({
        type: "branch",
        direction: "horizontal",
        children: [{ type: "leaf", id: g1 }, { type: "leaf", id: g2 }],
        ratios: [0.5, 0.5],
      });
      // activeGroupId should be null since we called setRoot directly (not split())
      expect(paneLayoutStore.state.activeGroupId).toBeNull();

      const setActiveGroupSpy = vi.spyOn(paneLayoutStore, "setActiveGroup");
      getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

      expect(setActiveGroupSpy).toHaveBeenCalledWith(g1);
      setActiveGroupSpy.mockRestore();
    });
  });
});
