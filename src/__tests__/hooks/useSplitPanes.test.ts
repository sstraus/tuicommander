import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { useSplitPanes } from "../../hooks/useSplitPanes";

/** Stub config object returned by load_config for setSplitTabMode persistence */
const stubConfig = { split_tab_mode: "separate" };

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
  // Reset layout
  terminalsStore.setLayout({ direction: "none", panes: [], ratio: 0.5, activePaneIndex: 0 });
}

describe("useSplitPanes", () => {
  let splitPanes: ReturnType<typeof useSplitPanes>;

  beforeEach(async () => {
    resetStores();
    // Mock load_config + save_config for setSplitTabMode persistence
    mockInvoke.mockResolvedValueOnce({ ...stubConfig }).mockResolvedValueOnce(undefined);
    await settingsStore.setSplitTabMode("separate");
    mockInvoke.mockReset().mockResolvedValue(undefined);
    splitPanes = useSplitPanes();
  });

  describe("handleSplit", () => {
    it("does nothing when no active terminal", () => {
      splitPanes.handleSplit("vertical");
      expect(terminalsStore.state.layout.panes.length).toBe(0);
    });

    it("initializes layout when first split", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");

      // Should have initialized layout and created a new pane
      expect(terminalsStore.state.layout.panes.length).toBe(2);
      expect(terminalsStore.state.layout.direction).toBe("vertical");
    });

    it("tracks new terminal in branch", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      splitPanes.handleSplit("horizontal");

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.terminals.length).toBe(2);
    });

    it("sets new pane as active", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      splitPanes.handleSplit("vertical");

      // Active terminal should be the new pane, not the original
      expect(terminalsStore.state.activeId).not.toBe(id);
    });

    it("unified mode does NOT add split terminal to branch", async () => {
      mockInvoke.mockResolvedValueOnce({ ...stubConfig }).mockResolvedValueOnce(undefined);
      await settingsStore.setSplitTabMode("unified");

      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      splitPanes.handleSplit("vertical");

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      // Branch should still only have the original terminal
      expect(branch?.terminals.length).toBe(1);
      expect(branch?.terminals[0]).toBe(id);
      // New terminal should still be created and active
      expect(terminalsStore.state.activeId).not.toBe(id);
    });

    it("split with no active repo works without error", () => {
      // Terminal with no repo set up
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Orphan",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      // Should not throw even without a repo
      expect(() => splitPanes.handleSplit("vertical")).not.toThrow();

      // Split should still have created a new terminal
      expect(terminalsStore.state.activeId).not.toBe(id);
      expect(terminalsStore.state.layout.panes.length).toBe(2);
    });
  });
});
