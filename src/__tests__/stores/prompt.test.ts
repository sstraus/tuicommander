import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import type { DetectedPrompt } from "../../types";

describe("promptStore", () => {
  let store: typeof import("../../stores/prompt").promptStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/prompt")).promptStore;
  });

  const mockPrompt: DetectedPrompt = {
    question: "Pick an option:",
    options: ["Option A", "Option B", "Option C"],
    sessionId: "sess-1",
  };

  describe("showPrompt()", () => {
    it("sets the active prompt", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        expect(store.state.activePrompt).toEqual(mockPrompt);
        expect(store.state.selectedOptionIndex).toBe(0);
        dispose();
      });
    });

    it("clears output buffer", () => {
      createRoot((dispose) => {
        store.appendOutput("some output");
        store.showPrompt(mockPrompt);
        expect(store.state.outputBuffer).toBe("");
        dispose();
      });
    });
  });

  describe("hidePrompt()", () => {
    it("clears active prompt", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.hidePrompt();
        expect(store.state.activePrompt).toBeNull();
        expect(store.state.selectedOptionIndex).toBe(0);
        dispose();
      });
    });
  });

  describe("selectOption()", () => {
    it("selects option by index", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        expect(store.state.selectedOptionIndex).toBe(2);
        dispose();
      });
    });

    it("ignores invalid indices", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectOption(-1);
        expect(store.state.selectedOptionIndex).toBe(0);
        store.selectOption(10);
        expect(store.state.selectedOptionIndex).toBe(0);
        dispose();
      });
    });

    it("ignores when no prompt", () => {
      createRoot((dispose) => {
        store.selectOption(1); // Should not throw
        dispose();
      });
    });
  });

  describe("selectPrevious()", () => {
    it("moves selection up", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        store.selectPrevious();
        expect(store.state.selectedOptionIndex).toBe(1);
        dispose();
      });
    });

    it("clamps at 0", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectPrevious();
        expect(store.state.selectedOptionIndex).toBe(0);
        dispose();
      });
    });
  });

  describe("selectNext()", () => {
    it("moves selection down", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectNext();
        expect(store.state.selectedOptionIndex).toBe(1);
        dispose();
      });
    });

    it("clamps at last option", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        store.selectNext();
        expect(store.state.selectedOptionIndex).toBe(2);
        dispose();
      });
    });
  });

  describe("getSelectedOptionNumber()", () => {
    it("returns 1-indexed option number", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        expect(store.getSelectedOptionNumber()).toBe(1);
        store.selectOption(2);
        expect(store.getSelectedOptionNumber()).toBe(3);
        dispose();
      });
    });
  });

  describe("appendOutput()", () => {
    it("appends to output buffer", () => {
      createRoot((dispose) => {
        store.appendOutput("hello ");
        store.appendOutput("world");
        expect(store.state.outputBuffer).toBe("hello world");
        dispose();
      });
    });

    it("trims buffer when exceeding max size", () => {
      createRoot((dispose) => {
        const bigData = "x".repeat(6000);
        store.appendOutput(bigData);
        expect(store.state.outputBuffer.length).toBeLessThanOrEqual(5000);
        dispose();
      });
    });
  });

  describe("clearOutputBuffer()", () => {
    it("clears the buffer", () => {
      createRoot((dispose) => {
        store.appendOutput("data");
        store.clearOutputBuffer();
        expect(store.state.outputBuffer).toBe("");
        dispose();
      });
    });
  });

  describe("appendStats()", () => {
    it("appends to stats buffer", () => {
      createRoot((dispose) => {
        store.appendStats("stat1");
        store.appendStats("stat2");
        expect(store.state.statsBuffer).toBe("stat1stat2");
        dispose();
      });
    });

    it("trims stats buffer when exceeding max size", () => {
      createRoot((dispose) => {
        const bigData = "x".repeat(4000);
        store.appendStats(bigData);
        expect(store.state.statsBuffer.length).toBeLessThanOrEqual(3000);
        dispose();
      });
    });
  });

  describe("session stats", () => {
    it("updateSessionStats creates new stats", () => {
      createRoot((dispose) => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        expect(store.getSessionStats("sess-1")?.toolUses).toBe(5);
        dispose();
      });
    });

    it("updateSessionStats merges with existing", () => {
      createRoot((dispose) => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        store.updateSessionStats("sess-1", { tokens: 100 });
        const stats = store.getSessionStats("sess-1");
        expect(stats?.toolUses).toBe(5);
        expect(stats?.tokens).toBe(100);
        dispose();
      });
    });

    it("clearSessionStats removes stats", () => {
      createRoot((dispose) => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        store.clearSessionStats("sess-1");
        expect(store.getSessionStats("sess-1")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("isActive()", () => {
    it("returns false when no prompt", () => {
      createRoot((dispose) => {
        expect(store.isActive()).toBe(false);
        dispose();
      });
    });

    it("returns true when prompt is active", () => {
      createRoot((dispose) => {
        store.showPrompt(mockPrompt);
        expect(store.isActive()).toBe(true);
        dispose();
      });
    });
  });
});
