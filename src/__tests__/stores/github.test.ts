import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import type { BranchPrStatus } from "../../types";
import { testInScope, testInScopeAsync } from "../helpers/store";

describe("githubStore", () => {
  let store: typeof import("../../stores/github").githubStore;

  // Mock repositoriesStore to return controlled repo paths
  const mockGetPaths = vi.fn<() => string[]>(() => ["/repo1"]);

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    localStorage.clear();
    mockInvoke.mockReset();
    mockGetPaths.mockReturnValue(["/repo1"]);

    vi.doMock("../../stores/repositories", () => ({
      repositoriesStore: {
        getPaths: mockGetPaths,
      },
    }));

    store = (await import("../../stores/github")).githubStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makePrStatus(overrides: Partial<BranchPrStatus> = {}): BranchPrStatus {
    return {
      branch: "feature/x",
      number: 42,
      title: "Add feature",
      state: "OPEN",
      url: "https://github.com/org/repo/pull/42",
      additions: 10,
      deletions: 5,
      checks: { passed: 2, failed: 0, pending: 0, total: 2 },
      check_details: [
        { context: "build", state: "SUCCESS" },
        { context: "test", state: "SUCCESS" },
      ],
      author: "alice",
      commits: 3,
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      review_decision: "",
      labels: [],
      is_draft: false,
      base_ref_name: "main",
      head_ref_oid: "abc1234",
      created_at: "2026-01-15T10:00:00Z",
      updated_at: "2026-01-15T12:00:00Z",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: null,
      merge_commit_allowed: true,
      squash_merge_allowed: true,
      rebase_merge_allowed: true,
      ...overrides,
    };
  }

  describe("initialization", () => {
    it("starts with empty repos state", () => {
      testInScope(() => {
        expect(store.state.repos).toEqual({});
      });
    });
  });

  describe("updateRepoData()", () => {
    it("sets branch PR data for a repo", () => {
      testInScope(() => {
        const prData = [makePrStatus()];
        store.updateRepoData("/repo1", prData);

        expect(store.state.repos["/repo1"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
      });
    });

    it("handles multiple branches in one repo", () => {
      testInScope(() => {
        const prData = [
          makePrStatus({ branch: "feature/x", number: 42 }),
          makePrStatus({ branch: "fix/y", number: 43 }),
        ];
        store.updateRepoData("/repo1", prData);

        expect(Object.keys(store.state.repos["/repo1"].branches)).toHaveLength(2);
        expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
        expect(store.state.repos["/repo1"].branches["fix/y"].number).toBe(43);
      });
    });

    it("removes stale branches no longer in poll results", () => {
      testInScope(() => {
        // First update adds two branches
        store.updateRepoData("/repo1", [
          makePrStatus({ branch: "feature/x" }),
          makePrStatus({ branch: "feature/y" }),
        ]);
        expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/y"]).toBeDefined();

        // Second update only has feature/x — feature/y should be removed
        store.updateRepoData("/repo1", [
          makePrStatus({ branch: "feature/x" }),
        ]);
        expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/y"]).toBeUndefined();
      });
    });

    it("updates lastPolled timestamp", () => {
      testInScope(() => {
        const before = Date.now();
        store.updateRepoData("/repo1", [makePrStatus()]);
        const after = Date.now();

        expect(store.state.repos["/repo1"].lastPolled).toBeGreaterThanOrEqual(before);
        expect(store.state.repos["/repo1"].lastPolled).toBeLessThanOrEqual(after);
      });
    });
  });

  describe("getCheckSummary()", () => {
    it("returns check summary for known branch", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [
          makePrStatus({ checks: { passed: 3, failed: 1, pending: 2, total: 6 } }),
        ]);

        const summary = store.getCheckSummary("/repo1", "feature/x");
        expect(summary).toEqual({ passed: 3, failed: 1, pending: 2, total: 6 });
      });
    });

    it("returns null for unknown repo", () => {
      testInScope(() => {
        expect(store.getCheckSummary("/unknown", "feature/x")).toBeNull();
      });
    });

    it("returns null for unknown branch", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [makePrStatus()]);
        expect(store.getCheckSummary("/repo1", "nonexistent")).toBeNull();
      });
    });
  });

  describe("getPrStatus()", () => {
    it("returns PR status for known branch", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [makePrStatus()]);

        const pr = store.getPrStatus("/repo1", "feature/x");
        expect(pr).not.toBeNull();
        expect(pr!.number).toBe(42);
        expect(pr!.title).toBe("Add feature");
        expect(pr!.state).toBe("OPEN");
        expect(pr!.url).toBe("https://github.com/org/repo/pull/42");
      });
    });

    it("returns null for unknown branches", () => {
      testInScope(() => {
        expect(store.getPrStatus("/unknown", "feature/x")).toBeNull();
      });
    });
  });

  describe("getCheckDetails()", () => {
    it("returns check details for known branch", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [makePrStatus()]);

        const details = store.getCheckDetails("/repo1", "feature/x");
        expect(details).toHaveLength(2);
        expect(details[0].context).toBe("build");
        expect(details[1].context).toBe("test");
      });
    });

    it("returns empty array for unknown branch", () => {
      testInScope(() => {
        expect(store.getCheckDetails("/unknown", "feature/x")).toEqual([]);
      });
    });
  });

  describe("getBranchPrData()", () => {
    it("returns full BranchPrStatus for known branch", () => {
      testInScope(() => {
        const pr = makePrStatus();
        store.updateRepoData("/repo1", [pr]);

        const data = store.getBranchPrData("/repo1", "feature/x");
        expect(data).not.toBeNull();
        expect(data!.author).toBe("alice");
        expect(data!.commits).toBe(3);
        expect(data!.additions).toBe(10);
        expect(data!.deletions).toBe(5);
      });
    });

    it("returns null for unknown branch", () => {
      testInScope(() => {
        expect(store.getBranchPrData("/repo1", "no-branch")).toBeNull();
      });
    });
  });

  describe("detectTransitions()", () => {
    let notifStore: typeof import("../../stores/prNotifications").prNotificationsStore;

    beforeEach(async () => {
      // Import prNotificationsStore from the same module registry created by outer beforeEach
      notifStore = (await import("../../stores/prNotifications")).prNotificationsStore;
      notifStore.clearAll();
    });

    afterEach(() => {
      notifStore.clearAll();
    });

    /** Seed initial branch state and then update to trigger detectTransitions */
    function transition(
      oldOverrides: Partial<BranchPrStatus>,
      newOverrides: Partial<BranchPrStatus>,
    ) {
      store.updateRepoData("/repo1", [makePrStatus(oldOverrides)]);
      store.updateRepoData("/repo1", [makePrStatus(newOverrides)]);
    }

    it("emits 'merged' when OPEN → MERGED", () => {
      testInScope(() => {
        transition({ state: "OPEN" }, { state: "MERGED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("merged");
        expect(active[0].prNumber).toBe(42);
      });
    });

    it("emits 'merged' when CLOSED → MERGED", () => {
      testInScope(() => {
        transition({ state: "CLOSED" }, { state: "MERGED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("merged");
      });
    });

    it("emits 'closed' when OPEN → CLOSED", () => {
      testInScope(() => {
        transition({ state: "OPEN" }, { state: "CLOSED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("closed");
      });
    });

    it("emits 'blocked' when mergeable becomes CONFLICTING on open PR", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", mergeable: "MERGEABLE" },
          { state: "OPEN", mergeable: "CONFLICTING" },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("blocked");
      });
    });

    it("emits 'ci_failed' when failed checks go from 0 to >0 on open PR", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", checks: { passed: 2, failed: 0, pending: 0, total: 2 } },
          { state: "OPEN", checks: { passed: 1, failed: 1, pending: 0, total: 2 } },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("ci_failed");
      });
    });

    it("emits 'changes_requested' when review_decision becomes CHANGES_REQUESTED", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", review_decision: "" },
          { state: "OPEN", review_decision: "CHANGES_REQUESTED" },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("changes_requested");
      });
    });

    it("emits 'ready' when PR becomes mergeable+approved+no-failures", () => {
      testInScope(() => {
        // Old: mergeable but not approved, so not 'ready'
        transition(
          {
            state: "OPEN",
            mergeable: "MERGEABLE",
            review_decision: "",
            checks: { passed: 2, failed: 0, pending: 0, total: 2 },
          },
          {
            state: "OPEN",
            mergeable: "MERGEABLE",
            review_decision: "APPROVED",
            checks: { passed: 2, failed: 0, pending: 0, total: 2 },
          },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("ready");
      });
    });

    it("emits 'ready' when PR goes from conflicting to mergeable+approved", () => {
      testInScope(() => {
        transition(
          {
            state: "OPEN",
            mergeable: "CONFLICTING",
            review_decision: "APPROVED",
            checks: { passed: 2, failed: 0, pending: 0, total: 2 },
          },
          {
            state: "OPEN",
            mergeable: "MERGEABLE",
            review_decision: "APPROVED",
            checks: { passed: 2, failed: 0, pending: 0, total: 2 },
          },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("ready");
      });
    });

    it("emits 'ready' when CI failures are resolved and PR is otherwise ready", () => {
      testInScope(() => {
        transition(
          {
            state: "OPEN",
            mergeable: "MERGEABLE",
            review_decision: "APPROVED",
            checks: { passed: 1, failed: 1, pending: 0, total: 2 },
          },
          {
            state: "OPEN",
            mergeable: "MERGEABLE",
            review_decision: "APPROVED",
            checks: { passed: 2, failed: 0, pending: 0, total: 2 },
          },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("ready");
      });
    });

    it("does not emit when state is unchanged", () => {
      testInScope(() => {
        transition({ state: "OPEN" }, { state: "OPEN" });
        expect(notifStore.getActive()).toHaveLength(0);
      });
    });

    it("does not emit on first update (no prior data)", () => {
      testInScope(() => {
        // Only one updateRepoData call — no prior data, no transitions
        store.updateRepoData("/repo1", [makePrStatus({ state: "MERGED" })]);
        expect(notifStore.getActive()).toHaveLength(0);
      });
    });

    it("does not emit 'blocked' when already CONFLICTING", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", mergeable: "CONFLICTING" },
          { state: "OPEN", mergeable: "CONFLICTING" },
        );
        expect(notifStore.getActive()).toHaveLength(0);
      });
    });

    it("does not emit 'ci_failed' when already had failures", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", checks: { passed: 1, failed: 1, pending: 0, total: 2 } },
          { state: "OPEN", checks: { passed: 0, failed: 2, pending: 0, total: 2 } },
        );
        expect(notifStore.getActive()).toHaveLength(0);
      });
    });

    it("does not emit 'blocked' or other OPEN transitions for closed PRs", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", mergeable: "MERGEABLE" },
          { state: "CLOSED", mergeable: "CONFLICTING" },
        );
        // 'closed' is emitted, but NOT 'blocked' (since PR is not OPEN in new state)
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("closed");
      });
    });

    it("includes correct branch and title in notification", () => {
      testInScope(() => {
        transition(
          { state: "OPEN", branch: "my/feature", title: "My Feature PR" },
          { state: "MERGED", branch: "my/feature", title: "My Feature PR" },
        );
        const active = notifStore.getActive();
        expect(active[0].branch).toBe("my/feature");
        expect(active[0].title).toBe("My Feature PR");
        expect(active[0].repoPath).toBe("/repo1");
      });
    });

    it("fires prTerminal callback on merged transition", () => {
      testInScope(() => {
        const cb = vi.fn();
        store.setOnPrTerminal(cb);
        transition({ state: "OPEN" }, { state: "MERGED" });
        expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42, "merged");
        store.setOnPrTerminal(null);
      });
    });

    it("fires prTerminal callback on closed transition", () => {
      testInScope(() => {
        const cb = vi.fn();
        store.setOnPrTerminal(cb);
        transition({ state: "OPEN" }, { state: "CLOSED" });
        expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42, "closed");
        store.setOnPrTerminal(null);
      });
    });

    it("does NOT fire prTerminal callback on non-terminal transitions", () => {
      testInScope(() => {
        const cb = vi.fn();
        store.setOnPrTerminal(cb);
        transition(
          { state: "OPEN", mergeable: "MERGEABLE" },
          { state: "OPEN", mergeable: "CONFLICTING" },
        );
        expect(cb).not.toHaveBeenCalled();
        store.setOnPrTerminal(null);
      });
    });
  });

  describe("polling", () => {
    // pollAll() checks the circuit breaker before polling — let it pass by default
    beforeEach(() => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        return Promise.resolve(null);
      });
    });

    it("polls repos on startPolling using batched get_all_pr_statuses", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [makePrStatus()] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();

        // Flush the initial poll microtask
        await vi.advanceTimersByTimeAsync(0);

        expect(mockInvoke).toHaveBeenCalledWith("get_all_pr_statuses", {
          paths: ["/repo1"],
          includeMerged: true,
        });
        store.stopPolling();
      });
    });

    it("includes all repo paths in batched poll", async () => {
      mockGetPaths.mockReturnValue(["/repo1", "/repo2"]);
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [], "/repo2": [] });
        return Promise.resolve(null);
      });

      // Need fresh import with new mock
      vi.resetModules();
      vi.doMock("../../stores/repositories", () => ({
        repositoriesStore: {
          getPaths: mockGetPaths,
        },
      }));
      store = (await import("../../stores/github")).githubStore;

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        expect(mockInvoke).toHaveBeenCalledWith("get_all_pr_statuses", {
          paths: ["/repo1", "/repo2"],
          includeMerged: true,
        });
        store.stopPolling();
      });
    });

    it("uses includeMerged=true on startup poll and false on subsequent", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0); // startup poll

        // First call is startup: includeMerged = true
        const startupCall = mockInvoke.mock.calls.find((c: unknown[]) => c[0] === "get_all_pr_statuses");
        expect(startupCall?.[1]).toMatchObject({ includeMerged: true });

        mockInvoke.mockClear();

        // Advance past the 30s interval for a subsequent poll
        await vi.advanceTimersByTimeAsync(30_000);

        const subsequentCall = mockInvoke.mock.calls.find((c: unknown[]) => c[0] === "get_all_pr_statuses");
        expect(subsequentCall?.[1]).toMatchObject({ includeMerged: false });

        store.stopPolling();
      });
    });

    it("skips per-repo fallback and applies backoff when batch fails", async () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.reject(new Error("batch failed"));
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // Batch error triggers early return with backoff — no per-repo fallback
        expect(mockInvoke).not.toHaveBeenCalledWith("get_repo_pr_statuses", expect.anything());
        // Backoff warning should be logged
        expect(warnSpy).toHaveBeenCalledWith(
          "[github]",
          expect.stringContaining("Batch PR poll failed"),
          expect.anything(),
        );
        warnSpy.mockRestore();
        store.stopPolling();
      });
    });

    it("stopPolling prevents further polls", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        const callCount = mockInvoke.mock.calls.length;
        store.stopPolling();

        // Advance past next polling interval — should NOT trigger new calls
        await vi.advanceTimersByTimeAsync(60000);

        expect(mockInvoke.mock.calls.length).toBe(callCount);
      });
    });

    it("pauses polling when document becomes hidden", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [makePrStatus()] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        const callsAfterStart = mockInvoke.mock.calls.length;

        // Simulate tab becoming hidden
        Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        // Advance past multiple poll intervals — should NOT trigger new calls
        await vi.advanceTimersByTimeAsync(120000);
        expect(mockInvoke.mock.calls.length).toBe(callsAfterStart);

        store.stopPolling();
        Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });
      });
    });

    it("resumes polling with immediate poll when document becomes visible", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [makePrStatus()] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // Go hidden
        Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));
        await vi.advanceTimersByTimeAsync(0);

        const callsWhileHidden = mockInvoke.mock.calls.length;

        // Go visible again
        Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));
        await vi.advanceTimersByTimeAsync(0);

        // Should trigger an immediate poll on becoming visible
        expect(mockInvoke.mock.calls.length).toBeGreaterThan(callsWhileHidden);

        store.stopPolling();
      });
    });

    it("skips polling when no repos are configured", async () => {
      mockGetPaths.mockReturnValue([]);
      mockInvoke.mockResolvedValue(null);

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // Should not have called invoke since there are no repos
        expect(mockInvoke).not.toHaveBeenCalled();

        store.stopPolling();
      });
    });

    it("updates store state from successful batch poll response", async () => {
      const prStatus = makePrStatus({ branch: "main", state: "OPEN", number: 7 });
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [prStatus] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // State should reflect the polled data
        const data = store.getBranchPrData("/repo1", "main");
        expect(data).not.toBeNull();
        expect(data!.number).toBe(7);
        expect(data!.state).toBe("OPEN");

        store.stopPolling();
      });
    });

    it("applies exponential backoff when batch errors occur", async () => {
      // Batch errors trigger backoff: 30s, 60s, 120s, ...
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.reject(new Error("network error"));
        return Promise.resolve(null);
      });
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        store.startPolling();

        const batchCallCount = () =>
          mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "get_all_pr_statuses").length;

        await vi.advanceTimersByTimeAsync(0);
        const after0 = batchCallCount(); // 1st poll (immediate)

        // 1st failure → backoff = 30s (BASE_INTERVAL * 2^0)
        await vi.advanceTimersByTimeAsync(30_000);
        const after30 = batchCallCount(); // 2nd poll at t=30s

        // 2nd failure → backoff = 60s (BASE_INTERVAL * 2^1)
        // At t=60s the 3rd poll hasn't fired yet (next at t=90s)
        await vi.advanceTimersByTimeAsync(30_000);
        const after60 = batchCallCount();

        // Backoff means fewer polls in the second 30s window
        expect(after30 - after0).toBeGreaterThan(after60 - after30);

        warnSpy.mockRestore();
        store.stopPolling();
      });
    });

    it("persists PR state to localStorage after successful poll", async () => {
      const prStatus = makePrStatus({ branch: "feat/x", state: "OPEN" });
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [prStatus] });
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        const raw = localStorage.getItem("github:pr_state");
        expect(raw).not.toBeNull();
        const saved = JSON.parse(raw!);
        expect(saved["/repo1"]).toBeDefined();
        expect(saved["/repo1"].branches["feat/x"]).toBeDefined();

        store.stopPolling();
      });
    });

    it("loads persisted PR state on startPolling for offline transition detection", async () => {
      // Persist an OPEN PR before starting
      const persistedPr = makePrStatus({ branch: "feat/y", state: "OPEN", number: 99 });
      localStorage.setItem(
        "github:pr_state",
        JSON.stringify({ "/repo1": { branches: { "feat/y": persistedPr }, remoteStatus: null, lastPolled: 0 } }),
      );

      // New poll returns the same PR as MERGED → should emit 'merged' notification
      const mergedPr = makePrStatus({ branch: "feat/y", state: "MERGED", number: 99 });
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "check_github_circuit") return Promise.resolve(true);
        if (cmd === "get_all_pr_statuses") return Promise.resolve({ "/repo1": [mergedPr] });
        return Promise.resolve(null);
      });

      let notifStore: typeof import("../../stores/prNotifications").prNotificationsStore;
      notifStore = (await import("../../stores/prNotifications")).prNotificationsStore;
      notifStore.clearAll();

      await testInScopeAsync(async () => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // Should have detected the OPEN → MERGED transition
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("merged");
        expect(active[0].branch).toBe("feat/y");

        store.stopPolling();
      });
    });
  });

  describe("getRemoteOnlyPrs()", () => {
    it("returns open PRs whose branch is not in the provided local branches set", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [
          makePrStatus({ branch: "local-branch", state: "OPEN" }),
          makePrStatus({ branch: "remote-only-a", state: "OPEN", number: 1 }),
          makePrStatus({ branch: "remote-only-b", state: "OPEN", number: 2 }),
        ]);

        const result = store.getRemoteOnlyPrs("/repo1", new Set(["local-branch"]));

        expect(result).toHaveLength(2);
        expect(result.map((p) => p.branch)).toEqual(expect.arrayContaining(["remote-only-a", "remote-only-b"]));
      });
    });

    it("excludes merged and closed PRs", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [
          makePrStatus({ branch: "merged-remote", state: "MERGED", number: 1 }),
          makePrStatus({ branch: "closed-remote", state: "CLOSED", number: 2 }),
          makePrStatus({ branch: "open-remote", state: "OPEN", number: 3 }),
        ]);

        const result = store.getRemoteOnlyPrs("/repo1", new Set([]));

        expect(result).toHaveLength(1);
        expect(result[0].branch).toBe("open-remote");
      });
    });

    it("returns empty array when all PRs have local branches", () => {
      testInScope(() => {
        store.updateRepoData("/repo1", [
          makePrStatus({ branch: "branch-a", state: "OPEN" }),
          makePrStatus({ branch: "branch-b", state: "OPEN", number: 2 }),
        ]);

        const result = store.getRemoteOnlyPrs("/repo1", new Set(["branch-a", "branch-b"]));

        expect(result).toHaveLength(0);
      });
    });

    it("returns empty array for unknown repo", () => {
      testInScope(() => {
        const result = store.getRemoteOnlyPrs("/unknown-repo", new Set([]));
        expect(result).toHaveLength(0);
      });
    });
  });
});
