import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useGitHub } from "../../hooks/useGitHub";

describe("useGitHub", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("initial state", () => {
    it("has null status, false loading, and null error", () => {
      createRoot((dispose) => {
        // Use undefined path to prevent onMount from calling invoke
        const { status, loading, error } = useGitHub(() => undefined);
        expect(status()).toBeNull();
        expect(loading()).toBe(false);
        expect(error()).toBeNull();
        dispose();
      });
    });
  });

  describe("refresh()", () => {
    it("calls invoke and sets status on success", async () => {
      await createRoot(async (dispose) => {
        const ghStatus = {
          pr_count: 3,
          open_issues: 5,
          behind: 2,
          ahead: 1,
        };
        mockInvoke.mockResolvedValue(ghStatus);

        const { refresh, status, stopPolling } = useGitHub(() => "/repos/my-repo");

        // Let onMount fire, then immediately stop polling to avoid infinite timer loop
        await vi.advanceTimersByTimeAsync(0);
        stopPolling();

        // Explicit refresh
        await refresh();

        expect(status()).toEqual(ghStatus);
        expect(mockInvoke).toHaveBeenCalledWith("get_github_status", { path: "/repos/my-repo" });

        dispose();
      });
    });

    it("sets error and null status on failure", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockRejectedValue(new Error("network error"));

        const { refresh, status, error, stopPolling } = useGitHub(() => "/repos/my-repo");

        // Let onMount fire, then stop polling
        await vi.advanceTimersByTimeAsync(0);
        stopPolling();

        // Explicit refresh
        await refresh();

        expect(status()).toBeNull();
        expect(error()).toBe("Error: network error");

        dispose();
      });
    });

    it("sets null status when repo path is undefined", async () => {
      await createRoot(async (dispose) => {
        const { refresh, status, stopPolling } = useGitHub(() => undefined);

        // Let onMount fire, then stop polling
        await vi.advanceTimersByTimeAsync(0);
        stopPolling();

        await refresh();

        expect(status()).toBeNull();
        // invoke should not have been called for undefined path
        expect(mockInvoke).not.toHaveBeenCalledWith("get_github_status", expect.anything());

        dispose();
      });
    });
  });

  describe("startPolling / stopPolling", () => {
    it("polls at intervals and can be stopped", async () => {
      await createRoot(async (dispose) => {
        const ghStatus = { pr_count: 1, open_issues: 0, behind: 0, ahead: 0 };
        mockInvoke.mockResolvedValue(ghStatus);

        const { stopPolling } = useGitHub(() => "/repos/my-repo");

        // Let onMount fire (initial refresh)
        await vi.advanceTimersByTimeAsync(0);
        const callCountAfterInit = mockInvoke.mock.calls.length;
        expect(callCountAfterInit).toBeGreaterThan(0);

        // Advance by one poll interval (30s) - should trigger another refresh
        await vi.advanceTimersByTimeAsync(30000);
        expect(mockInvoke.mock.calls.length).toBeGreaterThan(callCountAfterInit);

        // Stop polling
        stopPolling();
        const callCountAfterStop = mockInvoke.mock.calls.length;

        // Advance another interval - no new calls
        await vi.advanceTimersByTimeAsync(30000);
        expect(mockInvoke.mock.calls.length).toBe(callCountAfterStop);

        dispose();
      });
    });
  });

  describe("visibility change handling", () => {
    it("slows down polling when tab becomes hidden", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValue({ pr_count: 0 });

        useGitHub(() => "/repos/my-repo");

        // Let onMount fire and flush the initial 200ms debounce
        await vi.advanceTimersByTimeAsync(200);

        // Simulate tab becoming hidden
        Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        // Should now poll at 120s (HIDDEN_INTERVAL)
        const callsBefore = mockInvoke.mock.calls.length;
        await vi.advanceTimersByTimeAsync(30000);
        // At 30s interval, no new call because hidden interval is 120s
        expect(mockInvoke.mock.calls.length).toBe(callsBefore);

        // Restore
        Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });

        dispose();
      });
    });

    it("speeds up and refreshes immediately when tab becomes visible", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValue({ pr_count: 0 });

        const { stopPolling } = useGitHub(() => "/repos/my-repo");

        // Let onMount fire
        await vi.advanceTimersByTimeAsync(0);

        // Simulate hidden then visible
        Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        const callsBefore = mockInvoke.mock.calls.length;

        Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        // Should trigger immediate refresh
        await vi.advanceTimersByTimeAsync(0);
        expect(mockInvoke.mock.calls.length).toBeGreaterThan(callsBefore);

        stopPolling();
        dispose();
      });
    });

    it("uses backoff interval when visible with previous errors", async () => {
      await createRoot(async (dispose) => {
        // First call succeeds (from onMount), subsequent calls fail
        mockInvoke
          .mockResolvedValueOnce({ pr_count: 0 })
          .mockRejectedValue(new Error("network error"));
        const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

        const { stopPolling } = useGitHub(() => "/repos/my-repo");

        // Let onMount fire (succeeds)
        await vi.advanceTimersByTimeAsync(0);

        // Trigger an error by advancing to next poll
        await vi.advanceTimersByTimeAsync(30000);

        // Now simulate visibility change: hidden then visible
        Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });
        document.dispatchEvent(new Event("visibilitychange"));

        // The interval should now be backoff (60s for 1 error)
        await vi.advanceTimersByTimeAsync(0);

        stopPolling();
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("cleanup on dispose", () => {
    it("stops polling when disposed", async () => {
      let callCountAfterDispose = 0;

      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValue({ pr_count: 0 });

        useGitHub(() => "/repos/my-repo");

        // Let onMount fire
        await vi.advanceTimersByTimeAsync(0);

        dispose();
        callCountAfterDispose = mockInvoke.mock.calls.length;
      });

      // Advance timers after dispose - no new calls
      await vi.advanceTimersByTimeAsync(60000);
      expect(mockInvoke.mock.calls.length).toBe(callCountAfterDispose);
    });
  });
});
