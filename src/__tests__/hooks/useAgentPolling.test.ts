import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";

describe("useAgentPolling", () => {
  let store: typeof import("../../stores/terminals").terminalsStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockInvoke.mockReset();
    store = (await import("../../stores/terminals")).terminalsStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("polls the active terminal's foreground process", async () => {
    mockInvoke.mockResolvedValue("claude");

    await createRoot(async (dispose) => {
      const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      // The initial poll fires immediately
      await vi.advanceTimersByTimeAsync(0);
      await Promise.resolve(); // flush microtasks

      expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", {
        sessionId: "sess-1",
      });
      expect(store.get(id)?.agentType).toBe("claude");

      dispose();
    });
  });

  it("does not poll when no active terminal", async () => {
    await createRoot(async (dispose) => {
      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(3000);

      expect(mockInvoke).not.toHaveBeenCalledWith(
        "get_session_foreground_process",
        expect.anything(),
      );

      dispose();
    });
  });

  it("does not poll when active terminal has no session", async () => {
    await createRoot(async (dispose) => {
      const id = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(3000);

      expect(mockInvoke).not.toHaveBeenCalledWith(
        "get_session_foreground_process",
        expect.anything(),
      );

      dispose();
    });
  });

  it("sets agentType to null when result is null", async () => {
    mockInvoke.mockResolvedValue(null);

    await createRoot(async (dispose) => {
      const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(0);
      await Promise.resolve();

      expect(store.get(id)?.agentType).toBeNull();

      dispose();
    });
  });

  it("handles invoke errors gracefully", async () => {
    mockInvoke.mockRejectedValue(new Error("Session not found"));

    await createRoot(async (dispose) => {
      const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      // Should not throw
      await vi.advanceTimersByTimeAsync(0);
      await Promise.resolve();

      // agentType should remain null (default)
      expect(store.get(id)?.agentType).toBeNull();

      dispose();
    });
  });
});
