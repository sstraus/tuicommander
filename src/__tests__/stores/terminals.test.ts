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

    it("selects another terminal when active is removed", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setActive(id1);
        store.remove(id1);
        expect(store.state.activeId).toBe(id2);
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

    it("getAwaitingInputIds returns correct IDs", () => {
      createRoot((dispose) => {
        const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
        const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
        store.setAwaitingInput(id2, "confirmation");
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
          expect(store.state.layout.ratio).toBe(0.5);
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
          store.setLayout({ direction: "none", panes: [id1], ratio: 0.5, activePaneIndex: 0 });

          const newId = store.splitPane("vertical");

          expect(newId).toBeDefined();
          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toHaveLength(2);
          expect(store.state.layout.panes[0]).toBe(id1);
          expect(store.state.layout.panes[1]).toBe(newId);
          expect(store.state.layout.activePaneIndex).toBe(1);
          dispose();
        });
      });

      it("splits a single pane horizontally", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratio: 0.5, activePaneIndex: 0 });

          const newId = store.splitPane("horizontal");

          expect(store.state.layout.direction).toBe("horizontal");
          expect(store.state.layout.panes).toHaveLength(2);
          expect(store.state.layout.panes[1]).toBe(newId);
          dispose();
        });
      });

      it("returns null if already split", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratio: 0.5, activePaneIndex: 0 });

          const result = store.splitPane("vertical");

          expect(result).toBeNull();
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
          store.setLayout({ direction: "none", panes: [id1], ratio: 0.5, activePaneIndex: 0 });

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
          store.setLayout({ direction: "vertical", panes: [id1, id2], ratio: 0.5, activePaneIndex: 1 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });

      it("collapses split to single pane when closing pane 0", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          const id2 = store.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "horizontal", panes: [id1, id2], ratio: 0.5, activePaneIndex: 0 });

          store.closeSplitPane(0);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id2]);
          expect(store.state.layout.activePaneIndex).toBe(0);
          dispose();
        });
      });

      it("does nothing if not split", () => {
        createRoot((dispose) => {
          const id1 = store.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
          store.setLayout({ direction: "none", panes: [id1], ratio: 0.5, activePaneIndex: 0 });

          store.closeSplitPane(1);

          expect(store.state.layout.direction).toBe("none");
          expect(store.state.layout.panes).toEqual([id1]);
          dispose();
        });
      });
    });

    describe("setSplitRatio()", () => {
      it("sets the split ratio", () => {
        createRoot((dispose) => {
          store.setSplitRatio(0.7);
          expect(store.state.layout.ratio).toBe(0.7);
          dispose();
        });
      });

      it("clamps ratio to minimum 0.2", () => {
        createRoot((dispose) => {
          store.setSplitRatio(0.1);
          expect(store.state.layout.ratio).toBe(0.2);
          dispose();
        });
      });

      it("clamps ratio to maximum 0.8", () => {
        createRoot((dispose) => {
          store.setSplitRatio(0.95);
          expect(store.state.layout.ratio).toBe(0.8);
          dispose();
        });
      });
    });

    describe("setActivePaneIndex()", () => {
      it("sets the active pane index", () => {
        createRoot((dispose) => {
          store.setActivePaneIndex(1);
          expect(store.state.layout.activePaneIndex).toBe(1);
          dispose();
        });
      });

      it("sets back to 0", () => {
        createRoot((dispose) => {
          store.setActivePaneIndex(1);
          store.setActivePaneIndex(0);
          expect(store.state.layout.activePaneIndex).toBe(0);
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
            ratio: 0.6,
            activePaneIndex: 1,
          });

          expect(store.state.layout.direction).toBe("vertical");
          expect(store.state.layout.panes).toEqual([id1, id2]);
          expect(store.state.layout.ratio).toBe(0.6);
          expect(store.state.layout.activePaneIndex).toBe(1);
          dispose();
        });
      });
    });
  });
});
