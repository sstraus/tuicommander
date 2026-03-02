import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

// Mocks must be declared before any module import
const mockGetPrStatus = vi.fn();
const mockLastActivityAt = vi.fn<() => number>(() => 0);

vi.mock("../../stores/github", () => ({
  githubStore: {
    getPrStatus: mockGetPrStatus,
  },
}));

vi.mock("../../stores/userActivity", () => ({
  userActivityStore: {
    lastActivityAt: mockLastActivityAt,
  },
}));

describe("activePrStatus / mergedPrGrace", () => {
  let activePrStatus: typeof import("../../utils/mergedPrGrace").activePrStatus;
  let _resetMergedActivityAccum: typeof import("../../utils/mergedPrGrace")._resetMergedActivityAccum;
  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockGetPrStatus.mockReset();
    mockLastActivityAt.mockReturnValue(0);

    vi.doMock("../../stores/github", () => ({
      githubStore: {
        getPrStatus: mockGetPrStatus,
      },
    }));

    vi.doMock("../../stores/userActivity", () => ({
      userActivityStore: {
        lastActivityAt: mockLastActivityAt,
      },
    }));

    const mod = await import("../../utils/mergedPrGrace");
    activePrStatus = mod.activePrStatus;
    _resetMergedActivityAccum = mod._resetMergedActivityAccum;
  });

  afterEach(() => {
    vi.useRealTimers();
    _resetMergedActivityAccum();
  });

  function makePr(state: string, number = 42) {
    return {
      branch: "feature/x",
      number,
      title: "Test PR",
      state,
      url: "https://github.com/org/repo/pull/42",
      additions: 10,
      deletions: 5,
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
      check_details: [],
      author: "alice",
      commits: 1,
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      review_decision: "",
      labels: [],
      is_draft: false,
      base_ref_name: "main",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
      merge_state_label: { label: "Ready", css_class: "clean" },
      review_state_label: null,
    };
  }

  it("returns null when no PR exists", () => {
    createRoot((dispose) => {
      mockGetPrStatus.mockReturnValue(null);
      expect(activePrStatus("/repo", "feature/x")).toBeNull();
      dispose();
    });
  });

  it("returns null for CLOSED PR", () => {
    createRoot((dispose) => {
      mockGetPrStatus.mockReturnValue(makePr("CLOSED"));
      expect(activePrStatus("/repo", "feature/x")).toBeNull();
      dispose();
    });
  });

  it("returns null for closed PR regardless of case", () => {
    createRoot((dispose) => {
      mockGetPrStatus.mockReturnValue(makePr("closed"));
      expect(activePrStatus("/repo", "feature/x")).toBeNull();
      dispose();
    });
  });

  it("returns OPEN PR as-is", () => {
    createRoot((dispose) => {
      const pr = makePr("OPEN");
      mockGetPrStatus.mockReturnValue(pr);
      const result = activePrStatus("/repo", "feature/x");
      expect(result).toBe(pr);
      dispose();
    });
  });

  it("returns MERGED PR while within grace period", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      // No activity, accumulator stays at 0 — well within grace period
      const result = activePrStatus("/repo", "feature/x");
      expect(result).toBe(pr);
      dispose();
    });
  });

  it("accumulates active time and hides MERGED PR after 5 minutes of activity", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      const now = Date.now();

      // First call at t=0: initializes entry with ms=0, lastCheck=now
      vi.setSystemTime(now);
      mockLastActivityAt.mockReturnValue(now - 500);
      activePrStatus("/repo", "feature/x");

      // Each subsequent call advances 5000ms and accumulates 5000ms of active time.
      // MERGED_GRACE_MS = 300000ms, so we need 60 ticks of 5000ms.
      // We drive 62 ticks to ensure we cross the threshold.
      for (let i = 1; i <= 62; i++) {
        const t = now + i * 5000;
        vi.setSystemTime(t);
        mockLastActivityAt.mockReturnValue(t - 500); // user active within 2s
        activePrStatus("/repo", "feature/x");
      }

      // After 62 * 5000ms = 310000ms > MERGED_GRACE_MS, the PR must be hidden
      expect(activePrStatus("/repo", "feature/x")).toBeNull();

      dispose();
    });
  });

  it("does not accumulate time when user is inactive", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      const now = Date.now();

      // First call initializes the entry
      vi.setSystemTime(now);
      mockLastActivityAt.mockReturnValue(0); // inactive
      activePrStatus("/repo", "feature/x");

      // Advance time significantly, but user remains inactive
      vi.setSystemTime(now + 10 * 60 * 1000); // 10 minutes later
      mockLastActivityAt.mockReturnValue(0); // still inactive
      const result = activePrStatus("/repo", "feature/x");

      // PR must still be visible because no active time was accumulated
      expect(result).toBe(pr);

      dispose();
    });
  });

  it("caps elapsed per tick at 60s to avoid large jumps", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      const now = Date.now();

      // Initialize the accumulator
      vi.setSystemTime(now);
      mockLastActivityAt.mockReturnValue(now - 500);
      activePrStatus("/repo", "feature/x");

      // Jump forward by 10 minutes in a single tick — should be capped at 60s
      const laterTime = now + 10 * 60 * 1000;
      vi.setSystemTime(laterTime);
      mockLastActivityAt.mockReturnValue(laterTime - 500);
      activePrStatus("/repo", "feature/x");

      // One tick max adds 60s, which is less than MERGED_GRACE_MS (5 min)
      // So the PR should still be visible
      expect(activePrStatus("/repo", "feature/x")).toBe(pr);

      dispose();
    });
  });

  it("stays hidden once the grace period is consumed", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      const now = Date.now();

      // Build up enough accumulated time to cross the threshold
      vi.setSystemTime(now);
      mockLastActivityAt.mockReturnValue(now - 500);
      activePrStatus("/repo", "feature/x");

      for (let i = 1; i <= 62; i++) {
        const t = now + i * 5000;
        vi.setSystemTime(t);
        mockLastActivityAt.mockReturnValue(t - 500);
        activePrStatus("/repo", "feature/x");
      }

      // Confirm it is now hidden
      expect(activePrStatus("/repo", "feature/x")).toBeNull();

      // Reset activity — PR should remain hidden (entry persists in the map)
      mockLastActivityAt.mockReturnValue(0);
      expect(activePrStatus("/repo", "feature/x")).toBeNull();

      dispose();
    });
  });

  it("_resetMergedActivityAccum clears all accumulated state", () => {
    createRoot((dispose) => {
      const pr = makePr("MERGED");
      mockGetPrStatus.mockReturnValue(pr);
      const now = Date.now();

      // Push past the grace period
      vi.setSystemTime(now);
      mockLastActivityAt.mockReturnValue(now - 500);
      activePrStatus("/repo", "feature/x");

      for (let i = 1; i <= 62; i++) {
        const t = now + i * 5000;
        vi.setSystemTime(t);
        mockLastActivityAt.mockReturnValue(t - 500);
        activePrStatus("/repo", "feature/x");
      }

      expect(activePrStatus("/repo", "feature/x")).toBeNull();

      // Reset and verify the PR is visible again (accumulator cleared)
      _resetMergedActivityAccum();
      mockLastActivityAt.mockReturnValue(0);
      expect(activePrStatus("/repo", "feature/x")).toBe(pr);

      dispose();
    });
  });
});
