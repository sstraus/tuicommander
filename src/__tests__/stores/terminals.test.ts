import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

describe("terminalsStore", () => {
  let store: typeof import("../../stores/terminals").terminalsStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/terminals")).terminalsStore;
  });

  describe("add()", () => {
    it("creates a terminal with generated ID", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        expect(id).toBe("term-1");
        expect(store.get(id)).toBeDefined();
        expect(store.get(id)!.name).toBe("Test");
        expect(store.get(id)!.activity).toBe(false);
        expect(store.get(id)!.progress).toBeNull();
        dispose();
      });
    });

    it("increments counter for each terminal", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        expect(id1).toBe("term-1");
        expect(id2).toBe("term-2");
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes a terminal", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
        dispose();
      });
    });

    it("sets activeId to null when active terminal is removed (caller handles replacement)", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setActive(id1);
        store.remove(id1);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });

    it("sets activeId to null when last terminal is removed", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setActive(id);
        store.remove(id);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });
  });

  describe("setActive()", () => {
    it("sets the active terminal", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setActive(id);
        expect(store.state.activeId).toBe(id);
        dispose();
      });
    });

    it("clears activity indicator when setting active", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { activity: true });
        store.setActive(id);
        expect(store.get(id)!.activity).toBe(false);
        dispose();
      });
    });

    it("accepts null", () => {
      createRoot((dispose) => {
        store.setActive(null);
        expect(store.state.activeId).toBeNull();
        dispose();
      });
    });
  });

  describe("getActive()", () => {
    it("returns the active terminal", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setActive(id);
        expect(store.getActive()?.id).toBe(id);
        dispose();
      });
    });

    it("returns undefined when no active", () => {
      createRoot((dispose) => {
        expect(store.getActive()).toBeUndefined();
        dispose();
      });
    });
  });

  describe("update()", () => {
    it("updates terminal properties", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { name: "Updated", fontSize: 16 });
        expect(store.get(id)!.name).toBe("Updated");
        expect(store.get(id)!.fontSize).toBe(16);
        dispose();
      });
    });
  });

  describe("setSessionId()", () => {
    it("updates session ID", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setSessionId(id, "sess-1");
        expect(store.get(id)!.sessionId).toBe("sess-1");
        dispose();
      });
    });
  });

  describe("setFontSize()", () => {
    it("updates font size", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setFontSize(id, 18);
        expect(store.get(id)!.fontSize).toBe(18);
        dispose();
      });
    });

    it("fontSize is accessible via direct store path", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });

        // Verify direct store path access works (used in Terminal.tsx for SolidJS reactivity)
        expect(store.state.terminals[id]?.fontSize).toBe(14);

        store.setFontSize(id, 18);
        expect(store.state.terminals[id]?.fontSize).toBe(18);

        store.setFontSize(id, 22);
        expect(store.state.terminals[id]?.fontSize).toBe(22);

        // Verify it returns undefined for non-existent terminal
        expect(store.state.terminals["nonexistent"]?.fontSize).toBeUndefined();

        dispose();
      });
    });
  });

  describe("awaiting input", () => {
    it("sets awaiting input type", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id, "question");
        expect(store.get(id)!.awaitingInput).toBe("question");
        dispose();
      });
    });

    it("clears awaiting input", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: "question" });
        store.clearAwaitingInput(id);
        expect(store.get(id)!.awaitingInput).toBeNull();
        dispose();
      });
    });

    it("hasAwaitingInput returns true when any terminal is awaiting", () => {
      createRoot((dispose) => {
        store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id2, "error");
        expect(store.hasAwaitingInput()).toBe(true);
        dispose();
      });
    });

    it("hasAwaitingInput returns false when none awaiting", () => {
      createRoot((dispose) => {
        store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.hasAwaitingInput()).toBe(false);
        dispose();
      });
    });

    it("shellState update does not clear awaitingInput (PTY output must not erase question state)", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id, "question");
        // Simulate PTY output: shellState goes busy
        store.update(id, { shellState: "busy" });
        // awaitingInput must survive — the question is still pending
        expect(store.get(id)!.awaitingInput).toBe("question");
        // And going back to idle also must not clear it
        store.update(id, { shellState: "idle" });
        expect(store.get(id)!.awaitingInput).toBe("question");
        dispose();
      });
    });

    it("getAwaitingInputIds returns correct IDs", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id2, "error");
        const ids = store.getAwaitingInputIds();
        expect(ids).toContain(id2);
        expect(ids).not.toContain(id1);
        dispose();
      });
    });
  });

  describe("getIds()", () => {
    it("returns all terminal IDs", () => {
      createRoot((dispose) => {
        store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        expect(store.getIds()).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("sessionToTerminal reverse map", () => {
    it("getTerminalForSession returns terminal ID when session is assigned", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: "sess-abc", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getTerminalForSession("sess-abc")).toBe(id);
        dispose();
      });
    });

    it("getTerminalForSession returns null for unknown session", () => {
      createRoot((dispose) => {
        store.add({ sessionId: "sess-abc", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getTerminalForSession("not-a-session")).toBeNull();
        dispose();
      });
    });

    it("getTerminalForSession returns null for terminal with null sessionId", () => {
      createRoot((dispose) => {
        store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getTerminalForSession("sess-xyz")).toBeNull();
        dispose();
      });
    });

    it("map is updated when setSessionId is called", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getTerminalForSession("sess-new")).toBeNull();
        store.setSessionId(id, "sess-new");
        expect(store.getTerminalForSession("sess-new")).toBe(id);
        dispose();
      });
    });

    it("map is updated when setSessionId reassigns to a new session", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: "sess-old", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        store.setSessionId(id, "sess-new");
        expect(store.getTerminalForSession("sess-old")).toBeNull();
        expect(store.getTerminalForSession("sess-new")).toBe(id);
        dispose();
      });
    });

    it("map entry is removed when terminal is removed", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: "sess-abc", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getTerminalForSession("sess-abc")).toBe(id);
        store.remove(id);
        expect(store.getTerminalForSession("sess-abc")).toBeNull();
        dispose();
      });
    });

    it("getAgentTypeForSession uses reverse map for O(1) lookup", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: "sess-agent", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        store.update(id, { agentType: "claude" });
        expect(store.getAgentTypeForSession("sess-agent")).toBe("claude");
        dispose();
      });
    });

    it("getAgentTypeForSession returns null for unknown session", () => {
      createRoot((dispose) => {
        store.add({ sessionId: "sess-agent", fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getAgentTypeForSession("unknown-sess")).toBeNull();
        dispose();
      });
    });
  });

  describe("agentType", () => {
    it("initializes agentType as null", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        expect(store.get(id)!.agentType).toBeNull();
        dispose();
      });
    });

    it("can be updated to a known agent", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { agentType: "claude" });
        expect(store.get(id)!.agentType).toBe("claude");
        dispose();
      });
    });

    it("can be cleared back to null", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { agentType: "gemini" });
        store.update(id, { agentType: null });
        expect(store.get(id)!.agentType).toBeNull();
        dispose();
      });
    });
  });

  describe("unseen", () => {
    it("initializes unseen as false", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        expect(store.get(id)!.unseen).toBe(false);
        dispose();
      });
    });

    it("clears unseen when setting active", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { unseen: true });
        store.setActive(id);
        expect(store.get(id)!.unseen).toBe(false);
        dispose();
      });
    });

    it("setAwaitingInput does NOT set unseen (question dot is sufficient)", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setActive(id1);
        // Terminal id2 is not active — awaitingInput should NOT set unseen
        // (the orange/red dot already communicates "needs attention")
        store.setAwaitingInput(id2, "question");
        expect(store.get(id2)!.unseen).toBe(false);
        dispose();
      });
    });
  });

  describe("shellState", () => {
    it("initializes shellState as null", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        expect(store.get(id)!.shellState).toBeNull();
        dispose();
      });
    });

    it("can be updated to busy", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.shellState).toBe("busy");
        dispose();
      });
    });

    it("can be updated to idle", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { shellState: "idle" });
        expect(store.get(id)!.shellState).toBe("idle");
        dispose();
      });
    });

    it("clears awaitingInput on idle→busy transition", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { shellState: "idle" });
        store.setAwaitingInput(id, "question");
        expect(store.get(id)!.awaitingInput).toBe("question");
        // Agent resumes output → idle→busy transition should clear stale question
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.awaitingInput).toBeNull();
        dispose();
      });
    });

    it("does not clear awaitingInput on null→busy transition", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id, "question");
        // Initial busy (from null) should NOT clear — agent hasn't been idle yet
        store.update(id, { shellState: "busy" });
        expect(store.get(id)!.awaitingInput).toBe("question");
        dispose();
      });
    });

    it("preserves shellState on setActive", () => {
      createRoot((dispose) => {
        const id = store.add({ sessionId: null, fontSize: 14, name: "Test", cwd: null, awaitingInput: null });
        store.update(id, { shellState: "idle" });
        store.setActive(id);
        expect(store.get(id)!.shellState).toBe("idle");
        dispose();
      });
    });
  });

  describe("getCount()", () => {
    it("returns terminal count", () => {
      createRoot((dispose) => {
        expect(store.getCount()).toBe(0);
        store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        expect(store.getCount()).toBe(1);
        dispose();
      });
    });
  });

  describe("TabLayout", () => {
    describe("default layout", () => {
      it("initializes with direction none and empty panes", () => {
        createRoot((dispose) => {
          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });
    });

    describe("splitPane()", () => {
      it("splits a single pane vertically", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: "/tmp", awaitingInput: null });
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
          dispose();
        });
      });

      it("splits a single pane horizontally", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const newId = store.splitPane("horizontal");

          expect(store.state.layout.direction).toBe("horizontal");
          expect(store.state.layout.panes).toHaveLength(2);
          expect(store.state.layout.panes[1]).toBe(newId);
          expect(store.state.layout.ratios).toEqual([0.5, 0.5]);
          dispose();
        });
      });

      it("adds 3rd pane when same direction active", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
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
          dispose();
        });
      });

      it("returns null when opposite direction requested", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });
          store.splitPane("vertical");

          const result = store.splitPane("horizontal");
          expect(result).toBeNull();
          expect(store.state.layout.panes).toHaveLength(2);
          dispose();
        });
      });

      it("returns null at MAX_SPLIT_PANES (6)", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
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
          dispose();
        });
      });

      it("returns null if no panes in layout", () => {
        createRoot((dispose) => {
          const result = store.splitPane("vertical");
          expect(result).toBeNull();
          dispose();
        });
      });

      it("inherits cwd from the source pane", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: "/projects/foo", awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          const newId = store.splitPane("vertical");

          expect(newId).toBeDefined();
          expect(store.get(newId!)!.cwd).toBe("/projects/foo");
          dispose();
        });
      });
    });

    describe("closeSplitPane()", () => {
      it("collapses split to single pane when closing pane 1", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 1 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });

      it("collapses split to single pane when closing pane 0", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "horizontal", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.closeSplitPane(0);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id2]);
          expect(store.state.layout.ratios).toEqual([]);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });

      it("closes middle pane of 3, redistributes ratios proportionally", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          const id3 = store.add({ sessionId: null, fontSize: 14, name: "T3", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [0.4, 0.2, 0.4], activePaneIndex: 1 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toEqual([id1, id3]);
          expect(store.state.layout.ratios[0]).toBeCloseTo(0.5);
          expect(store.state.layout.ratios[1]).toBeCloseTo(0.5);
          // activePaneIndex should be clamped
          expect(store.state.layout.activePaneIndex).toBe(1);
          dispose();
        });
      });

      it("decrements activePaneIndex when a pane before the active one is closed", () => {
        createRoot((dispose) => {
          // 4 panes [A,B,C,D], activePaneIndex: 2 (C), close pane 0 (A)
          // Result: [B,C,D], activePaneIndex should be 1 (still pointing at C)
          const idA = store.add({ sessionId: null, fontSize: 14, name: "A", cwd: null, awaitingInput: null });
          const idB = store.add({ sessionId: null, fontSize: 14, name: "B", cwd: null, awaitingInput: null });
          const idC = store.add({ sessionId: null, fontSize: 14, name: "C", cwd: null, awaitingInput: null });
          const idD = store.add({ sessionId: null, fontSize: 14, name: "D", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [idA, idB, idC, idD], ratios: [0.25, 0.25, 0.25, 0.25], activePaneIndex: 2 });

          store.closeSplitPane(0);

          expect(store.state.layout.panes).toEqual([idB, idC, idD]);
          expect(store.state.layout.activePaneIndex).toBe(1); // still points at C
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
          dispose();
        });
      });

      it("closes last pane of 3, activePaneIndex decrements", () => {
        createRoot((dispose) => {
          // 3 panes, activePaneIndex: 2, close index 2
          // Result: 2 panes, activePaneIndex should be 1
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          const id3 = store.add({ sessionId: null, fontSize: 14, name: "T3", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [0.4, 0.3, 0.3], activePaneIndex: 2 });

          store.closeSplitPane(2);

          expect(store.state.layout.panes).toEqual([id1, id2]);
          expect(store.state.layout.activePaneIndex).toBe(1);
          expect(store.state.layout.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1.0);
          dispose();
        });
      });

      it("does nothing if not split", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratios: [], activePaneIndex: 0 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
          dispose();
        });
      });
    });

    describe("setHandleRatio()", () => {
      it("adjusts boundary between two adjacent panes", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setHandleRatio(0, 0.7);
          expect(store.state.layout.ratios[0]).toBeCloseTo(0.7);
          expect(store.state.layout.ratios[1]).toBeCloseTo(0.3);
          dispose();
        });
      });

      it("enforces minimum pane fraction", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          // Try to push boundary to 0.99 — should be clamped so second pane >= MIN_PANE_FRACTION
          store.setHandleRatio(0, 0.99);
          expect(store.state.layout.ratios[1]).toBeGreaterThanOrEqual(0.05);
          dispose();
        });
      });
    });

    describe("setActivePaneIndex()", () => {
      it("sets the active pane index", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setActivePaneIndex(1);
          expect(store.state.layout.activePaneIndex).toBe(1);
          dispose();
        });
      });

      it("sets back to 0", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratios: [0.5, 0.5], activePaneIndex: 0 });

          store.setActivePaneIndex(1);
          store.setActivePaneIndex(0);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });

      it("accepts any valid index", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          const id3 = store.add({ sessionId: null, fontSize: 14, name: "T3", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2, id3], ratios: [1/3, 1/3, 1/3], activePaneIndex: 0 });

          store.setActivePaneIndex(2);
          expect(store.state.layout.activePaneIndex).toBe(2);

          // Clamps to valid range
          store.setActivePaneIndex(10);
          expect(store.state.layout.activePaneIndex).toBe(2);
          dispose();
        });
      });
    });

    describe("setLayout()", () => {
      it("sets the complete layout", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });

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
          dispose();
        });
      });
    });
  });
});
