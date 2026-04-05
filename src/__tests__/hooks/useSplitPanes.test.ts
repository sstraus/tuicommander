import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { useSplitPanes } from "../../hooks/useSplitPanes";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  paneLayoutStore.reset();
  resetGroupCounter();
}

describe("useSplitPanes", () => {
  let splitPanes: ReturnType<typeof useSplitPanes>;

  beforeEach(() => {
    resetStores();
    splitPanes = useSplitPanes();
  });

  describe("handleSplit", () => {
    it("does nothing when no active terminal and no split", () => {
      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.isSplit()).toBe(false);
    });

    it("bootstraps tree with active terminal on first split", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");

      expect(paneLayoutStore.isSplit()).toBe(true);
      const allGroups = paneLayoutStore.getAllGroupIds();
      expect(allGroups.length).toBe(2);

      // First group contains the original terminal
      const firstGroup = paneLayoutStore.state.groups[allGroups[0]];
      expect(firstGroup.tabs.length).toBe(1);
      expect(firstGroup.tabs[0].id).toBe(id);
      expect(firstGroup.tabs[0].type).toBe("terminal");

      // Second group is empty (no dummy terminal)
      const secondGroup = paneLayoutStore.state.groups[allGroups[1]];
      expect(secondGroup.tabs.length).toBe(0);
    });

    it("creates empty pane (no dummy terminal)", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");

      // Only one terminal exists — no new terminal was created
      expect(terminalsStore.getIds().length).toBe(1);
    });

    it("sets new group as active after split", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");

      const allGroups = paneLayoutStore.getAllGroupIds();
      // Active group should be the new (second) group
      expect(paneLayoutStore.state.activeGroupId).toBe(allGroups[1]);
    });

    it("split with no active repo works without error", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Orphan",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      expect(() => splitPanes.handleSplit("vertical")).not.toThrow();
      expect(paneLayoutStore.isSplit()).toBe(true);
    });

    it("splits active group when already split (recursive)", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(2);

      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(3);
    });

    it("allows mixed directions (vertical then horizontal)", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(2);

      // Unlike the old flat model, opposite direction is allowed
      splitPanes.handleSplit("horizontal");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(3);
    });

    it("respects max depth 3", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      // Depth 0 → 1
      splitPanes.handleSplit("vertical");
      // Depth 1 → 2
      splitPanes.handleSplit("vertical");
      // Depth 2 → would be 3 (rejected)
      const groupsBefore = paneLayoutStore.getAllGroupIds().length;
      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(groupsBefore);
    });
  });

  describe("resetLayout", () => {
    it("clears tree and all groups", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");
      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.isSplit()).toBe(true);

      splitPanes.resetLayout();

      expect(paneLayoutStore.isSplit()).toBe(false);
      expect(paneLayoutStore.getAllGroupIds().length).toBe(0);
      expect(paneLayoutStore.state.activeGroupId).toBe(null);
    });
  });

  describe("closeActivePane", () => {
    it("removes active group and flattens tree", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");
      expect(paneLayoutStore.getAllGroupIds().length).toBe(2);

      // Close the active (empty) pane
      splitPanes.closeActivePane();

      // Should collapse to single leaf
      expect(paneLayoutStore.isSplit()).toBe(false);
      expect(paneLayoutStore.getAllGroupIds().length).toBe(1);
    });

    it("destroys terminals in the closed group", () => {
      const id1 = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id1);

      splitPanes.handleSplit("vertical");
      const allGroups = paneLayoutStore.getAllGroupIds();

      // Switch to first group (has terminal) and close it
      paneLayoutStore.setActiveGroup(allGroups[0]);
      splitPanes.closeActivePane();

      // Terminal should be removed
      expect(terminalsStore.get(id1)).toBeUndefined();
    });
  });

  describe("toggleZoomPane", () => {
    it("zoom: saves tree, shows single leaf; unzoom: restores", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");
      const allGroupsBefore = paneLayoutStore.getAllGroupIds();
      expect(allGroupsBefore.length).toBe(2);

      // Switch to first group (has terminal)
      paneLayoutStore.setActiveGroup(allGroupsBefore[0]);

      // Zoom
      splitPanes.toggleZoomPane();
      expect(splitPanes.zoomed()).toBe(true);
      expect(paneLayoutStore.isSplit()).toBe(false); // single leaf, no branch
      expect(paneLayoutStore.getAllGroupIds().length).toBe(1);
      expect(paneLayoutStore.getAllGroupIds()[0]).toBe(allGroupsBefore[0]);

      // Unzoom — restore full tree
      splitPanes.toggleZoomPane();
      expect(splitPanes.zoomed()).toBe(false);
      expect(paneLayoutStore.isSplit()).toBe(true);
      expect(paneLayoutStore.getAllGroupIds().length).toBe(2);
    });

    it("does nothing when not split", () => {
      splitPanes.toggleZoomPane();
      expect(splitPanes.zoomed()).toBe(false);
    });
  });
});
