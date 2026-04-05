import { describe, it, expect, vi, beforeEach } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

describe("terminalsStore", () => {
  let store: typeof import("../../stores/terminals").terminalsStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/terminals")).terminalsStore;
  });

  describe("add()", () => {
    it("creates a terminal with generated ID", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        expect(id).toBe("term-1");
        expect(store.get(id)).toBeDefined();
        expect(store.get(id)!.name).toBe("Test");
        expect(store.get(id)!.activity).toBe(false);
        expect(store.get(id)!.progress).toBeNull();
      });
    });

    it("increments counter for each terminal", () => {
      testInScope(() => {
        const id1 = store.add(makeTerminal({ name: "T1" }));
        const id2 = store.add(makeTerminal({ name: "T2" }));
        expect(id1).toBe("term-1");
        expect(id2).toBe("term-2");
      });
    });
  });

  describe("remove()", () => {
    it("removes a terminal", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
      });
    });

    it("sets activeId to null when active terminal is removed (caller handles replacement)", () => {
      testInScope(() => {
        const id1 = store.add(makeTerminal({ name: "T1" }));
        store.add(makeTerminal({ name: "T2" }));
        store.setActive(id1);
        store.remove(id1);
        expect(store.state.activeId).toBeNull();
      });
    });

    it("sets activeId to null when last terminal is removed", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setActive(id);
        store.remove(id);
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  describe("setActive()", () => {
    it("sets the active terminal", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setActive(id);
        expect(store.state.activeId).toBe(id);
      });
    });

    it("clears activity indicator when setting active", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { activity: true });
        store.setActive(id);
        expect(store.get(id)!.activity).toBe(false);
      });
    });

    it("accepts null", () => {
      testInScope(() => {
        store.setActive(null);
        expect(store.state.activeId).toBeNull();
      });
    });
  });

  describe("getActive()", () => {
    it("returns the active terminal", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setActive(id);
        expect(store.getActive()?.id).toBe(id);
      });
    });

    it("returns undefined when no active", () => {
      testInScope(() => {
        expect(store.getActive()).toBeUndefined();
      });
    });
  });

  describe("update()", () => {
    it("updates terminal properties", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { name: "Updated", fontSize: 16 });
        expect(store.get(id)!.name).toBe("Updated");
        expect(store.get(id)!.fontSize).toBe(16);
      });
    });
  });

  describe("setSessionId()", () => {
    it("updates session ID", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setSessionId(id, "sess-1");
        expect(store.get(id)!.sessionId).toBe("sess-1");
      });
    });
  });

  describe("setFontSize()", () => {
    it("updates font size", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setFontSize(id, 18);
        expect(store.get(id)!.fontSize).toBe(18);
      });
    });

    it("fontSize is accessible via direct store path", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());

        // Verify direct store path access works (used in Terminal.tsx for SolidJS reactivity)
        expect(store.state.terminals[id]?.fontSize).toBe(14);

        store.setFontSize(id, 18);
        expect(store.state.terminals[id]?.fontSize).toBe(18);

        store.setFontSize(id, 22);
        expect(store.state.terminals[id]?.fontSize).toBe(22);

        // Verify it returns undefined for non-existent terminal
        expect(store.state.terminals["nonexistent"]?.fontSize).toBeUndefined();

      });
    });
  });

  describe("awaiting input", () => {
    it("sets awaiting input type", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setAwaitingInput(id, "question");
        expect(store.get(id)!.awaitingInput).toBe("question");
      });
    });

    it("clears awaiting input", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ awaitingInput: "question" }));
        store.clearAwaitingInput(id);
        expect(store.get(id)!.awaitingInput).toBeNull();
      });
    });

    it("hasAwaitingInput returns true when any terminal is awaiting", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1" }));
        const id2 = store.add(makeTerminal({ name: "T2" }));
        store.setAwaitingInput(id2, "error");
        expect(store.hasAwaitingInput()).toBe(true);
      });
    });

    it("hasAwaitingInput returns false when none awaiting", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1" }));
        expect(store.hasAwaitingInput()).toBe(false);
      });
    });

    it("shellState update does not clear awaitingInput (PTY output must not erase question state)", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setAwaitingInput(id, "question");
        // Simulate PTY output: shellState goes busy
        store.update(id, { shellState: "busy" });
        // awaitingInput must survive — the question is still pending
        expect(store.get(id)!.awaitingInput).toBe("question");
        // And going back to idle also must not clear it
        store.update(id, { shellState: "idle" });
        expect(store.get(id)!.awaitingInput).toBe("question");
      });
    });

    it("getAwaitingInputIds returns correct IDs", () => {
      testInScope(() => {
        const id1 = store.add(makeTerminal({ name: "T1" }));
        const id2 = store.add(makeTerminal({ name: "T2" }));
        store.setAwaitingInput(id2, "error");
        const ids = store.getAwaitingInputIds();
        expect(ids).toContain(id2);
        expect(ids).not.toContain(id1);
      });
    });
  });

  describe("getIds()", () => {
    it("returns all terminal IDs", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1" }));
        store.add(makeTerminal({ name: "T2" }));
        expect(store.getIds()).toHaveLength(2);
      });
    });
  });

  describe("sessionToTerminal reverse map", () => {
    it("getTerminalForSession returns terminal ID when session is assigned", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
        expect(store.getTerminalForSession("sess-abc")).toBe(id);
      });
    });

    it("getTerminalForSession returns null for unknown session", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
        expect(store.getTerminalForSession("not-a-session")).toBeNull();
      });
    });

    it("getTerminalForSession returns null for terminal with null sessionId", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1" }));
        expect(store.getTerminalForSession("sess-xyz")).toBeNull();
      });
    });

    it("map is updated when setSessionId is called", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ name: "T1" }));
        expect(store.getTerminalForSession("sess-new")).toBeNull();
        store.setSessionId(id, "sess-new");
        expect(store.getTerminalForSession("sess-new")).toBe(id);
      });
    });

    it("map is updated when setSessionId reassigns to a new session", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-old" }));
        store.setSessionId(id, "sess-new");
        expect(store.getTerminalForSession("sess-old")).toBeNull();
        expect(store.getTerminalForSession("sess-new")).toBe(id);
      });
    });

    it("map entry is removed when terminal is removed", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
        expect(store.getTerminalForSession("sess-abc")).toBe(id);
        store.remove(id);
        expect(store.getTerminalForSession("sess-abc")).toBeNull();
      });
    });

    it("getAgentTypeForSession uses reverse map for O(1) lookup", () => {
      testInScope(() => {
        const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-agent" }));
        store.update(id, { agentType: "claude" });
        expect(store.getAgentTypeForSession("sess-agent")).toBe("claude");
      });
    });

    it("getAgentTypeForSession returns null for unknown session", () => {
      testInScope(() => {
        store.add(makeTerminal({ name: "T1", sessionId: "sess-agent" }));
        expect(store.getAgentTypeForSession("unknown-sess")).toBeNull();
      });
    });
  });

  describe("agentType", () => {
    it("initializes agentType as null", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        expect(store.get(id)!.agentType).toBeNull();
      });
    });

    it("can be updated to a known agent", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { agentType: "claude" });
        expect(store.get(id)!.agentType).toBe("claude");
      });
    });

    it("can be cleared back to null", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { agentType: "gemini" });
        store.update(id, { agentType: null });
        expect(store.get(id)!.agentType).toBeNull();
      });
    });
  });

  describe("unseen", () => {
    it("initializes unseen as false", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        expect(store.get(id)!.unseen).toBe(false);
      });
    });

    it("clears unseen when setting active", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { unseen: true });
        store.setActive(id);
        expect(store.get(id)!.unseen).toBe(false);
      });
    });

    it("setAwaitingInput does NOT set unseen (question dot is sufficient)", () => {
      testInScope(() => {
        const id1 = store.add(makeTerminal({ name: "T1" }));
        const id2 = store.add(makeTerminal({ name: "T2" }));
        store.setActive(id1);
        // Terminal id2 is not active — awaitingInput should NOT set unseen
        // (the orange/red dot already communicates "needs attention")
        store.setAwaitingInput(id2, "question");
        expect(store.get(id2)!.unseen).toBe(false);
      });
    });
  });

  describe("shellState", () => {
    it("initializes shellState as null", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        expect(store.get(id)!.shellState).toBeNull();
      });
    });

    it("can be updated to busy", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.shellState).toBe("busy");
      });
    });

    it("can be updated to idle", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { shellState: "idle" });
        expect(store.get(id)!.shellState).toBe("idle");
      });
    });

    it("clears awaitingInput on idle→busy transition", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { shellState: "idle" });
        store.setAwaitingInput(id, "question");
        expect(store.get(id)!.awaitingInput).toBe("question");
        // Agent resumes output → idle→busy transition should clear stale question
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.awaitingInput).toBeNull();
      });
    });

    it("does not clear awaitingInput on null→busy transition", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.setAwaitingInput(id, "question");
        // Initial busy (from null) should NOT clear — agent hasn't been idle yet
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.awaitingInput).toBe("question");
      });
    });

    it("preserves shellState on setActive", () => {
      testInScope(() => {
        const id = store.add(makeTerminal());
        store.update(id, { shellState: "idle" });
        store.setActive(id);
        expect(store.get(id)!.shellState).toBe("idle");
      });
    });
  });

  describe("getCount()", () => {
    it("returns terminal count", () => {
      testInScope(() => {
        expect(store.getCount()).toBe(0);
        store.add(makeTerminal({ name: "T1" }));
        expect(store.getCount()).toBe(1);
      });
    });
  });

});
