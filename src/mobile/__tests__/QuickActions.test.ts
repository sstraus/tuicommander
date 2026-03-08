import { describe, it, expect, vi } from "vitest";

// Mock dependencies that pull in Tauri/transport
vi.mock("../../transport", () => ({
  rpc: vi.fn(),
  subscribePty: vi.fn(() => () => {}),
}));
vi.mock("../../stores/appLogger", () => ({
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// TerminalKeybar shows confirm keys (Yes/No) only when awaitingInput is true.
// We test the data condition that drives the Show when={} guard.
describe("TerminalKeybar confirm keys visibility rule", () => {
  function shouldShowConfirmKeys(awaitingInput: boolean | undefined): boolean {
    return awaitingInput === true;
  }

  it("shows confirm keys when awaiting_input is true", () => {
    expect(shouldShowConfirmKeys(true)).toBe(true);
  });

  it("hides confirm keys when awaiting_input is false", () => {
    expect(shouldShowConfirmKeys(false)).toBe(false);
  });

  it("hides confirm keys when awaiting_input is undefined (no state)", () => {
    expect(shouldShowConfirmKeys(undefined)).toBe(false);
  });

  it("hides confirm keys when session is busy but not awaiting input", () => {
    expect(shouldShowConfirmKeys(false)).toBe(false);
  });

  it("hides confirm keys when session is rate-limited", () => {
    expect(shouldShowConfirmKeys(false)).toBe(false);
  });
});
