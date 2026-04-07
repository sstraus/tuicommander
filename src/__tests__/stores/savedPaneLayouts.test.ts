import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
}));

import { savedPaneLayouts, paneLayoutKey } from "../../stores/savedPaneLayouts";
import type { PaneLayoutState } from "../../stores/paneLayout";

describe("savedPaneLayouts", () => {
  beforeEach(() => {
    savedPaneLayouts.clear();
  });

  describe("paneLayoutKey", () => {
    it("creates key from repoPath and branchName", () => {
      const key = paneLayoutKey("/repos/myapp", "main");
      expect(key).toBe("/repos/myapp\0main");
    });

    it("different branches produce different keys", () => {
      const k1 = paneLayoutKey("/repos/myapp", "main");
      const k2 = paneLayoutKey("/repos/myapp", "feature");
      expect(k1).not.toBe(k2);
    });
  });

  describe("get / set / delete", () => {
    const layout: PaneLayoutState = {
      root: { type: "leaf", id: "g1" },
      groups: { g1: { id: "g1", tabs: [{ id: "t1", type: "terminal" }], activeTabId: "t1" } },
      activeGroupId: "g1",
    };

    it("stores and retrieves layout by key", () => {
      const key = paneLayoutKey("/repo", "main");
      savedPaneLayouts.set(key, layout);
      expect(savedPaneLayouts.get(key)).toEqual(layout);
    });

    it("returns undefined for missing key", () => {
      expect(savedPaneLayouts.get(paneLayoutKey("/repo", "main"))).toBeUndefined();
    });

    it("delete removes the entry", () => {
      const key = paneLayoutKey("/repo", "main");
      savedPaneLayouts.set(key, layout);
      savedPaneLayouts.delete(key);
      expect(savedPaneLayouts.get(key)).toBeUndefined();
    });

    it("clear removes all entries", () => {
      savedPaneLayouts.set(paneLayoutKey("/r1", "main"), layout);
      savedPaneLayouts.set(paneLayoutKey("/r2", "dev"), layout);
      savedPaneLayouts.clear();
      expect(savedPaneLayouts.size).toBe(0);
    });
  });
});
