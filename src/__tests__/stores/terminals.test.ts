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

  describe("TabLayout", () => {
    describe("default layout", () => {
      it("initializes with direction none and empty panes", () => {
        testInScope(() => {
          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
        });
      });
    });

    describe("splitPane()", () => {
      it("splits a single pane vertically", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1", cwd: "/tmp" }));
          store.setActive(id1);
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const newId = store.splitPane("vertical");

          expect(newId).toBeDefined();
          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toHaveLength(2);
          expect(store.state.layout.panes[0]).toBe(id1);
          expect(store.state.layout.panes[1]).toBe(newId);
          expect(store.state.layout.ratios).toEqual([0.5, 0.5]);
          expect(store.state.layout.activePaneIndex).toBe(1);
        });
      });

      it("splits a single pane horizontally", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const newId = store.splitPane("horizontal");

          expect(store.state.layout.direction).toBe("horizontal");
          expect(store.state.layout.panes).toHaveLength(2);
          expect(store.state.layout.panes[1]).toBe(newId);
          expect(store.state.layout.ratios).toEqual([0.5, 0.5]);
        });
      });

      it("adds 3rd pane when same direction active", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const id2 = store.splitPane("vertical");
          expect(id2).not.toBeNull();

          const id3 = store.splitPane("vertical");
          expect(id3).not.toBeNull();
          expect(store.state.layout.panes).toHaveLength(3);
          expect(store.state.layout.direction).toBe("vertical");

          const r = 1 / 3;
          expect(store.state.layout.ratios[0]).toBeCloseTo(r);
          expect(store.state.layout.ratios[1]).toBeCloseTo(r);
          expect(store.state.layout.ratios[2]).toBeCloseTo(r);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
        });
      });

      it("returns null when opposite direction requested", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });
          store.splitPane("vertical");

          const result = store.splitPane("horizontal");
          expect(result).toBeNull();
          expect(store.state.layout.panes).toHaveLength(2);
        });
      });

      it("returns null at MAX_SPLIT_PANES (6)", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          // Split up to max (6 panes = 1 original + 5 splits)
          for (let i = 1; i < 6; i++) {
            const r = store.splitPane("vertical");
            expect(r).not.toBeNull();
          }
          expect(store.state.layout.panes).toHaveLength(6);

          // One more should be rejected
          const result = store.splitPane("vertical");
          expect(result).toBeNull();
          expect(store.state.layout.panes).toHaveLength(6);
        });
      });

      it("returns null if no panes in layout", () => {
        testInScope(() => {
          const result = store.splitPane("vertical");
          expect(result).toBeNull();
        });
      });

      it("inherits cwd from the source pane", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1", cwd: "/projects/foo" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const newId = store.splitPane("vertical");

          expect(newId).toBeDefined();
          expect(store.get(newId!)!.cwd).toBe("/projects/foo");
        });
      });
    });

    describe("closeSplitPane()", () => {
      it("collapses split to single pane when closing pane 1", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 1 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
        });
      });

      it("collapses split to single pane when closing pane 0", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "horizontal", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.closeSplitPane(0);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id2]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
        });
      });

      it("closes middle pane of 3, redistributes ratios proportionally", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          const id3 = store.add(makeTerminal({ name: "T3" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [0.4, 0.2, 0.4], activePaneIndex: 1 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toEqual([id1, id3]);
          expect(store.state.layout.ratios[0]).toBeCloseTo(0.5);
          expect(store.state.layout.ratios[1]).toBeCloseTo(0.5);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
          // activePaneIndex should be clamped
          expect(store.state.layout.activePaneIndex).toBe(1);
        });
      });

      it("decrements activePaneIndex when a pane before the active one is closed", () => {
        testInScope(() => {
          // 4 panes [A,B,C,D], activePaneIndex: 2 (C), close pane 0 (A)
          // Result: [B,C,D], activePaneIndex should be 1 (still pointing at C)
          const idA = store.add(makeTerminal({ name: "A" }));
          const idB = store.add(makeTerminal({ name: "B" }));
          const idC = store.add(makeTerminal({ name: "C" }));
          const idD = store.add(makeTerminal({ name: "D" }));
          store.setLayout({ direction: "vertical", panes: [idA, idB, idC, idD], ratios: [0.25, 0.25, 0.25, 0.25], activePaneIndex: 2 });

          store.closeSplitPane(0);

          expect(store.state.layout.panes).toEqual([idB, idC, idD]);
          expect(store.state.layout.activePaneIndex).toBe(1); // still points at C
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
        });
      });

      it("closes last pane of 3, activePaneIndex decrements", () => {
        testInScope(() => {
          // 3 panes, activePaneIndex: 2, close index 2
          // Result: 2 panes, activePaneIndex should be 1
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          const id3 = store.add(makeTerminal({ name: "T3" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [0.4, 0.3, 0.3], activePaneIndex: 2 });

          store.closeSplitPane(2);

          expect(store.state.layout.panes).toEqual([id1, id2]);
          expect(store.state.layout.activePaneIndex).toBe(1);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
        });
      });

      it("does nothing if not split", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
        });
      });
    });

    describe("setHandleRatio()", () => {
      it("adjusts boundary between two adjacent panes", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setHandleRatio(0, 0.7);
          expect(store.state.layout.ratios[0]).toBeCloseTo(0.7);
          expect(store.state.layout.ratios[1]).toBeCloseTo(0.3);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
        });
      });

      it("enforces minimum pane fraction", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          // Try to push boundary to 0.99 — should be clamped so second pane >= MIN_PANE_FRACTION
          store.setHandleRatio(0, 0.99);
          expect(store.state.layout.ratios[1]).toBeGreaterThanOrEqual(0.05);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
        });
      });
    });

    describe("setActivePaneIndex()", () => {
      it("sets the active pane index", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setActivePaneIndex(1);
          expect(store.state.layout.activePaneIndex).toBe(1);
        });
      });

      it("sets back to 0", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setActivePaneIndex(1);
          store.setActivePaneIndex(0);
          expect(store.state.layout.activePaneIndex).toBe(0);
        });
      });

      it("accepts any valid index", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));
          const id3 = store.add(makeTerminal({ name: "T3" }));
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [1/3, 1/3, 1/3], activePaneIndex: 0 });

          store.setActivePaneIndex(2);
          expect(store.state.layout.activePaneIndex).toBe(2);

          // Clamps to valid range
          store.setActivePaneIndex(10);
          expect(store.state.layout.activePaneIndex).toBe(2);
        });
      });
    });

    describe("setLayout()", () => {
      it("sets the complete layout", () => {
        testInScope(() => {
          const id1 = store.add(makeTerminal({ name: "T1" }));
          const id2 = store.add(makeTerminal({ name: "T2" }));

          store.setLayout({
            direction: "vertical",
            panes: [id1, id2],
            ratios: [0.6, 0.4],
            activePaneIndex: 1,
          });

          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toEqual([id1, id2]);
          expect(store.state.layout.ratios).toEqual([0.6, 0.4]);
          expect(store.state.layout.activePaneIndex).toBe(1);
        });
      });
    });
  });
});
