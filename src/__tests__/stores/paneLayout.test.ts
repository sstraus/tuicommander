import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
}));
import {
  splitLeaf,
  removeLeaf,
  findLeaf,
  findParent,
  allLeafIds,
  findAdjacentLeaf,
  nodeDepth,
  leafDepthFromRoot,
  normalizeRatios,
  setHandleRatio,
  MAX_SPLIT_DEPTH,
  MIN_PANE_RATIO,
  type PaneNode,
  type PaneBranch,
  type PaneLeaf,
} from "../../stores/paneLayout";
import { testInScope } from "../helpers/store";

describe("paneLayout tree utilities", () => {
  describe("nodeDepth", () => {
    it("leaf has depth 0", () => {
      expect(nodeDepth({ type: "leaf", id: "g1" })).toBe(0);
    });

    it("branch with leaves has depth 1", () => {
      const branch: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(nodeDepth(branch)).toBe(1);
    });

    it("nested branches accumulate depth", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          {
            type: "branch",
            direction: "horizontal",
            children: [{ type: "leaf", id: "g2" }, { type: "leaf", id: "g3" }],
            ratios: [0.5, 0.5],
          },
        ],
        ratios: [0.5, 0.5],
      };
      expect(nodeDepth(tree)).toBe(2);
    });
  });

  describe("leafDepthFromRoot", () => {
    it("root leaf has depth 0", () => {
      const leaf: PaneLeaf = { type: "leaf", id: "g1" };
      expect(leafDepthFromRoot(leaf, "g1")).toBe(0);
    });

    it("child of root branch has depth 1", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(leafDepthFromRoot(tree, "g2")).toBe(1);
    });

    it("returns -1 for missing leaf", () => {
      const leaf: PaneLeaf = { type: "leaf", id: "g1" };
      expect(leafDepthFromRoot(leaf, "g999")).toBe(-1);
    });
  });

  describe("findLeaf", () => {
    it("finds leaf in flat tree", () => {
      const leaf: PaneLeaf = { type: "leaf", id: "g1" };
      expect(findLeaf(leaf, "g1")).toEqual(leaf);
    });

    it("finds leaf in nested tree", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          {
            type: "branch",
            direction: "horizontal",
            children: [{ type: "leaf", id: "g2" }, { type: "leaf", id: "g3" }],
            ratios: [0.5, 0.5],
          },
        ],
        ratios: [0.5, 0.5],
      };
      expect(findLeaf(tree, "g3")).toEqual({ type: "leaf", id: "g3" });
    });

    it("returns null for missing leaf", () => {
      const leaf: PaneLeaf = { type: "leaf", id: "g1" };
      expect(findLeaf(leaf, "g999")).toBeNull();
    });
  });

  describe("findParent", () => {
    it("returns null for root leaf", () => {
      expect(findParent({ type: "leaf", id: "g1" }, "g1")).toBeNull();
    });

    it("finds parent of direct child", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = findParent(tree, "g2");
      expect(result).not.toBeNull();
      expect(result!.parent).toBe(tree);
      expect(result!.index).toBe(1);
    });
  });

  describe("allLeafIds", () => {
    it("returns single ID for leaf", () => {
      expect(allLeafIds({ type: "leaf", id: "g1" })).toEqual(["g1"]);
    });

    it("returns all IDs in DFS order", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          {
            type: "branch",
            direction: "horizontal",
            children: [{ type: "leaf", id: "g2" }, { type: "leaf", id: "g3" }],
            ratios: [0.5, 0.5],
          },
        ],
        ratios: [0.5, 0.5],
      };
      expect(allLeafIds(tree)).toEqual(["g1", "g2", "g3"]);
    });
  });

  describe("normalizeRatios", () => {
    it("normalizes to sum 1.0", () => {
      const result = normalizeRatios([1, 3]);
      expect(result[0]).toBeCloseTo(0.25);
      expect(result[1]).toBeCloseTo(0.75);
    });

    it("handles all zeros", () => {
      const result = normalizeRatios([0, 0, 0]);
      expect(result).toEqual([1 / 3, 1 / 3, 1 / 3]);
    });

    it("leaves already-normalized ratios unchanged", () => {
      const result = normalizeRatios([0.5, 0.5]);
      expect(result[0]).toBeCloseTo(0.5);
      expect(result[1]).toBeCloseTo(0.5);
    });
  });

  describe("splitLeaf", () => {
    it("splits a root leaf into a branch", () => {
      const root: PaneLeaf = { type: "leaf", id: "g1" };
      const result = splitLeaf(root, "g1", "vertical", "g2");
      expect(result).not.toBeNull();
      expect(result!.type).toBe("branch");
      const branch = result as PaneBranch;
      expect(branch.direction).toBe("vertical");
      expect(branch.children).toHaveLength(2);
      expect(branch.ratios).toEqual([0.5, 0.5]);
      expect((branch.children[0] as PaneLeaf).id).toBe("g1");
      expect((branch.children[1] as PaneLeaf).id).toBe("g2");
    });

    it("splits a nested leaf", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = splitLeaf(tree, "g2", "horizontal", "g3");
      expect(result).not.toBeNull();
      const root = result as PaneBranch;
      expect(root.children[1].type).toBe("branch");
      const inner = root.children[1] as PaneBranch;
      expect(inner.direction).toBe("horizontal");
      expect(allLeafIds(root)).toEqual(["g1", "g2", "g3"]);
    });

    it("returns null when leaf not found", () => {
      const root: PaneLeaf = { type: "leaf", id: "g1" };
      expect(splitLeaf(root, "g999", "vertical", "g2")).toBeNull();
    });

    it("returns null when max depth would be exceeded", () => {
      // Build a tree at max depth - 1
      let tree: PaneNode = { type: "leaf", id: "g1" };
      for (let i = 0; i < MAX_SPLIT_DEPTH - 1; i++) {
        const newId = `g${i + 2}`;
        // Find the deepest leaf to split
        const deepest = allLeafIds(tree).pop()!;
        const result = splitLeaf(tree, deepest, "vertical", newId);
        expect(result).not.toBeNull();
        tree = result!;
      }
      // Now trying to split a deep leaf should fail
      const deepestLeaf = allLeafIds(tree).pop()!;
      expect(splitLeaf(tree, deepestLeaf, "vertical", "gN")).toBeNull();
    });
  });

  describe("removeLeaf", () => {
    it("returns null when removing root leaf", () => {
      expect(removeLeaf({ type: "leaf", id: "g1" }, "g1")).toBeNull();
    });

    it("flattens branch to remaining leaf when one child removed", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = removeLeaf(tree, "g1");
      expect(result).toEqual({ type: "leaf", id: "g2" });
    });

    it("keeps branch with 3+ children after removing one", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          { type: "leaf", id: "g2" },
          { type: "leaf", id: "g3" },
        ],
        ratios: [0.33, 0.33, 0.34],
      };
      const result = removeLeaf(tree, "g2") as PaneBranch;
      expect(result.type).toBe("branch");
      expect(result.children).toHaveLength(2);
      expect(allLeafIds(result)).toEqual(["g1", "g3"]);
      // Ratios should be normalized
      expect(result.ratios[0] + result.ratios[1]).toBeCloseTo(1.0);
    });

    it("removes nested leaf and flattens", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          {
            type: "branch",
            direction: "horizontal",
            children: [{ type: "leaf", id: "g2" }, { type: "leaf", id: "g3" }],
            ratios: [0.5, 0.5],
          },
        ],
        ratios: [0.5, 0.5],
      };
      // Remove g2: inner branch collapses to g3, outer branch becomes g1 + g3
      const result = removeLeaf(tree, "g2") as PaneBranch;
      expect(allLeafIds(result)).toEqual(["g1", "g3"]);
    });

    it("does not modify tree when leaf not found", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = removeLeaf(tree, "g999");
      expect(result).toBe(tree); // same reference = unchanged
    });
  });

  describe("setHandleRatio", () => {
    it("adjusts ratios at handle position", () => {
      const branch: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = setHandleRatio(branch, 0, 0.7);
      expect(result.ratios[0]).toBeCloseTo(0.7);
      expect(result.ratios[1]).toBeCloseTo(0.3);
    });

    it("enforces minimum ratio", () => {
      const branch: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      const result = setHandleRatio(branch, 0, 0.99);
      expect(result.ratios[0]).toBeLessThanOrEqual(1.0 - MIN_PANE_RATIO);
      expect(result.ratios[1]).toBeGreaterThanOrEqual(MIN_PANE_RATIO);
    });

    it("returns same branch for invalid handle index", () => {
      const branch: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(setHandleRatio(branch, 5, 0.5)).toBe(branch);
      expect(setHandleRatio(branch, -1, 0.5)).toBe(branch);
    });
  });

  describe("findAdjacentLeaf", () => {
    it("finds right neighbor in vertical split", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(findAdjacentLeaf(tree, "g1", "right")).toBe("g2");
      expect(findAdjacentLeaf(tree, "g2", "left")).toBe("g1");
    });

    it("finds bottom neighbor in horizontal split", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "horizontal",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(findAdjacentLeaf(tree, "g1", "down")).toBe("g2");
      expect(findAdjacentLeaf(tree, "g2", "up")).toBe("g1");
    });

    it("returns null at boundary", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(findAdjacentLeaf(tree, "g1", "left")).toBeNull();
      expect(findAdjacentLeaf(tree, "g2", "right")).toBeNull();
    });

    it("returns null for perpendicular direction", () => {
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [{ type: "leaf", id: "g1" }, { type: "leaf", id: "g2" }],
        ratios: [0.5, 0.5],
      };
      expect(findAdjacentLeaf(tree, "g1", "up")).toBeNull();
      expect(findAdjacentLeaf(tree, "g1", "down")).toBeNull();
    });

    it("navigates across nested splits", () => {
      // Layout: g1 | (g2 / g3)
      const tree: PaneBranch = {
        type: "branch",
        direction: "vertical",
        children: [
          { type: "leaf", id: "g1" },
          {
            type: "branch",
            direction: "horizontal",
            children: [{ type: "leaf", id: "g2" }, { type: "leaf", id: "g3" }],
            ratios: [0.5, 0.5],
          },
        ],
        ratios: [0.5, 0.5],
      };
      // From g1, going right should reach g2 (first leaf of right subtree)
      expect(findAdjacentLeaf(tree, "g1", "right")).toBe("g2");
      // From g2, going left should reach g1
      expect(findAdjacentLeaf(tree, "g2", "left")).toBe("g1");
      // From g3, going left should reach g1
      expect(findAdjacentLeaf(tree, "g3", "left")).toBe("g1");
      // From g2, going down should reach g3
      expect(findAdjacentLeaf(tree, "g2", "down")).toBe("g3");
      // From g3, going up should reach g2
      expect(findAdjacentLeaf(tree, "g3", "up")).toBe("g2");
    });
  });
});

describe("paneLayoutStore", () => {
  let store: typeof import("../../stores/paneLayout").paneLayoutStore;

  beforeEach(async () => {
    // Re-import to get fresh store
    const mod = await import("../../stores/paneLayout");
    store = mod.paneLayoutStore;
    store.reset();
    mod.resetGroupCounter();
  });

  describe("createGroup", () => {
    it("creates an empty group with unique ID", () => {
      testInScope(() => {
        const id = store.createGroup();
        expect(id).toBe("g1");
        expect(store.state.groups[id]).toBeDefined();
        expect(store.state.groups[id].tabs).toEqual([]);
        expect(store.state.groups[id].activeTabId).toBeNull();
      });
    });

    it("increments IDs", () => {
      testInScope(() => {
        const id1 = store.createGroup();
        const id2 = store.createGroup();
        expect(id1).toBe("g1");
        expect(id2).toBe("g2");
      });
    });
  });

  describe("split", () => {
    it("creates initial tree from null root", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        store.setActiveGroup(g1);

        const newId = store.split(g1, "vertical");
        expect(newId).not.toBeNull();
        expect(store.getRoot()?.type).toBe("branch");
        const root = store.getRoot() as PaneBranch;
        expect(root.direction).toBe("vertical");
        expect(root.children).toHaveLength(2);
        expect(store.state.activeGroupId).toBe(newId);
      });
    });

    it("splits from null root (first split ever)", () => {
      testInScope(() => {
        const g1 = store.createGroup();

        const newId = store.split(g1, "horizontal");
        expect(newId).not.toBeNull();
        expect(store.getRoot()?.type).toBe("branch");
      });
    });

    it("rejects split beyond max depth", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        store.setActiveGroup(g1);

        // Split to depth 1
        const g2 = store.split(g1, "vertical")!;
        // Split g2 to depth 2
        const g3 = store.split(g2, "horizontal")!;
        // Split g3 should fail (depth 3 = MAX_SPLIT_DEPTH)
        const g4 = store.split(g3, "vertical");
        expect(g4).toBeNull();
      });
    });
  });

  describe("closePane", () => {
    it("removes a leaf and flattens", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        const g2 = store.split(g1, "vertical")!;

        store.closePane(g2);
        expect(store.getRoot()?.type).toBe("leaf");
        expect((store.getRoot() as PaneLeaf).id).toBe(g1);
        expect(store.state.groups[g2]).toBeUndefined();
      });
    });

    it("switches active group when closing active pane", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        const g2 = store.split(g1, "vertical")!;
        store.setActiveGroup(g2);

        store.closePane(g2);
        expect(store.state.activeGroupId).toBe(g1);
      });
    });
  });

  describe("addTab / removeTab / moveTab", () => {
    it("adds a tab to group", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.addTab(g1, { id: "term-1", type: "terminal" });

        expect(store.state.groups[g1].tabs).toHaveLength(1);
        expect(store.state.groups[g1].activeTabId).toBe("term-1");
      });
    });

    it("does not duplicate existing tab", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.addTab(g1, { id: "term-1", type: "terminal" });
        store.addTab(g1, { id: "term-1", type: "terminal" });

        expect(store.state.groups[g1].tabs).toHaveLength(1);
      });
    });

    it("removes tab and updates active", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.addTab(g1, { id: "term-1", type: "terminal" });
        store.addTab(g1, { id: "md-1", type: "markdown" });

        store.removeTab(g1, "md-1");
        expect(store.state.groups[g1].tabs).toHaveLength(1);
        expect(store.state.groups[g1].activeTabId).toBe("term-1");
      });
    });

    it("moves tab between groups", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        const g2 = store.createGroup();
        store.addTab(g1, { id: "term-1", type: "terminal" });
        store.addTab(g1, { id: "md-1", type: "markdown" });

        store.moveTab(g1, g2, "md-1");
        expect(store.state.groups[g1].tabs).toHaveLength(1);
        expect(store.state.groups[g2].tabs).toHaveLength(1);
        expect(store.state.groups[g2].activeTabId).toBe("md-1");
      });
    });
  });

  describe("navigatePane", () => {
    it("navigates between panes", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        store.setActiveGroup(g1);
        const g2 = store.split(g1, "vertical")!;
        store.setActiveGroup(g1);

        const target = store.navigatePane("right");
        expect(target).toBe(g2);
        expect(store.state.activeGroupId).toBe(g2);
      });
    });

    it("returns null at boundary", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        store.setActiveGroup(g1);
        store.split(g1, "vertical");
        store.setActiveGroup(g1);

        const target = store.navigatePane("left");
        expect(target).toBeNull();
        expect(store.state.activeGroupId).toBe(g1);
      });
    });
  });

  describe("serialize / restore", () => {
    it("round-trips the layout", () => {
      testInScope(() => {
        const g1 = store.createGroup();
        store.setRoot({ type: "leaf", id: g1 });
        store.setActiveGroup(g1);
        store.addTab(g1, { id: "term-1", type: "terminal" });
        const g2 = store.split(g1, "vertical")!;
        store.addTab(g2, { id: "md-1", type: "markdown" });

        const serialized = store.serialize();
        store.reset();
        expect(store.getRoot()).toBeNull();

        store.restore(serialized);
        expect(store.getRoot()?.type).toBe("branch");
        expect(store.state.groups[g1].tabs).toHaveLength(1);
        expect(store.state.groups[g2].tabs).toHaveLength(1);
        expect(store.state.activeGroupId).toBe(g2);
      });
    });
  });

  describe("loadFromDisk", () => {
    it("filters out non-terminal tabs on restore", async () => {
      const { invoke } = await import("../../invoke");
      const mockInvoke = vi.mocked(invoke);

      const savedLayout = {
        root: {
          type: "branch",
          direction: "vertical",
          children: [
            { type: "leaf", id: "g1" },
            { type: "leaf", id: "g2" },
          ],
          ratios: [0.5, 0.5],
        },
        groups: {
          g1: {
            id: "g1",
            tabs: [
              { id: "term-1", type: "terminal" },
              { id: "md-1", type: "markdown" },
              { id: "diff-1", type: "diff" },
            ],
            activeTabId: "md-1",
          },
          g2: {
            id: "g2",
            tabs: [
              { id: "editor-1", type: "editor" },
            ],
            activeTabId: "editor-1",
          },
        },
        activeGroupId: "g1",
      };

      mockInvoke.mockResolvedValueOnce(savedLayout);

      await testInScope(async () => {
        await store.loadFromDisk();

        // g1 should only have the terminal tab
        expect(store.state.groups.g1.tabs).toEqual([{ id: "term-1", type: "terminal" }]);
        expect(store.state.groups.g1.activeTabId).toBe("term-1");

        // g2 had only an editor tab — filtered to empty
        expect(store.state.groups.g2.tabs).toEqual([]);
        expect(store.state.groups.g2.activeTabId).toBeNull();

        // Tree structure preserved
        expect(store.getRoot()?.type).toBe("branch");
      });
    });

    it("does nothing when no saved layout", async () => {
      const { invoke } = await import("../../invoke");
      vi.mocked(invoke).mockResolvedValueOnce(null);

      await testInScope(async () => {
        await store.loadFromDisk();
        expect(store.getRoot()).toBeNull();
      });
    });
  });
});
