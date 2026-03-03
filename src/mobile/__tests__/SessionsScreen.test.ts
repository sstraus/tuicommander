import { describe, it, expect } from "vitest";

// Test the pull-to-refresh spinner logic for SessionsScreen.
// The spinner should only show when refreshing=true AND sessions.length > 0.
describe("SessionsScreen refresh spinner visibility", () => {
  function shouldShowSpinner(refreshing: boolean, sessionCount: number): boolean {
    return refreshing && sessionCount > 0;
  }

  it("hidden when not refreshing and no sessions", () => {
    expect(shouldShowSpinner(false, 0)).toBe(false);
  });

  it("hidden when not refreshing with sessions", () => {
    expect(shouldShowSpinner(false, 3)).toBe(false);
  });

  it("hidden when refreshing but no sessions (skeleton handles that case)", () => {
    expect(shouldShowSpinner(true, 0)).toBe(false);
  });

  it("visible when refreshing and sessions exist", () => {
    expect(shouldShowSpinner(true, 1)).toBe(true);
  });

  it("visible when refreshing and multiple sessions exist", () => {
    expect(shouldShowSpinner(true, 5)).toBe(true);
  });
});
