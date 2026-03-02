import { describe, it, expect, vi, afterEach } from "vitest";
import { relativeTime, formatRelativeTime } from "../../utils/time";

describe("relativeTime", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns empty string for empty input", () => {
    expect(relativeTime("")).toBe("");
  });

  it("returns 'just now' for very recent timestamps", () => {
    const now = new Date().toISOString();
    expect(relativeTime(now)).toBe("just now");
  });

  it("returns minutes ago", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T10:30:00Z"));
    expect(relativeTime("2026-01-15T10:25:00Z")).toBe("5m ago");
    vi.useRealTimers();
  });

  it("returns hours ago", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T13:00:00Z"));
    expect(relativeTime("2026-01-15T10:00:00Z")).toBe("3h ago");
    vi.useRealTimers();
  });

  it("returns days ago", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-18T10:00:00Z"));
    expect(relativeTime("2026-01-15T10:00:00Z")).toBe("3d ago");
    vi.useRealTimers();
  });

  it("returns weeks ago for 7+ days", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-29T10:00:00Z"));
    expect(relativeTime("2026-01-15T10:00:00Z")).toBe("2w ago");
    vi.useRealTimers();
  });

  it("returns months ago for 30+ days", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-04-15T10:00:00Z"));
    expect(relativeTime("2026-01-15T10:00:00Z")).toBe("3mo ago");
    vi.useRealTimers();
  });
});

describe("formatRelativeTime", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns 'never' for null or undefined", () => {
    expect(formatRelativeTime(null)).toBe("never");
    expect(formatRelativeTime(undefined)).toBe("never");
  });

  it("returns 'just now' for a millisecond timestamp within 5 seconds", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T10:00:00Z"));
    const nowMs = Date.now();
    expect(formatRelativeTime(nowMs)).toBe("just now");
  });

  it("returns seconds ago for timestamps less than 60 seconds old", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T10:00:30Z"));
    const thirtySecondsAgoMs = new Date("2026-01-15T10:00:00Z").getTime();
    expect(formatRelativeTime(thirtySecondsAgoMs)).toBe("30s ago");
  });

  it("returns minutes ago for timestamps less than 60 minutes old", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T10:05:00Z"));
    const fiveMinutesAgoMs = new Date("2026-01-15T10:00:00Z").getTime();
    expect(formatRelativeTime(fiveMinutesAgoMs)).toBe("5m ago");
  });

  it("returns hours ago for timestamps less than 24 hours old", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T13:00:00Z"));
    const threeHoursAgoMs = new Date("2026-01-15T10:00:00Z").getTime();
    expect(formatRelativeTime(threeHoursAgoMs)).toBe("3h ago");
  });

  it("returns days ago for timestamps older than 24 hours", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-18T10:00:00Z"));
    const threeDaysAgoMs = new Date("2026-01-15T10:00:00Z").getTime();
    expect(formatRelativeTime(threeDaysAgoMs)).toBe("3d ago");
  });

  // Unit mismatch regression: Rust get_last_commit_timestamps() returns Unix seconds (%ct),
  // but formatRelativeTime() expects milliseconds. Without the *1000 conversion in
  // useGitOperations.ts, a seconds-based timestamp like 1700000000 would be treated
  // as 1700000000 ms = year 1970 + ~20 days, producing "~20000d ago" instead of a
  // reasonable relative time.
  it("treats input as milliseconds — a Unix-seconds value without conversion shows absurd output", () => {
    vi.useFakeTimers();
    // Freeze time at 2023-11-14T22:13:20Z (= Unix 1700000000)
    vi.setSystemTime(new Date(1700000000 * 1000));

    // Correct usage: pass milliseconds (what useGitOperations.ts does after *1000)
    const unixSecondsMs = 1700000000 * 1000;
    expect(formatRelativeTime(unixSecondsMs)).toBe("just now");

    // Bug scenario: pass raw Unix seconds without conversion
    // 1700000000 ms = ~Jan 20, 1970 → ~19,000+ days ago from 2023
    const rawUnixSeconds = 1700000000;
    const result = formatRelativeTime(rawUnixSeconds);
    // The result should show thousands of days ago, confirming seconds ≠ milliseconds
    expect(result).toMatch(/^\d+d ago$/);
    const days = parseInt(result);
    expect(days).toBeGreaterThan(10000);
  });
});
