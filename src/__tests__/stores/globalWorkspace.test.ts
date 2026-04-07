import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
}));

import { testInScope, makeTerminal } from "../helpers/store";
import type { PaneLayoutState, PaneLeaf, PaneBranch } from "../../stores/paneLayout";

describe("globalWorkspaceStore", () => {
  let store: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;
  let paneLayoutStore: typeof import("../../stores/paneLayout").paneLayoutStore;
  let savedPaneLayouts: typeof import("../../stores/savedPaneLayouts").savedPaneLayouts;
  let resetGroupCounter: typeof import("../../stores/paneLayout").resetGroupCounter;
  let terminalsStore: typeof import("../../stores/terminals").terminalsStore;

  beforeEach(async () => {
    vi.resetModules();
    store = (await import("../../stores/globalWorkspace")).globalWorkspaceStore;
    const paneLayout = await import("../../stores/paneLayout");
    paneLayoutStore = paneLayout.paneLayoutStore;
    resetGroupCounter = paneLayout.resetGroupCounter;
    savedPaneLayouts = (await import("../../stores/savedPaneLayouts")).savedPaneLayouts;
    terminalsStore = (await import("../../stores/terminals")).terminalsStore;
    resetGroupCounter();
    savedPaneLayouts.clear();
  });

  describe("isActive", () => {
    it("starts inactive", () => {
      testInScope(() => {
        expect(store.isActive()).toBe(false);
      });
    });

    it("toggles on activate/deactivate", () => {
      testInScope(() => {
        store.activate();
        expect(store.isActive()).toBe(true);
        store.deactivate();
        expect(store.isActive()).toBe(false);
      });
    });
  });

  describe("promote / unpromote", () => {
    it("adds terminal to promoted set", () => {
      testInScope(() => {
        store.promote("t1");
        expect(store.isPromoted("t1")).toBe(true);
        expect(store.getPromotedIds()).toEqual(["t1"]);
      });
    });

    it("removes terminal from promoted set", () => {
      testInScope(() => {
        store.promote("t1");
        store.unpromote("t1");
        expect(store.isPromoted("t1")).toBe(false);
        expect(store.getPromotedIds()).toEqual([]);
      });
    });

    it("promote is idempotent", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t1");
        expect(store.getPromotedIds()).toEqual(["t1"]);
      });
    });

    it("unpromote non-existent terminal is a no-op", () => {
      testInScope(() => {
        store.unpromote("t999");
        expect(store.getPromotedIds()).toEqual([]);
      });
    });

    it("tracks multiple terminals", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        store.promote("t3");
        expect(store.getPromotedIds().sort()).toEqual(["t1", "t2", "t3"]);
        store.unpromote("t2");
        expect(store.getPromotedIds().sort()).toEqual(["t1", "t3"]);
      });
    });
  });

  describe("hasPromoted", () => {
    it("returns false when empty", () => {
      testInScope(() => {
        expect(store.hasPromoted()).toBe(false);
      });
    });

    it("returns true when terminals are promoted", () => {
      testInScope(() => {
        store.promote("t1");
        expect(store.hasPromoted()).toBe(true);
      });
    });

    it("returns false after all terminals unpromoted", () => {
      testInScope(() => {
        store.promote("t1");
        store.unpromote("t1");
        expect(store.hasPromoted()).toBe(false);
      });
    });
  });

  describe("onTerminalRemoved", () => {
    it("removes terminal from promoted set", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        store.onTerminalRemoved("t1");
        expect(store.isPromoted("t1")).toBe(false);
        expect(store.getPromotedIds()).toEqual(["t2"]);
      });
    });

    it("is a no-op for non-promoted terminal", () => {
      testInScope(() => {
        store.promote("t1");
        store.onTerminalRemoved("t999");
        expect(store.getPromotedIds()).toEqual(["t1"]);
      });
    });

    it("auto-deactivates when last promoted terminal removed while active", () => {
      testInScope(() => {
        store.promote("t1");
        store.activate();
        expect(store.isActive()).toBe(true);
        store.onTerminalRemoved("t1");
        expect(store.isActive()).toBe(false);
      });
    });

    it("does not deactivate if promoted terminals remain", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        store.activate();
        store.onTerminalRemoved("t1");
        expect(store.isActive()).toBe(true);
        expect(store.getPromotedIds()).toEqual(["t2"]);
      });
    });
  });

  describe("layout", () => {
    it("starts with null layout", () => {
      testInScope(() => {
        expect(store.getLayout()).toBeNull();
      });
    });

    it("setLayout / getLayout round-trip", () => {
      testInScope(() => {
        const layout = {
          root: { type: "leaf" as const, id: "g1" },
          groups: { g1: { id: "g1", tabs: [{ id: "t1", type: "terminal" as const }], activeTabId: "t1" } },
          activeGroupId: "g1",
        };
        store.setLayout(layout);
        expect(store.getLayout()).toEqual(layout);
      });
    });

    it("setLayout with null clears layout", () => {
      testInScope(() => {
        store.setLayout({
          root: { type: "leaf" as const, id: "g1" },
          groups: { g1: { id: "g1", tabs: [], activeTabId: null } },
          activeGroupId: "g1",
        });
        store.setLayout(null);
        expect(store.getLayout()).toBeNull();
      });
    });
  });

  describe("workspace switch — save/restore orchestration", () => {
    const repoKey = "/repo\0main";

    function setupRepoLayout(): PaneLayoutState {
      // Set up a split layout in paneLayoutStore simulating a repo view
      const gid1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(gid1, { id: "t1", type: "terminal" });
      const gid2 = paneLayoutStore.split(gid1, "vertical")!;
      paneLayoutStore.addTab(gid2, { id: "t2", type: "terminal" });
      return paneLayoutStore.serialize();
    }

    it("activate saves current repo layout and restores global layout", () => {
      testInScope(() => {
        const repoLayout = setupRepoLayout();

        // Set a global layout
        const globalLayout: PaneLayoutState = {
          root: { type: "leaf", id: "g10" },
          groups: { g10: { id: "g10", tabs: [{ id: "t5", type: "terminal" }], activeTabId: "t5" } },
          activeGroupId: "g10",
        };
        store.setLayout(globalLayout);
        store.promote("t5");

        // Activate global workspace
        store.activate(repoKey);

        // Repo layout should be saved
        expect(savedPaneLayouts.get(repoKey)).toEqual(repoLayout);

        // paneLayoutStore should now have the global layout
        const currentLayout = paneLayoutStore.serialize();
        expect(currentLayout.groups.g10).toBeDefined();
        expect(store.isActive()).toBe(true);
      });
    });

    it("deactivate saves global layout and restores repo layout", () => {
      testInScope(() => {
        const repoLayout = setupRepoLayout();
        store.setLayout({
          root: { type: "leaf", id: "g10" },
          groups: { g10: { id: "g10", tabs: [{ id: "t5", type: "terminal" }], activeTabId: "t5" } },
          activeGroupId: "g10",
        });
        store.promote("t5");

        store.activate(repoKey);
        store.deactivate(repoKey);

        // Repo layout should be restored in paneLayoutStore
        const restored = paneLayoutStore.serialize();
        expect(restored.root).toEqual(repoLayout.root);
        expect(store.isActive()).toBe(false);

        // Global layout should be saved back
        expect(store.getLayout()).not.toBeNull();
      });
    });

    it("round-trip: repo → global → repo preserves both layouts", () => {
      testInScope(() => {
        setupRepoLayout();
        const originalRepoLayout = paneLayoutStore.serialize();

        const globalLayout: PaneLayoutState = {
          root: { type: "leaf", id: "g10" },
          groups: { g10: { id: "g10", tabs: [{ id: "t5", type: "terminal" }], activeTabId: "t5" } },
          activeGroupId: "g10",
        };
        store.setLayout(globalLayout);
        store.promote("t5");

        store.activate(repoKey);
        store.deactivate(repoKey);

        const restoredRepoLayout = paneLayoutStore.serialize();
        expect(restoredRepoLayout.root).toEqual(originalRepoLayout.root);
        expect(Object.keys(restoredRepoLayout.groups).length).toBe(Object.keys(originalRepoLayout.groups).length);
      });
    });

    it("activate with empty global layout resets paneLayoutStore", () => {
      testInScope(() => {
        setupRepoLayout();
        store.promote("t1"); // need at least one promoted

        store.activate(repoKey);

        // No global layout was set, so paneLayoutStore should be reset
        expect(paneLayoutStore.isSplit()).toBe(false);
        expect(store.isActive()).toBe(true);
      });
    });

    it("activate with no repoKey saves nothing", () => {
      testInScope(() => {
        setupRepoLayout();
        store.promote("t1");

        store.activate();

        // No repoKey means no saving of repo layout
        expect(savedPaneLayouts.size).toBe(0);
        expect(store.isActive()).toBe(true);
      });
    });

    it("deactivate with no repoKey restores nothing (just resets)", () => {
      testInScope(() => {
        store.promote("t1");
        store.activate();

        store.deactivate();

        expect(store.isActive()).toBe(false);
      });
    });
  });

  describe("auto-layout on promote/unpromote", () => {
    it("first promote creates single leaf with terminal tab", () => {
      testInScope(() => {
        store.promote("t1");
        const layout = store.getLayout();
        expect(layout).not.toBeNull();
        expect(layout!.root!.type).toBe("leaf");
        // The group should contain a terminal tab for t1
        const groupId = (layout!.root! as PaneLeaf).id;
        const group = layout!.groups[groupId];
        expect(group.tabs).toEqual([{ id: "t1", type: "terminal" }]);
      });
    });

    it("second promote splits horizontally", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        const layout = store.getLayout();
        expect(layout!.root!.type).toBe("branch");
        const root = layout!.root! as PaneBranch;
        expect(root.direction).toBe("horizontal");
        expect(root.children).toHaveLength(2);
        // Both should be leaves with terminal tabs
        const g1 = layout!.groups[(root.children[0] as PaneLeaf).id];
        const g2 = layout!.groups[(root.children[1] as PaneLeaf).id];
        expect(g1.tabs[0].id).toBe("t1");
        expect(g2.tabs[0].id).toBe("t2");
      });
    });

    it("third promote splits again within depth limit", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        store.promote("t3");
        const layout = store.getLayout();
        // Should have 3 terminal tabs across groups
        const termIds = Object.values(layout!.groups).flatMap(g => g.tabs.map(t => t.id)).sort();
        expect(termIds).toEqual(["t1", "t2", "t3"]);
      });
    });

    it("promote beyond MAX_SPLIT_DEPTH returns false", () => {
      testInScope(() => {
        // MAX_SPLIT_DEPTH is 3, so we can have up to 2^3 = 8 leaves
        // Actually, depth 3 means: leaf at depth 0, 1 split = depth 1, 2 splits = depth 2
        // Let's fill it up and check
        store.promote("t1");
        store.promote("t2");
        store.promote("t3");
        store.promote("t4");
        // At some point it should be denied
        // With MAX_SPLIT_DEPTH=3, splitting at depth 2 is the last allowed
        // Let's just verify the method returns false when full
        const results: boolean[] = [];
        for (let i = 5; i <= 20; i++) {
          results.push(store.promote(`t${i}`));
        }
        // At least some should be false (denied)
        expect(results).toContain(false);
      });
    });

    it("unpromote removes terminal from layout and flattens tree", () => {
      testInScope(() => {
        store.promote("t1");
        store.promote("t2");
        store.unpromote("t1");
        const layout = store.getLayout();
        expect(layout).not.toBeNull();
        // Should be back to single leaf with t2
        expect(layout!.root!.type).toBe("leaf");
        const group = layout!.groups[(layout!.root! as PaneLeaf).id];
        expect(group.tabs[0].id).toBe("t2");
      });
    });

    it("unpromote last terminal sets layout to null", () => {
      testInScope(() => {
        store.promote("t1");
        store.unpromote("t1");
        expect(store.getLayout()).toBeNull();
      });
    });

    it("promote while global workspace active updates paneLayoutStore live", () => {
      testInScope(() => {
        store.promote("t1");
        store.activate();
        // paneLayoutStore should have the t1 layout
        const beforeGroups = Object.values(paneLayoutStore.serialize().groups);
        expect(beforeGroups.some(g => g.tabs.some(t => t.id === "t1"))).toBe(true);

        store.promote("t2");
        // paneLayoutStore should now also have t2
        const afterGroups = Object.values(paneLayoutStore.serialize().groups);
        expect(afterGroups.some(g => g.tabs.some(t => t.id === "t2"))).toBe(true);
      });
    });

    it("promote while in repo view only updates background layout", () => {
      testInScope(() => {
        // Set up repo layout in paneLayoutStore
        const gid = paneLayoutStore.createGroup();
        paneLayoutStore.addTab(gid, { id: "repo-t1", type: "terminal" });

        store.promote("t1");

        // paneLayoutStore should still have repo layout
        const groups = Object.values(paneLayoutStore.serialize().groups);
        expect(groups.some(g => g.tabs.some(t => t.id === "repo-t1"))).toBe(true);
        // Global layout should have t1
        const globalLayout = store.getLayout();
        expect(globalLayout).not.toBeNull();
      });
    });
  });

  describe("terminal lifecycle integration", () => {
    it("terminalsStore.remove() auto-unpromotes the terminal", () => {
      testInScope(() => {
        const id = terminalsStore.add(makeTerminal({ name: "agent" }));
        store.promote(id);
        expect(store.isPromoted(id)).toBe(true);

        terminalsStore.remove(id);
        expect(store.isPromoted(id)).toBe(false);
        expect(store.getPromotedIds()).toEqual([]);
      });
    });

    it("removing last promoted terminal while active auto-deactivates", () => {
      testInScope(() => {
        const id = terminalsStore.add(makeTerminal({ name: "agent" }));
        store.promote(id);
        store.activate();
        expect(store.isActive()).toBe(true);

        terminalsStore.remove(id);
        expect(store.isActive()).toBe(false);
      });
    });

    it("removing non-promoted terminal has no effect on global workspace", () => {
      testInScope(() => {
        const id1 = terminalsStore.add(makeTerminal({ name: "agent1" }));
        const id2 = terminalsStore.add(makeTerminal({ name: "agent2" }));
        store.promote(id1);

        terminalsStore.remove(id2);
        expect(store.isPromoted(id1)).toBe(true);
        expect(store.getPromotedIds()).toEqual([id1]);
      });
    });
  });
});
