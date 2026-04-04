import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { RateLimitInfo } from "../../rate-limit";
import { testInScope } from "../helpers/store";

describe("rateLimitStore", () => {
  let store: typeof import("../../stores/ratelimit").rateLimitStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    localStorage.clear();
    store = (await import("../../stores/ratelimit")).rateLimitStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeInfo(sessionId: string, retryAfterMs: number | null = 10000): RateLimitInfo {
    return {
      agentType: "claude",
      sessionId,
      retryAfterMs,
      message: "rate limited",
      detectedAt: Date.now(),
    };
  }

  describe("addRateLimit()", () => {
    it("adds rate limit info", () => {
      testInScope(() => {
        const info = makeInfo("sess-1");
        store.addRateLimit(info);
        expect(store.getRateLimitInfo("sess-1")).toBeDefined();
      });
    });
  });

  describe("removeRateLimit()", () => {
    it("removes rate limit info", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1"));
        store.removeRateLimit("sess-1");
        expect(store.getRateLimitInfo("sess-1")).toBeUndefined();
      });
    });
  });

  describe("isRateLimited()", () => {
    it("returns true when within rate limit window", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 10000));
        expect(store.isRateLimited("sess-1")).toBe(true);
      });
    });

    it("returns false after window expires", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 5000));
        vi.advanceTimersByTime(6000);
        expect(store.isRateLimited("sess-1")).toBe(false);
      });
    });

    it("returns false for unknown sessions", () => {
      testInScope(() => {
        expect(store.isRateLimited("unknown")).toBe(false);
      });
    });
  });

  describe("getWaitTime()", () => {
    it("returns remaining wait time", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 10000));
        vi.advanceTimersByTime(3000);
        expect(store.getWaitTime("sess-1")).toBe(7000);
      });
    });

    it("returns 0 for unknown sessions", () => {
      testInScope(() => {
        expect(store.getWaitTime("unknown")).toBe(0);
      });
    });
  });

  describe("getRateLimitedSessions()", () => {
    it("returns only active rate-limited sessions", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 10000));
        store.addRateLimit(makeInfo("sess-2", 1000));
        vi.advanceTimersByTime(2000);
        const sessions = store.getRateLimitedSessions();
        expect(sessions).toContain("sess-1");
        expect(sessions).not.toContain("sess-2");
      });
    });
  });

  describe("getRateLimitedCount()", () => {
    it("returns count of active rate limits", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 10000));
        store.addRateLimit(makeInfo("sess-2", 10000));
        expect(store.getRateLimitedCount()).toBe(2);
      });
    });
  });

  describe("cleanupExpired()", () => {
    it("removes expired rate limits", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1", 10000));
        store.addRateLimit(makeInfo("sess-2", 1000));
        vi.advanceTimersByTime(2000);
        store.cleanupExpired();
        expect(store.getRateLimitInfo("sess-1")).toBeDefined();
        expect(store.getRateLimitInfo("sess-2")).toBeUndefined();
      });
    });
  });

  describe("clearAll()", () => {
    it("clears all rate limits", () => {
      testInScope(() => {
        store.addRateLimit(makeInfo("sess-1"));
        store.addRateLimit(makeInfo("sess-2"));
        store.clearAll();
        expect(store.getRateLimitedCount()).toBe(0);
      });
    });
  });
});
