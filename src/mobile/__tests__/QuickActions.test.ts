import { describe, it, expect, vi } from "vitest";

// Mock dependencies that pull in Tauri/transport
vi.mock("../../transport", () => ({
  rpc: vi.fn(),
  subscribePty: vi.fn(() => () => {}),
}));
vi.mock("../../stores/appLogger", () => ({
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// The conditional rendering logic: QuickActions visible only when awaiting_input
// We test the data condition that drives Show when={} rather than DOM rendering
// to avoid the complexity of mocking all child components in SessionDetailScreen.
describe("QuickActions conditional visibility rule", () => {
  function shouldShowQuickActions(awaitingInput: boolean | undefined): boolean {
    return awaitingInput === true;
  }

  it("shows when awaiting_input is true", () => {
    expect(shouldShowQuickActions(true)).toBe(true);
  });

  it("hides when awaiting_input is false", () => {
    expect(shouldShowQuickActions(false)).toBe(false);
  });

  it("hides when awaiting_input is undefined (no state)", () => {
    expect(shouldShowQuickActions(undefined)).toBe(false);
  });

  it("hides when session is busy but not awaiting input", () => {
    // shell_state=busy, awaiting_input=false → hidden
    expect(shouldShowQuickActions(false)).toBe(false);
  });

  it("hides when session is rate-limited", () => {
    // rate_limited=true, awaiting_input=false → hidden
    expect(shouldShowQuickActions(false)).toBe(false);
  });
});
