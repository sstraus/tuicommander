import { describe, it, expect, vi, beforeEach } from "vitest";
import type { DetectedPrompt } from "../../types";
import { testInScope } from "../helpers/store";

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
      testInScope(() => {
        store.showPrompt(mockPrompt);
        expect(store.state.activePrompt).toEqual(mockPrompt);
        expect(store.state.selectedOptionIndex).toBe(0);
      });
    });

    it("clears output buffer", () => {
      testInScope(() => {
        store.appendOutput("some output");
        store.showPrompt(mockPrompt);
        expect(store.state.outputBuffer).toBe("");
      });
    });
  });

  describe("hidePrompt()", () => {
    it("clears active prompt", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.hidePrompt();
        expect(store.state.activePrompt).toBeNull();
        expect(store.state.selectedOptionIndex).toBe(0);
      });
    });
  });

  describe("selectOption()", () => {
    it("selects option by index", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        expect(store.state.selectedOptionIndex).toBe(2);
      });
    });

    it("ignores invalid indices", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectOption(-1);
        expect(store.state.selectedOptionIndex).toBe(0);
        store.selectOption(10);
        expect(store.state.selectedOptionIndex).toBe(0);
      });
    });

    it("ignores when no prompt", () => {
      testInScope(() => {
        store.selectOption(1); // Should not throw
      });
    });
  });

  describe("selectPrevious()", () => {
    it("moves selection up", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        store.selectPrevious();
        expect(store.state.selectedOptionIndex).toBe(1);
      });
    });

    it("clamps at 0", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectPrevious();
        expect(store.state.selectedOptionIndex).toBe(0);
      });
    });
  });

  describe("selectNext()", () => {
    it("moves selection down", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectNext();
        expect(store.state.selectedOptionIndex).toBe(1);
      });
    });

    it("clamps at last option", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        store.selectOption(2);
        store.selectNext();
        expect(store.state.selectedOptionIndex).toBe(2);
      });
    });
  });

  describe("getSelectedOptionNumber()", () => {
    it("returns 1-indexed option number", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        expect(store.getSelectedOptionNumber()).toBe(1);
        store.selectOption(2);
        expect(store.getSelectedOptionNumber()).toBe(3);
      });
    });
  });

  describe("appendOutput()", () => {
    it("appends to output buffer", () => {
      testInScope(() => {
        store.appendOutput("hello ");
        store.appendOutput("world");
        expect(store.state.outputBuffer).toBe("hello world");
      });
    });

    it("trims buffer when exceeding max size", () => {
      testInScope(() => {
        const bigData = "x".repeat(6000);
        store.appendOutput(bigData);
        expect(store.state.outputBuffer.length).toBeLessThanOrEqual(5000);
      });
    });
  });

  describe("clearOutputBuffer()", () => {
    it("clears the buffer", () => {
      testInScope(() => {
        store.appendOutput("data");
        store.clearOutputBuffer();
        expect(store.state.outputBuffer).toBe("");
      });
    });
  });

  describe("appendStats()", () => {
    it("appends to stats buffer", () => {
      testInScope(() => {
        store.appendStats("stat1");
        store.appendStats("stat2");
        expect(store.state.statsBuffer).toBe("stat1stat2");
      });
    });

    it("trims stats buffer when exceeding max size", () => {
      testInScope(() => {
        const bigData = "x".repeat(4000);
        store.appendStats(bigData);
        expect(store.state.statsBuffer.length).toBeLessThanOrEqual(3000);
      });
    });
  });

  describe("session stats", () => {
    it("updateSessionStats creates new stats", () => {
      testInScope(() => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        expect(store.getSessionStats("sess-1")?.toolUses).toBe(5);
      });
    });

    it("updateSessionStats merges with existing", () => {
      testInScope(() => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        store.updateSessionStats("sess-1", { tokens: 100 });
        const stats = store.getSessionStats("sess-1");
        expect(stats?.toolUses).toBe(5);
        expect(stats?.tokens).toBe(100);
      });
    });

    it("clearSessionStats removes stats", () => {
      testInScope(() => {
        store.updateSessionStats("sess-1", { toolUses: 5 });
        store.clearSessionStats("sess-1");
        expect(store.getSessionStats("sess-1")).toBeUndefined();
      });
    });
  });

  describe("isActive()", () => {
    it("returns false when no prompt", () => {
      testInScope(() => {
        expect(store.isActive()).toBe(false);
      });
    });

    it("returns true when prompt is active", () => {
      testInScope(() => {
        store.showPrompt(mockPrompt);
        expect(store.isActive()).toBe(true);
      });
    });
  });
});
