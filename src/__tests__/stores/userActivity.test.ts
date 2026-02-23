import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Import after module setup — SolidJS reactive primitives work in test context
import { userActivityStore } from "../../stores/userActivity";

describe("userActivityStore", () => {
  beforeEach(() => {
    userActivityStore.reset();
    vi.useFakeTimers();
  });

  afterEach(() => {
    userActivityStore.stopListening();
    vi.useRealTimers();
  });

  it("lastActivityAt is 0 initially", () => {
    expect(userActivityStore.lastActivityAt()).toBe(0);
  });

  it("recordActivity updates lastActivityAt to Date.now()", () => {
    vi.setSystemTime(new Date("2026-01-15T10:00:00Z"));
    userActivityStore.recordActivity();
    expect(userActivityStore.lastActivityAt()).toBe(new Date("2026-01-15T10:00:00Z").getTime());
  });

  it("msSinceLastActivity returns Infinity when no activity recorded", () => {
    expect(userActivityStore.msSinceLastActivity()).toBe(Infinity);
  });

  it("msSinceLastActivity returns elapsed time after activity", () => {
    vi.setSystemTime(1000);
    userActivityStore.recordActivity();
    vi.setSystemTime(3500);
    expect(userActivityStore.msSinceLastActivity()).toBe(2500);
  });

  it("startListening adds window event listeners for click and keydown", () => {
    const addSpy = vi.spyOn(window, "addEventListener");
    userActivityStore.startListening();
    const calls = addSpy.mock.calls.filter(
      ([type]) => type === "click" || type === "keydown",
    );
    expect(calls.length).toBe(2);
    // Both should be passive
    for (const [, , options] of calls) {
      expect(options).toEqual({ passive: true });
    }
    addSpy.mockRestore();
  });

  it("stopListening removes window event listeners", () => {
    const removeSpy = vi.spyOn(window, "removeEventListener");
    userActivityStore.startListening();
    userActivityStore.stopListening();
    const calls = removeSpy.mock.calls.filter(
      ([type]) => type === "click" || type === "keydown",
    );
    expect(calls.length).toBe(2);
    removeSpy.mockRestore();
  });

  it("click event triggers recordActivity", () => {
    vi.setSystemTime(5000);
    userActivityStore.startListening();
    window.dispatchEvent(new MouseEvent("click"));
    expect(userActivityStore.lastActivityAt()).toBe(5000);
  });

  it("keydown event triggers recordActivity", () => {
    vi.setSystemTime(7000);
    userActivityStore.startListening();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    expect(userActivityStore.lastActivityAt()).toBe(7000);
  });

  it("reset clears lastActivityAt to 0", () => {
    userActivityStore.recordActivity();
    expect(userActivityStore.lastActivityAt()).not.toBe(0);
    userActivityStore.reset();
    expect(userActivityStore.lastActivityAt()).toBe(0);
  });

  it("multiple recordActivity calls update to latest time", () => {
    vi.setSystemTime(1000);
    userActivityStore.recordActivity();
    vi.setSystemTime(2000);
    userActivityStore.recordActivity();
    expect(userActivityStore.lastActivityAt()).toBe(2000);
  });
});
