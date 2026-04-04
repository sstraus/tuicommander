import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";

/** Helper: advance timers and flush all pending microtasks */
async function tick(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
  await Promise.resolve();
  await Promise.resolve();
}

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

      // Poll fires on first interval tick (30s fallback), not immediately
      await vi.advanceTimersByTimeAsync(30_000);
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

      await vi.advanceTimersByTimeAsync(30_000);

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

      await vi.advanceTimersByTimeAsync(30_000);

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

      await vi.advanceTimersByTimeAsync(30_000);
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
      await vi.advanceTimersByTimeAsync(30_000);
      await Promise.resolve();

      // agentType should remain null (default)
      expect(store.get(id)?.agentType).toBeNull();

      dispose();
    });
  });

  describe("session discovery", () => {
    it("calls discover_agent_session when agentType transitions null→agent and agentSessionId is null", async () => {
      // First poll returns null, second returns "claude" (triggers discovery in same poll cycle)
      mockInvoke
        .mockResolvedValueOnce(null)            // get_session_foreground_process → null
        .mockResolvedValueOnce("claude")         // get_session_foreground_process → claude
        .mockResolvedValueOnce("found-uuid");    // discover_agent_session → uuid

      await createRoot(async (dispose) => {
        const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // first poll: null
        expect(store.get(id)?.agentType).toBeNull();

        await tick(30_000); // second poll: claude detected + discovery fires in same cycle
        expect(store.get(id)?.agentType).toBe("claude");
        expect(mockInvoke).toHaveBeenCalledWith("discover_agent_session", expect.objectContaining({
          agentType: "claude",
        }));
        expect(store.get(id)?.agentSessionId).toBe("found-uuid");

        dispose();
      });
    });

    it("does not retry discover_agent_session on subsequent polls", async () => {
      mockInvoke
        .mockResolvedValueOnce("claude")       // poll 1: claude
        .mockResolvedValueOnce("found-uuid")   // discover: uuid
        .mockResolvedValueOnce("claude")       // poll 2: still claude
        .mockResolvedValueOnce("claude");      // poll 3: still claude

      await createRoot(async (dispose) => {
        store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // poll 1 + discovery
        await tick(30_000); // poll 2 (no discovery)
        await tick(30_000); // poll 3 (no discovery)

        const discoveryCalls = mockInvoke.mock.calls.filter(
          ([cmd]) => cmd === "discover_agent_session",
        );
        expect(discoveryCalls).toHaveLength(1);

        dispose();
      });
    });

    it("skips discovery for agents without sessionDiscovery config (e.g. aider)", async () => {
      mockInvoke.mockResolvedValue("aider");

      await createRoot(async (dispose) => {
        const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000);
        expect(store.get(id)?.agentType).toBe("aider");

        const discoveryCalls = mockInvoke.mock.calls.filter(
          ([cmd]) => cmd === "discover_agent_session",
        );
        expect(discoveryCalls).toHaveLength(0);

        dispose();
      });
    });

    it("clears agentSessionId on agent→null transition and allows re-discovery", async () => {
      // NULL_THRESHOLD is 3: need 3 consecutive null polls before clearing
      mockInvoke
        .mockResolvedValueOnce("claude")       // poll 1: claude detected
        .mockResolvedValueOnce("uuid-1")       // discover: uuid-1
        .mockResolvedValueOnce("claude")       // poll 2: still claude
        .mockResolvedValueOnce(null)           // poll 3: null streak 1
        .mockResolvedValueOnce(null)           // poll 4: null streak 2
        .mockResolvedValueOnce(null)           // poll 5: null streak 3 → cleared
        .mockResolvedValueOnce("claude")       // poll 6: claude re-launched
        .mockResolvedValueOnce("uuid-2");      // re-discover: uuid-2

      await createRoot(async (dispose) => {
        const id = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // poll 1: claude + discovery queued
        await tick(30_000); // poll 2: still claude (discovery already done)
        expect(store.get(id)?.agentSessionId).toBe("uuid-1");

        await tick(30_000); // poll 3: null streak 1 — still holding agentType
        await tick(30_000); // poll 4: null streak 2 — still holding
        await tick(30_000); // poll 5: null streak 3 → agentType cleared
        expect(store.get(id)?.agentType).toBeNull();
        expect(store.get(id)?.agentSessionId).toBeNull();

        await tick(30_000); // poll 6: re-launched → re-discovery
        expect(store.get(id)?.agentType).toBe("claude");
        expect(store.get(id)?.agentSessionId).toBe("uuid-2");

        dispose();
      });
    });

    it("passes claimed_ids from other terminals to avoid duplicate assignment", async () => {
      // Two terminals, both running claude — processed sequentially
      mockInvoke
        .mockResolvedValueOnce("claude")     // term-1: get_session_foreground_process
        .mockResolvedValueOnce("uuid-a")     // term-1: discover_agent_session
        .mockResolvedValueOnce("claude")     // term-2: get_session_foreground_process
        .mockResolvedValueOnce("uuid-b");    // term-2: discover_agent_session

      await createRoot(async (dispose) => {
        const id1 = store.add({ sessionId: "sess-1", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: "sess-2", fontSize: 14, name: "T2", cwd: null, awaitingInput: null });

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // both polled sequentially + both discover

        const discoveryCalls = mockInvoke.mock.calls.filter(
          ([cmd]) => cmd === "discover_agent_session",
        );
        expect(discoveryCalls).toHaveLength(2);

        // Second discovery call must include the first terminal's claimed UUID
        const secondArgs = discoveryCalls[1];
        expect(secondArgs[1]).toHaveProperty("claimedIds");
        expect(secondArgs[1].claimedIds).toContain("uuid-a");

        expect(store.get(id1)?.agentSessionId).toBe("uuid-a");
        expect(store.get(id2)?.agentSessionId).toBe("uuid-b");

        dispose();
      });
    });
  });
});
