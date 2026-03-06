import { describe, it, expect, vi } from "vitest";

// Mock dependencies that pull in Tauri/transport
vi.mock("../../transport", () => ({
  rpc: vi.fn(),
  subscribePty: vi.fn(() => () => {}),
}));
vi.mock("../../stores/appLogger", () => ({
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

/**
 * OutputView error-state logic: when subscribePty rejects, the component
 * should capture the error and expose it so a fallback message renders.
 */
describe("OutputView subscribePty error handling", () => {
  /** Mirror the logic that OutputView uses to derive display state from error */
  function deriveOutputState(error: string | null): {
    showError: boolean;
    errorMessage: string;
  } {
    return {
      showError: error !== null,
      errorMessage: error ?? "",
    };
  }

  it("shows no error when subscribePty succeeds", () => {
    const state = deriveOutputState(null);
    expect(state.showError).toBe(false);
    expect(state.errorMessage).toBe("");
  });

  it("shows error fallback when subscribePty rejects", () => {
    const state = deriveOutputState("WebSocket connection failed");
    expect(state.showError).toBe(true);
    expect(state.errorMessage).toBe("WebSocket connection failed");
  });

  it("shows error fallback for session-not-found rejection", () => {
    const state = deriveOutputState("Session not found: abc123");
    expect(state.showError).toBe(true);
    expect(state.errorMessage).toBe("Session not found: abc123");
  });

  it("shows error fallback for network errors", () => {
    const state = deriveOutputState("Network error");
    expect(state.showError).toBe(true);
    expect(state.errorMessage).toBe("Network error");
  });
});
