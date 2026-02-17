import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("errorHandlingStore", () => {
  let store: typeof import("../../stores/errorHandling").errorHandlingStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/errorHandling")).errorHandlingStore;
  });

  describe("handleError()", () => {
    it("returns retry for network errors", () => {
      createRoot((dispose) => {
        const decision = store.handleError("sess-1", "network error");
        expect(decision.action).toBe("retry");
        expect(decision.delayMs).toBeGreaterThan(0);
        dispose();
      });
    });

    it("returns abort for auth errors", () => {
      createRoot((dispose) => {
        const decision = store.handleError("sess-1", "unauthorized");
        expect(decision.action).toBe("abort");
        dispose();
      });
    });

    it("tracks retry info", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "network error");
        expect(store.getRetryInfo("sess-1")).toBeDefined();
        expect(store.getRetryInfo("sess-1")!.retryCount).toBe(1);
        dispose();
      });
    });

    it("clears retry info on abort", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "unauthorized");
        expect(store.getRetryInfo("sess-1")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("updateConfig()", () => {
    it("updates configuration", () => {
      createRoot((dispose) => {
        store.updateConfig({ maxRetries: 5 });
        expect(store.state.config.maxRetries).toBe(5);
        dispose();
      });
    });

    it("persists via Tauri invoke", () => {
      createRoot((dispose) => {
        store.updateConfig({ maxRetries: 5 });
        expect(mockInvoke).toHaveBeenCalledWith(
          "save_ui_prefs",
          expect.objectContaining({
            config: expect.objectContaining({
              error_handling: expect.objectContaining({ max_retries: 5 }),
            }),
          }),
        );
        dispose();
      });
    });
  });

  describe("setStrategy()", () => {
    it("updates strategy", () => {
      createRoot((dispose) => {
        store.setStrategy("abort");
        expect(store.state.config.strategy).toBe("abort");
        dispose();
      });
    });
  });

  describe("setMaxRetries()", () => {
    it("clamps to valid range", () => {
      createRoot((dispose) => {
        store.setMaxRetries(15); // Should clamp to 10
        expect(store.state.config.maxRetries).toBe(10);
        store.setMaxRetries(-1); // Should clamp to 0
        expect(store.state.config.maxRetries).toBe(0);
        dispose();
      });
    });
  });

  describe("setBaseDelay()", () => {
    it("clamps to valid range", () => {
      createRoot((dispose) => {
        store.setBaseDelay(50); // Should clamp to 100
        expect(store.state.config.baseDelayMs).toBe(100);
        store.setBaseDelay(100000); // Should clamp to 60000
        expect(store.state.config.baseDelayMs).toBe(60000);
        dispose();
      });
    });
  });

  describe("clearRetry()", () => {
    it("clears retry info for session", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "network error");
        store.clearRetry("sess-1");
        expect(store.getRetryInfo("sess-1")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("isRetrying()", () => {
    it("returns true when retrying", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "network error");
        expect(store.isRetrying("sess-1")).toBe(true);
        dispose();
      });
    });

    it("returns false when not retrying", () => {
      createRoot((dispose) => {
        expect(store.isRetrying("unknown")).toBe(false);
        dispose();
      });
    });
  });

  describe("getActiveRetries()", () => {
    it("returns all active retries", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "network error");
        store.handleError("sess-2", "connection refused");
        expect(store.getActiveRetries()).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("resetAll()", () => {
    it("clears all retry state", () => {
      createRoot((dispose) => {
        store.handleError("sess-1", "network error");
        store.handleError("sess-2", "connection refused");
        store.resetAll();
        expect(store.getActiveRetries()).toHaveLength(0);
        dispose();
      });
    });
  });

  describe("resetConfig()", () => {
    it("resets config to defaults", () => {
      createRoot((dispose) => {
        store.updateConfig({ maxRetries: 5, strategy: "abort" });
        store.resetConfig();
        expect(store.state.config.maxRetries).toBe(3);
        expect(store.state.config.strategy).toBe("retry");
        dispose();
      });
    });
  });

  describe("getHandler()", () => {
    it("returns the handler instance", () => {
      createRoot((dispose) => {
        const handler = store.getHandler();
        expect(handler).toBeDefined();
        expect(typeof handler.handle).toBe("function");
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads config from Rust backend", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "load_ui_prefs") {
          return { error_handling: { strategy: "abort", max_retries: 7 } };
        }
        return undefined;
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.config.strategy).toBe("abort");
        expect(store.state.config.maxRetries).toBe(7);
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-error-handling", JSON.stringify({
        strategy: "abort",
        maxRetries: 5,
      }));

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-error-handling")).toBeNull();
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", expect.anything());
        dispose();
      });
    });

    it("handles hydration failure gracefully", async () => {
      mockInvoke.mockRejectedValue(new Error("load failed"));

      await createRoot(async (dispose) => {
        await store.hydrate(); // Should not throw
        expect(store.state.config.strategy).toBe("retry"); // Default
        dispose();
      });
    });

    it("handles corrupt legacy data gracefully", async () => {
      localStorage.setItem("tui-commander-error-handling", "not-json{{{");

      await createRoot(async (dispose) => {
        await store.hydrate(); // Should not throw
        expect(localStorage.getItem("tui-commander-error-handling")).toBeNull();
        dispose();
      });
    });
  });

  describe("handleError() non-retry decisions", () => {
    it("clears retry info for skip action", () => {
      createRoot((dispose) => {
        // "skip" strategy doesn't retry at all
        store.setStrategy("skip");
        const decision = store.handleError("sess-1", "network error");
        expect(decision.action).toBe("skip");
        expect(store.isRetrying("sess-1")).toBe(false);
        dispose();
      });
    });
  });
});
