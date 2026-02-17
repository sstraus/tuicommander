import { describe, it, expect, vi, afterEach } from "vitest";
import { relativeTime } from "../../utils/time";

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
