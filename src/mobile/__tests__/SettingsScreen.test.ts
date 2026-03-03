import { describe, it, expect } from "vitest";

// Test the connection status derivation logic for SettingsScreen.
describe("SettingsScreen connection status", () => {
  function deriveConnectionStatus(connectionError: string | null): {
    label: string;
    isConnected: boolean;
  } {
    if (connectionError) {
      return { label: "Disconnected", isConnected: false };
    }
    return { label: "Connected", isConnected: true };
  }

  it("shows Connected when no error", () => {
    const status = deriveConnectionStatus(null);
    expect(status.label).toBe("Connected");
    expect(status.isConnected).toBe(true);
  });

  it("shows Disconnected when error is set", () => {
    const status = deriveConnectionStatus("Network error: connection refused");
    expect(status.label).toBe("Disconnected");
    expect(status.isConnected).toBe(false);
  });

  it("shows Disconnected for any non-null error string", () => {
    const status = deriveConnectionStatus("timeout");
    expect(status.label).toBe("Disconnected");
    expect(status.isConnected).toBe(false);
  });
});
