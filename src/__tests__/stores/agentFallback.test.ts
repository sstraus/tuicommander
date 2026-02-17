import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("agentFallbackStore", () => {
  let store: typeof import("../../stores/agentFallback").agentFallbackStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/agentFallback")).agentFallbackStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("defaults", () => {
    it("defaults to claude as primary and active", () => {
      createRoot((dispose) => {
        expect(store.getActiveAgent()).toBe("claude");
        expect(store.state.primaryAgent).toBe("claude");
        expect(store.isUsingFallback()).toBe(false);
        dispose();
      });
    });
  });

  describe("configure()", () => {
    it("configures the fallback chain", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "gemini",
          fallbacks: ["claude", "aider"],
          recoveryIntervalMs: 30000,
          autoRecovery: true,
        });
        expect(store.state.primaryAgent).toBe("gemini");
        expect(store.state.fallbackChain).toEqual(["claude", "aider"]);
        dispose();
      });
    });

    it("persists via Tauri invoke", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "gemini",
          fallbacks: ["claude"],
          recoveryIntervalMs: 30000,
          autoRecovery: true,
        });
        expect(mockInvoke).toHaveBeenCalledWith(
          "save_agent_config",
          expect.objectContaining({
            config: expect.objectContaining({ primary_agent: "gemini" }),
          }),
        );
        dispose();
      });
    });
  });

  describe("markUnavailable()", () => {
    it("marks agent as unavailable", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        expect(store.state.unavailableAgents.has("claude")).toBe(true);
        dispose();
      });
    });

    it("triggers fallback when active agent is marked unavailable", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        // Should switch to next available in chain
        expect(store.getActiveAgent()).not.toBe("claude");
        expect(store.isUsingFallback()).toBe(true);
        dispose();
      });
    });
  });

  describe("markAvailable()", () => {
    it("marks agent as available", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        store.markAvailable("claude");
        expect(store.state.unavailableAgents.has("claude")).toBe(false);
        dispose();
      });
    });

    it("recovers to primary when primary becomes available", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        expect(store.isUsingFallback()).toBe(true);
        store.markAvailable("claude");
        expect(store.getActiveAgent()).toBe("claude");
        expect(store.isUsingFallback()).toBe(false);
        dispose();
      });
    });
  });

  describe("switchToFallback()", () => {
    it("finds first available agent in chain", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        // Default chain is claude, gemini, opencode, aider, codex
        // claude is unavailable, should pick gemini
        expect(store.getActiveAgent()).toBe("gemini");
        dispose();
      });
    });
  });

  describe("getStatusMessage()", () => {
    it("returns null when not using fallback", () => {
      createRoot((dispose) => {
        expect(store.getStatusMessage()).toBeNull();
        dispose();
      });
    });

    it("returns status message when using fallback", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        const msg = store.getStatusMessage();
        expect(msg).toContain("fallback");
        expect(msg).toContain("claude");
        dispose();
      });
    });
  });

  describe("setPrimary()", () => {
    it("sets primary agent", () => {
      createRoot((dispose) => {
        store.setPrimary("gemini");
        expect(store.state.primaryAgent).toBe("gemini");
        expect(store.getActiveAgent()).toBe("gemini");
        dispose();
      });
    });
  });

  describe("forceResetToPrimary()", () => {
    it("resets to primary regardless of state", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        store.forceResetToPrimary();
        expect(store.getActiveAgent()).toBe("claude");
        expect(store.isUsingFallback()).toBe(false);
        expect(store.state.unavailableAgents.size).toBe(0);
        dispose();
      });
    });
  });

  describe("cleanup()", () => {
    it("stops recovery checks", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        store.cleanup();
        expect(store.state.recoveryIntervalId).toBeNull();
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads config from Rust backend", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "load_agent_config") {
          return {
            primary_agent: "gemini",
            auto_recovery: false,
            fallback_chain: ["claude", "aider"],
            recovery_interval_ms: 30000,
          };
        }
        return undefined;
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.primaryAgent).toBe("gemini");
        expect(store.state.activeAgent).toBe("gemini");
        expect(store.state.autoRecovery).toBe(false);
        expect(store.state.fallbackChain).toEqual(["claude", "aider"]);
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-agent-fallback", JSON.stringify({
        primary: "gemini",
        autoRecovery: true,
        fallbacks: ["claude"],
        recoveryIntervalMs: 45000,
      }));

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-agent-fallback")).toBeNull();
        expect(mockInvoke).toHaveBeenCalledWith("save_agent_config", expect.anything());
        dispose();
      });
    });

    it("handles hydration failure gracefully", async () => {
      mockInvoke.mockRejectedValue(new Error("load failed"));

      await createRoot(async (dispose) => {
        await store.hydrate(); // Should not throw
        expect(store.state.primaryAgent).toBe("claude"); // Default
        dispose();
      });
    });
  });

  describe("switchToFallback() all unavailable", () => {
    it("warns when all agents are unavailable", () => {
      createRoot((dispose) => {
        const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
        // Mark all agents as unavailable
        store.markUnavailable("claude");
        store.markUnavailable("gemini");
        store.markUnavailable("opencode");
        store.markUnavailable("aider");
        store.markUnavailable("codex");
        expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("All agents unavailable"));
        warnSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("configure() saveConfig error", () => {
    it("handles save failure gracefully (fire-and-forget)", async () => {
      const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
      mockInvoke.mockRejectedValueOnce(new Error("save failed"));

      createRoot((dispose) => {
        store.configure({
          primary: "gemini",
          fallbacks: ["claude"],
          recoveryIntervalMs: 30000,
          autoRecovery: true,
        });
        // State should be updated even though save failed
        expect(store.state.primaryAgent).toBe("gemini");
        dispose();
      });

      // Let the rejected promise settle
      await vi.advanceTimersByTimeAsync(0);
      expect(debugSpy).toHaveBeenCalledWith(
        expect.stringContaining("Failed to save agent fallback config"),
        expect.anything(),
      );
      debugSpy.mockRestore();
    });
  });

  describe("configure() with usingFallback", () => {
    it("starts recovery checks when reconfigured while using fallback", () => {
      createRoot((dispose) => {
        store.markUnavailable("claude");
        expect(store.isUsingFallback()).toBe(true);

        store.configure({
          primary: "claude",
          fallbacks: ["gemini", "aider"],
          recoveryIntervalMs: 5000,
          autoRecovery: true,
        });

        expect(store.state.recoveryIntervalId).not.toBeNull();
        store.cleanup();
        dispose();
      });
    });

    it("updates active agent when not using fallback", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "gemini",
          fallbacks: ["claude"],
          recoveryIntervalMs: 30000,
          autoRecovery: true,
        });

        expect(store.getActiveAgent()).toBe("gemini");
        dispose();
      });
    });
  });

  describe("tryRecoverToPrimary()", () => {
    it("does nothing when not using fallback", () => {
      createRoot((dispose) => {
        store.tryRecoverToPrimary();
        expect(store.getActiveAgent()).toBe("claude");
        dispose();
      });
    });
  });

  describe("startRecoveryChecks()", () => {
    it("does nothing when autoRecovery is disabled", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "claude",
          fallbacks: ["gemini"],
          recoveryIntervalMs: 1000,
          autoRecovery: false,
        });
        store.markUnavailable("claude");
        store.startRecoveryChecks();
        expect(store.state.recoveryIntervalId).toBeNull();
        dispose();
      });
    });

    it("starts interval when autoRecovery is enabled", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "claude",
          fallbacks: ["gemini"],
          recoveryIntervalMs: 1000,
          autoRecovery: true,
        });
        store.markUnavailable("claude");
        store.startRecoveryChecks();
        expect(store.state.recoveryIntervalId).not.toBeNull();
        store.cleanup();
        dispose();
      });
    });

    it("recovery interval checks if primary is available", () => {
      createRoot((dispose) => {
        store.configure({
          primary: "claude",
          fallbacks: ["gemini"],
          recoveryIntervalMs: 100,
          autoRecovery: true,
        });
        store.markUnavailable("claude");
        store.startRecoveryChecks();

        // Advance timer to trigger recovery check
        vi.advanceTimersByTime(150);

        // Should have tried to recover — primary has no rate limit
        // so it should be marked available
        expect(store.getActiveAgent()).toBe("claude");
        expect(store.isUsingFallback()).toBe(false);
        store.cleanup();
        dispose();
      });
    });
  });
});
