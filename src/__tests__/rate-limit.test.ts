import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  isStillRateLimited,
  getRemainingWaitTime,
  formatWaitTime,
  type RateLimitInfo,
} from "../rate-limit";

function makeInfo(overrides: Partial<RateLimitInfo> = {}): RateLimitInfo {
  return {
    agentType: "claude",
    sessionId: "sess-1",
    retryAfterMs: 10000,
    message: "rate limited",
    detectedAt: Date.now(),
    ...overrides,
  };
}

describe("isStillRateLimited", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns false when retryAfterMs is null", () => {
    const info = makeInfo({ retryAfterMs: null });
    expect(isStillRateLimited(info)).toBe(false);
  });

  it("returns true when within the rate limit window", () => {
    const info = makeInfo({ retryAfterMs: 10000, detectedAt: Date.now() });
    expect(isStillRateLimited(info)).toBe(true);
  });

  it("returns false when rate limit window has passed", () => {
    const info = makeInfo({ retryAfterMs: 5000, detectedAt: Date.now() });
    vi.advanceTimersByTime(6000);
    expect(isStillRateLimited(info)).toBe(false);
  });

  it("returns true at boundary (exactly at limit)", () => {
    const now = Date.now();
    const info = makeInfo({ retryAfterMs: 5000, detectedAt: now });
    // elapsed = 4999, still within
    vi.advanceTimersByTime(4999);
    expect(isStillRateLimited(info)).toBe(true);
  });
});

describe("getRemainingWaitTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns 0 when retryAfterMs is null", () => {
    const info = makeInfo({ retryAfterMs: null });
    expect(getRemainingWaitTime(info)).toBe(0);
  });

  it("returns full wait time when just detected", () => {
    const info = makeInfo({ retryAfterMs: 10000, detectedAt: Date.now() });
    expect(getRemainingWaitTime(info)).toBe(10000);
  });

  it("returns remaining time after partial wait", () => {
    const info = makeInfo({ retryAfterMs: 10000, detectedAt: Date.now() });
    vi.advanceTimersByTime(3000);
    expect(getRemainingWaitTime(info)).toBe(7000);
  });

  it("returns 0 when wait time has fully elapsed", () => {
    const info = makeInfo({ retryAfterMs: 5000, detectedAt: Date.now() });
    vi.advanceTimersByTime(10000);
    expect(getRemainingWaitTime(info)).toBe(0);
  });
});

describe("formatWaitTime", () => {
  it("returns 'now' for 0 or negative", () => {
    expect(formatWaitTime(0)).toBe("now");
    expect(formatWaitTime(-100)).toBe("now");
  });

  it("returns '< 1s' for sub-second", () => {
    expect(formatWaitTime(500)).toBe("< 1s");
    expect(formatWaitTime(999)).toBe("< 1s");
  });

  it("formats seconds", () => {
    expect(formatWaitTime(1000)).toBe("1s");
    expect(formatWaitTime(30000)).toBe("30s");
    expect(formatWaitTime(59000)).toBe("59s");
  });

  it("formats minutes", () => {
    expect(formatWaitTime(60000)).toBe("1m");
    expect(formatWaitTime(90000)).toBe("1m 30s");
  });

  it("formats hours", () => {
    expect(formatWaitTime(3600000)).toBe("1h");
    expect(formatWaitTime(5400000)).toBe("1h 30m");
  });

  it("rounds up sub-second remainders to next second", () => {
    // 1500ms = ceil(1.5) = 2s
    expect(formatWaitTime(1500)).toBe("2s");
  });
});
