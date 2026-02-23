import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import type { BranchPrStatus } from "../../types";

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
      created_at: "2026-01-15T10:00:00Z",
      updated_at: "2026-01-15T12:00:00Z",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: null,
      ...overrides,
    };
  }

  describe("initialization", () => {
    it("starts with empty repos state", () => {
      createRoot((dispose) => {
        expect(store.state.repos).toEqual({});
        dispose();
      });
    });
  });

  describe("updateRepoData()", () => {
    it("sets branch PR data for a repo", () => {
      createRoot((dispose) => {
        const prData = [makePrStatus()];
        store.updateRepoData("/repo1", prData);

        expect(store.state.repos["/repo1"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
        expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
        dispose();
      });
    });

    it("handles multiple branches in one repo", () => {
      createRoot((dispose) => {
        const prData = [
          makePrStatus({ branch: "feature/x", number: 42 }),
          makePrStatus({ branch: "fix/y", number: 43 }),
        ];
        store.updateRepoData("/repo1", prData);

        expect(Object.keys(store.state.repos["/repo1"].branches)).toHaveLength(2);
        expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
        expect(store.state.repos["/repo1"].branches["fix/y"].number).toBe(43);
        dispose();
      });
    });

    it("removes stale branches no longer in poll results", () => {
      createRoot((dispose) => {
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
        dispose();
      });
    });

    it("updates lastPolled timestamp", () => {
      createRoot((dispose) => {
        const before = Date.now();
        store.updateRepoData("/repo1", [makePrStatus()]);
        const after = Date.now();

        expect(store.state.repos["/repo1"].lastPolled).toBeGreaterThanOrEqual(before);
        expect(store.state.repos["/repo1"].lastPolled).toBeLessThanOrEqual(after);
        dispose();
      });
    });
  });

  describe("getCheckSummary()", () => {
    it("returns check summary for known branch", () => {
      createRoot((dispose) => {
        store.updateRepoData("/repo1", [
          makePrStatus({ checks: { passed: 3, failed: 1, pending: 2, total: 6 } }),
        ]);

        const summary = store.getCheckSummary("/repo1", "feature/x");
        expect(summary).toEqual({ passed: 3, failed: 1, pending: 2, total: 6 });
        dispose();
      });
    });

    it("returns null for unknown repo", () => {
      createRoot((dispose) => {
        expect(store.getCheckSummary("/unknown", "feature/x")).toBeNull();
        dispose();
      });
    });

    it("returns null for unknown branch", () => {
      createRoot((dispose) => {
        store.updateRepoData("/repo1", [makePrStatus()]);
        expect(store.getCheckSummary("/repo1", "nonexistent")).toBeNull();
        dispose();
      });
    });
  });

  describe("getPrStatus()", () => {
    it("returns PR status for known branch", () => {
      createRoot((dispose) => {
        store.updateRepoData("/repo1", [makePrStatus()]);

        const pr = store.getPrStatus("/repo1", "feature/x");
        expect(pr).not.toBeNull();
        expect(pr!.number).toBe(42);
        expect(pr!.title).toBe("Add feature");
        expect(pr!.state).toBe("OPEN");
        expect(pr!.url).toBe("https://github.com/org/repo/pull/42");
        dispose();
      });
    });

    it("returns null for unknown branches", () => {
      createRoot((dispose) => {
        expect(store.getPrStatus("/unknown", "feature/x")).toBeNull();
        dispose();
      });
    });
  });

  describe("getCheckDetails()", () => {
    it("returns check details for known branch", () => {
      createRoot((dispose) => {
        store.updateRepoData("/repo1", [makePrStatus()]);

        const details = store.getCheckDetails("/repo1", "feature/x");
        expect(details).toHaveLength(2);
        expect(details[0].context).toBe("build");
        expect(details[1].context).toBe("test");
        dispose();
      });
    });

    it("returns empty array for unknown branch", () => {
      createRoot((dispose) => {
        expect(store.getCheckDetails("/unknown", "feature/x")).toEqual([]);
        dispose();
      });
    });
  });

  describe("getBranchPrData()", () => {
    it("returns full BranchPrStatus for known branch", () => {
      createRoot((dispose) => {
        const pr = makePrStatus();
        store.updateRepoData("/repo1", [pr]);

        const data = store.getBranchPrData("/repo1", "feature/x");
        expect(data).not.toBeNull();
        expect(data!.author).toBe("alice");
        expect(data!.commits).toBe(3);
        expect(data!.additions).toBe(10);
        expect(data!.deletions).toBe(5);
        dispose();
      });
    });

    it("returns null for unknown branch", () => {
      createRoot((dispose) => {
        expect(store.getBranchPrData("/repo1", "no-branch")).toBeNull();
        dispose();
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
      createRoot((dispose) => {
        transition({ state: "OPEN" }, { state: "MERGED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("merged");
        expect(active[0].prNumber).toBe(42);
        dispose();
      });
    });

    it("emits 'merged' when CLOSED → MERGED", () => {
      createRoot((dispose) => {
        transition({ state: "CLOSED" }, { state: "MERGED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("merged");
        dispose();
      });
    });

    it("emits 'closed' when OPEN → CLOSED", () => {
      createRoot((dispose) => {
        transition({ state: "OPEN" }, { state: "CLOSED" });
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("closed");
        dispose();
      });
    });

    it("emits 'blocked' when mergeable becomes CONFLICTING on open PR", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", mergeable: "MERGEABLE" },
          { state: "OPEN", mergeable: "CONFLICTING" },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("blocked");
        dispose();
      });
    });

    it("emits 'ci_failed' when failed checks go from 0 to >0 on open PR", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", checks: { passed: 2, failed: 0, pending: 0, total: 2 } },
          { state: "OPEN", checks: { passed: 1, failed: 1, pending: 0, total: 2 } },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("ci_failed");
        dispose();
      });
    });

    it("emits 'changes_requested' when review_decision becomes CHANGES_REQUESTED", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", review_decision: "" },
          { state: "OPEN", review_decision: "CHANGES_REQUESTED" },
        );
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("changes_requested");
        dispose();
      });
    });

    it("emits 'ready' when PR becomes mergeable+approved+no-failures", () => {
      createRoot((dispose) => {
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
        dispose();
      });
    });

    it("emits 'ready' when PR goes from conflicting to mergeable+approved", () => {
      createRoot((dispose) => {
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
        dispose();
      });
    });

    it("emits 'ready' when CI failures are resolved and PR is otherwise ready", () => {
      createRoot((dispose) => {
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
        dispose();
      });
    });

    it("does not emit when state is unchanged", () => {
      createRoot((dispose) => {
        transition({ state: "OPEN" }, { state: "OPEN" });
        expect(notifStore.getActive()).toHaveLength(0);
        dispose();
      });
    });

    it("does not emit on first update (no prior data)", () => {
      createRoot((dispose) => {
        // Only one updateRepoData call — no prior data, no transitions
        store.updateRepoData("/repo1", [makePrStatus({ state: "MERGED" })]);
        expect(notifStore.getActive()).toHaveLength(0);
        dispose();
      });
    });

    it("does not emit 'blocked' when already CONFLICTING", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", mergeable: "CONFLICTING" },
          { state: "OPEN", mergeable: "CONFLICTING" },
        );
        expect(notifStore.getActive()).toHaveLength(0);
        dispose();
      });
    });

    it("does not emit 'ci_failed' when already had failures", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", checks: { passed: 1, failed: 1, pending: 0, total: 2 } },
          { state: "OPEN", checks: { passed: 0, failed: 2, pending: 0, total: 2 } },
        );
        expect(notifStore.getActive()).toHaveLength(0);
        dispose();
      });
    });

    it("does not emit 'blocked' or other OPEN transitions for closed PRs", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", mergeable: "MERGEABLE" },
          { state: "CLOSED", mergeable: "CONFLICTING" },
        );
        // 'closed' is emitted, but NOT 'blocked' (since PR is not OPEN in new state)
        const active = notifStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].type).toBe("closed");
        dispose();
      });
    });

    it("includes correct branch and title in notification", () => {
      createRoot((dispose) => {
        transition(
          { state: "OPEN", branch: "my/feature", title: "My Feature PR" },
          { state: "MERGED", branch: "my/feature", title: "My Feature PR" },
        );
        const active = notifStore.getActive();
        expect(active[0].branch).toBe("my/feature");
        expect(active[0].title).toBe("My Feature PR");
        expect(active[0].repoPath).toBe("/repo1");
        dispose();
      });
    });
  });

  describe("polling", () => {
    it("polls repos on startPolling", async () => {
      mockInvoke.mockResolvedValue([makePrStatus()]);

      await createRoot(async (dispose) => {
        store.startPolling();

        // Flush the initial poll microtask
        await vi.advanceTimersByTimeAsync(0);

        expect(mockInvoke).toHaveBeenCalledWith("get_repo_pr_statuses", { path: "/repo1" });
        store.stopPolling();
        dispose();
      });
    });

    it("polls multiple repos", async () => {
      mockGetPaths.mockReturnValue(["/repo1", "/repo2"]);
      mockInvoke.mockResolvedValue([]);

      // Need fresh import with new mock
      vi.resetModules();
      vi.doMock("../../stores/repositories", () => ({
        repositoriesStore: {
          getPaths: mockGetPaths,
        },
      }));
      store = (await import("../../stores/github")).githubStore;

      await createRoot(async (dispose) => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        expect(mockInvoke).toHaveBeenCalledWith("get_repo_pr_statuses", { path: "/repo1" });
        expect(mockInvoke).toHaveBeenCalledWith("get_repo_pr_statuses", { path: "/repo2" });
        store.stopPolling();
        dispose();
      });
    });

    it("stopPolling prevents further polls", async () => {
      mockInvoke.mockResolvedValue([]);

      await createRoot(async (dispose) => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        const callCount = mockInvoke.mock.calls.length;
        store.stopPolling();

        // Advance past next polling interval — should NOT trigger new calls
        await vi.advanceTimersByTimeAsync(60000);

        expect(mockInvoke.mock.calls.length).toBe(callCount);
        dispose();
      });
    });

    it("pauses polling when document becomes hidden", async () => {
      mockInvoke.mockResolvedValue([makePrStatus()]);

      await createRoot(async (dispose) => {
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
        dispose();
      });
    });

    it("resumes polling with immediate poll when document becomes visible", async () => {
      mockInvoke.mockResolvedValue([makePrStatus()]);

      await createRoot(async (dispose) => {
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
        dispose();
      });
    });

    it("handles per-repo poll failure gracefully", async () => {
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      mockInvoke.mockRejectedValueOnce(new Error("network error"));

      await createRoot(async (dispose) => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining("Failed to poll PR statuses"),
          expect.any(Error),
        );
        consoleSpy.mockRestore();
        store.stopPolling();
        dispose();
      });
    });

    it("skips polling when no repos are configured", async () => {
      mockGetPaths.mockReturnValue([]);
      mockInvoke.mockResolvedValue([]);

      await createRoot(async (dispose) => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // Should not have called invoke since there are no repos
        expect(mockInvoke).not.toHaveBeenCalled();

        store.stopPolling();
        dispose();
      });
    });

    it("updates store state from successful poll response", async () => {
      const prStatus = makePrStatus({ branch: "main", state: "OPEN", number: 7 });
      mockInvoke.mockResolvedValue([prStatus]);

      await createRoot(async (dispose) => {
        store.startPolling();
        await vi.advanceTimersByTimeAsync(0);

        // State should reflect the polled data
        const data = store.getBranchPrData("/repo1", "main");
        expect(data).not.toBeNull();
        expect(data!.number).toBe(7);
        expect(data!.state).toBe("OPEN");

        store.stopPolling();
        dispose();
      });
    });

    it("continues polling at base interval even when per-repo errors occur", async () => {
      // Per-repo errors are caught inside pollAll() — they do NOT trigger backoff.
      // The outer catch (backoff logic) is only reachable if Promise.all itself throws,
      // which can't happen when each path's error is caught individually.
      mockInvoke.mockRejectedValue(new Error("network error"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      // BASE = 30s — should stay at this interval regardless of per-repo errors
      await createRoot(async (dispose) => {
        store.startPolling();

        await vi.advanceTimersByTimeAsync(0);
        const after0 = mockInvoke.mock.calls.length; // 2 (initial poll: PR + remote)

        await vi.advanceTimersByTimeAsync(30_000);
        const after30 = mockInvoke.mock.calls.length; // 4 (one more poll at 30s)

        await vi.advanceTimersByTimeAsync(30_000);
        const after60 = mockInvoke.mock.calls.length; // 6 (one more poll at 60s)

        expect(after0).toBe(2);
        expect(after30).toBe(4);
        expect(after60).toBe(6);

        consoleSpy.mockRestore();
        store.stopPolling();
        dispose();
      });
    });
  });
});
