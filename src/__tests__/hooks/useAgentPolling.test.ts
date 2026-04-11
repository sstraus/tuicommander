import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { makeTerminal, testInScopeAsync } from "../helpers/store";

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

    await testInScopeAsync(async () => {
      const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
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

    });
  });

  it("does not poll when no active terminal", async () => {
    await testInScopeAsync(async () => {
      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(30_000);

      expect(mockInvoke).not.toHaveBeenCalledWith(
        "get_session_foreground_process",
        expect.anything(),
      );

    });
  });

  it("does not poll when active terminal has no session", async () => {
    await testInScopeAsync(async () => {
      const id = store.add(makeTerminal({ name: "T1" }));
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(30_000);

      expect(mockInvoke).not.toHaveBeenCalledWith(
        "get_session_foreground_process",
        expect.anything(),
      );

    });
  });

  it("sets agentType to null when result is null", async () => {
    mockInvoke.mockResolvedValue(null);

    await testInScopeAsync(async () => {
      const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      await vi.advanceTimersByTimeAsync(30_000);
      await Promise.resolve();

      expect(store.get(id)?.agentType).toBeNull();

    });
  });

  it("handles invoke errors gracefully", async () => {
    mockInvoke.mockRejectedValue(new Error("Session not found"));

    await testInScopeAsync(async () => {
      const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
      store.setActive(id);

      const { useAgentPolling } = await import("../../hooks/useAgentPolling");
      useAgentPolling();

      // Should not throw
      await vi.advanceTimersByTimeAsync(30_000);
      await Promise.resolve();

      // agentType should remain null (default)
      expect(store.get(id)?.agentType).toBeNull();

    });
  });

  describe("session discovery", () => {
    it("calls discover_agent_session when agentType transitions null→agent and agentSessionId is null", async () => {
      // First poll returns null, second returns "claude" (triggers discovery in same poll cycle)
      mockInvoke
        .mockResolvedValueOnce(null)            // get_session_foreground_process → null
        .mockResolvedValueOnce("claude")         // get_session_foreground_process → claude
        .mockResolvedValueOnce("found-uuid");    // discover_agent_session → uuid

      await testInScopeAsync(async () => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

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

      });
    });

    it("does not retry discover_agent_session on subsequent polls", async () => {
      mockInvoke
        .mockResolvedValueOnce("claude")       // poll 1: claude
        .mockResolvedValueOnce("found-uuid")   // discover: uuid
        .mockResolvedValueOnce("claude")       // poll 2: still claude
        .mockResolvedValueOnce("claude");      // poll 3: still claude

      await testInScopeAsync(async () => {
        store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // poll 1 + discovery
        await tick(30_000); // poll 2 (no discovery)
        await tick(30_000); // poll 3 (no discovery)

        const discoveryCalls = mockInvoke.mock.calls.filter(
          ([cmd]) => cmd === "discover_agent_session",
        );
        expect(discoveryCalls).toHaveLength(1);

      });
    });

    it("skips discovery for agents without sessionDiscovery config (e.g. aider)", async () => {
      mockInvoke.mockResolvedValue("aider");

      await testInScopeAsync(async () => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

        const { useAgentPolling } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000);
        expect(store.get(id)?.agentType).toBe("aider");

        const discoveryCalls = mockInvoke.mock.calls.filter(
          ([cmd]) => cmd === "discover_agent_session",
        );
        expect(discoveryCalls).toHaveLength(0);

      });
    });

    it("dispatches synthetic shell-state to plugins when agent first detected", async () => {
      // Bug: when agentType transitions null→"claude", structured shell-state events
      // dispatched BEFORE detection completes were filtered out (pluginMatchesSession
      // returned false because agentType was still null). The plugin never learned
      // the current shellState. Fix: after agent-started, replay the current shellState
      // so filtered plugins catch up.
      mockInvoke.mockResolvedValue("claude");

      await testInScopeAsync(async () => {
        const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
        const { pluginRegistry } = await import("../../plugins/pluginRegistry");

        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-synth" }));
        // Simulate: shell is idle, but agentType not yet detected
        store.update(id, { shellState: "idle" });

        // Register a plugin with agentTypes: ["claude"] that listens for shell-state
        const shellStateHandler = vi.fn();
        await pluginRegistry.register(
          { id: "test-keepalive", onload: (host) => {
            host.registerStructuredEventHandler("shell-state", shellStateHandler);
          }, onunload: () => {} },
          ["pty:write"],
          [],
          ["claude"],
        );

        // Before detection: dispatch shell-state directly → should be filtered (agentType null)
        pluginRegistry.dispatchStructuredEvent("shell-state", { state: "idle" }, "sess-synth");
        await new Promise<void>((r) => queueMicrotask(r));
        expect(shellStateHandler).not.toHaveBeenCalled();

        // Now detect agent (null → claude) — should trigger synthetic replay
        await detectAgentForTerminal(id, "idle");
        await new Promise<void>((r) => queueMicrotask(r));

        // Plugin should have received the synthetic shell-state event
        expect(shellStateHandler).toHaveBeenCalledWith(
          expect.objectContaining({ state: "idle" }),
          "sess-synth",
        );

        pluginRegistry.unregister("test-keepalive");
      });
    });

    it("fires agent-stopped for filtered plugins on direct agent→agent transitions", async () => {
      // Bug: when agentType switched from claude to codex without first passing
      // through null (user exits claude and immediately runs codex, before the
      // NULL_THRESHOLD idle-streak clears the agent), neither agent-started nor
      // agent-stopped was dispatched. Plugins filtered on agentTypes=["claude"]
      // (e.g. cache-keepalive) kept their internal per-session state and wrote
      // keepalive messages into the now-codex PTY. Fix: emit agent-stopped
      // before the store update (filter still matches old type) and agent-started
      // after (filter matches new type).
      let foregroundReturn: string | null = "claude";
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_session_foreground_process") return Promise.resolve(foregroundReturn);
        if (cmd === "discover_agent_session") return Promise.resolve(null);
        return Promise.resolve(null);
      });

      await testInScopeAsync(async () => {
        const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
        const { pluginRegistry } = await import("../../plugins/pluginRegistry");

        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-trans" }));
        store.update(id, { shellState: "idle" });

        const claudeEvents: string[] = [];
        await pluginRegistry.register(
          {
            id: "test-claude-only",
            onload: (host) => {
              host.onStateChange((e) => {
                if (e.sessionId === "sess-trans") claudeEvents.push(e.type);
              });
            },
            onunload: () => {},
          },
          ["pty:write"],
          [],
          ["claude"],
        );

        const codexEvents: string[] = [];
        await pluginRegistry.register(
          {
            id: "test-codex-only",
            onload: (host) => {
              host.onStateChange((e) => {
                if (e.sessionId === "sess-trans") codexEvents.push(e.type);
              });
            },
            onunload: () => {},
          },
          ["pty:write"],
          [],
          ["codex"],
        );

        // null → claude: claude-filtered plugin gets agent-started
        await detectAgentForTerminal(id, "busy");
        expect(store.get(id)?.agentType).toBe("claude");
        expect(claudeEvents).toEqual(["agent-started"]);
        expect(codexEvents).toEqual([]);

        // claude → codex (direct): claude plugin MUST receive agent-stopped,
        // codex plugin MUST receive agent-started
        foregroundReturn = "codex";
        await detectAgentForTerminal(id, "busy");
        expect(store.get(id)?.agentType).toBe("codex");
        expect(claudeEvents).toEqual(["agent-started", "agent-stopped"]);
        expect(codexEvents).toEqual(["agent-started"]);

        pluginRegistry.unregister("test-claude-only");
        pluginRegistry.unregister("test-codex-only");
      });
    });

    it("clears agentSessionId on agent→null transition and allows re-discovery", async () => {
      // NULL_THRESHOLD is 3: need 3 consecutive idle-source null detections before clearing.
      // Only source="idle" can clear — polls never clear (sticky agentType fix).
      mockInvoke
        .mockResolvedValueOnce("claude")       // poll 1: claude detected
        .mockResolvedValueOnce("uuid-1")       // discover: uuid-1
        .mockResolvedValueOnce("claude")       // poll 2: still claude
        .mockResolvedValueOnce(null)           // idle 1: null streak 1
        .mockResolvedValueOnce(null)           // idle 2: null streak 2
        .mockResolvedValueOnce(null)           // idle 3: null streak 3 → cleared
        .mockResolvedValueOnce("claude")       // poll 3: claude re-launched
        .mockResolvedValueOnce("uuid-2");      // re-discover: uuid-2

      await testInScopeAsync(async () => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

        const { useAgentPolling, detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
        useAgentPolling();

        await tick(30_000); // poll 1: claude + discovery queued
        await tick(30_000); // poll 2: still claude (discovery already done)
        expect(store.get(id)?.agentSessionId).toBe("uuid-1");

        // Idle-source detections can clear agentType after NULL_THRESHOLD consecutive nulls
        await detectAgentForTerminal(id, "idle"); // idle 1: null streak 1 — still holding
        await detectAgentForTerminal(id, "idle"); // idle 2: null streak 2 — still holding
        await detectAgentForTerminal(id, "idle"); // idle 3: null streak 3 → cleared
        expect(store.get(id)?.agentType).toBeNull();
        expect(store.get(id)?.agentSessionId).toBeNull();

        await tick(30_000); // poll 3: re-launched → re-discovery
        expect(store.get(id)?.agentType).toBe("claude");
        expect(store.get(id)?.agentSessionId).toBe("uuid-2");

      });
    });

    it("passes claimed_ids from other terminals to avoid duplicate assignment", async () => {
      // Two terminals, both running claude — processed sequentially
      mockInvoke
        .mockResolvedValueOnce("claude")     // term-1: get_session_foreground_process
        .mockResolvedValueOnce("uuid-a")     // term-1: discover_agent_session
        .mockResolvedValueOnce("claude")     // term-2: get_session_foreground_process
        .mockResolvedValueOnce("uuid-b");    // term-2: discover_agent_session

      await testInScopeAsync(async () => {
        const id1 = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
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

      });
    });
  });
});
