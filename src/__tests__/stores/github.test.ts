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
  });
});
