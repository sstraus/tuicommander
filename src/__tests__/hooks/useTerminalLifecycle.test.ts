import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import { makeTerminal } from "../helpers/store";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { useTerminalLifecycle } from "../../hooks/useTerminalLifecycle";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";

// Reset store state between tests by removing all terminals
function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const id of diffTabsStore.getIds()) {
    diffTabsStore.remove(id);
  }
  for (const id of mdTabsStore.getIds()) {
    mdTabsStore.remove(id);
  }
  for (const id of editorTabsStore.getIds()) {
    editorTabsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
  paneLayoutStore.reset();
  resetGroupCounter();
}

/** Flush pending requestAnimationFrame callbacks (used by closeTerminal to defer focus) */
const flushRAF = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

describe("useTerminalLifecycle", () => {
  const mockPty = {
    canSpawn: vi.fn().mockResolvedValue(true),
    close: vi.fn().mockResolvedValue(undefined),
  };

  const mockDialogs = {
    confirmCloseTerminal: vi.fn().mockResolvedValue(true),
    confirm: vi.fn().mockResolvedValue(true),
  };

  const mockSetStatusInfo = vi.fn();

  let lifecycle: ReturnType<typeof useTerminalLifecycle>;

  beforeEach(() => {
    resetStores();
    mockPty.canSpawn.mockReset().mockResolvedValue(true);
    mockPty.close.mockReset().mockResolvedValue(undefined);
    mockDialogs.confirmCloseTerminal.mockReset().mockResolvedValue(true);
    mockDialogs.confirm.mockReset().mockResolvedValue(true);
    mockSetStatusInfo.mockReset();
    // Default: no foreground process (plain shell)
    mockInvoke.mockReset().mockResolvedValue(null);

    lifecycle = useTerminalLifecycle({
      pty: mockPty,
      dialogs: mockDialogs,
      setStatusInfo: mockSetStatusInfo,
      getDefaultFontSize: () => 14,
    });
  });

  describe("zoom", () => {
    it("returns default font size when no active terminal", () => {
      expect(lifecycle.activeFontSize()).toBe(14);
    });

    it("returns active terminal font size", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 18,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      expect(lifecycle.activeFontSize()).toBe(18);
    });

    it("zoomIn increases font size by step", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      lifecycle.zoomIn();
      expect(terminalsStore.get(id)!.fontSize).toBe(16);
    });

    it("zoomOut decreases font size by step", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      lifecycle.zoomOut();
      expect(terminalsStore.get(id)!.fontSize).toBe(12);
    });

    it("zoomReset returns to default font size", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 22,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      lifecycle.zoomReset();
      expect(terminalsStore.get(id)!.fontSize).toBe(14);
    });

    it("clamps zoom to min font size", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 8,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      lifecycle.zoomOut();
      expect(terminalsStore.get(id)!.fontSize).toBe(8);
    });

    it("clamps zoom to max font size", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 32,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      lifecycle.zoomIn();
      expect(terminalsStore.get(id)!.fontSize).toBe(32);
    });

    it("zoomInAll increases font size for all terminals", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2", fontSize: 18 }));
      terminalsStore.setActive(id1);

      lifecycle.zoomInAll();
      expect(terminalsStore.get(id1)!.fontSize).toBe(16);
      expect(terminalsStore.get(id2)!.fontSize).toBe(20);
    });

    it("zoomOutAll decreases font size for all terminals", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2", fontSize: 18 }));
      terminalsStore.setActive(id1);

      lifecycle.zoomOutAll();
      expect(terminalsStore.get(id1)!.fontSize).toBe(12);
      expect(terminalsStore.get(id2)!.fontSize).toBe(16);
    });

    it("zoomResetAll resets font size for all terminals", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1", fontSize: 20 }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2", fontSize: 10 }));
      terminalsStore.setActive(id1);

      lifecycle.zoomResetAll();
      expect(terminalsStore.get(id1)!.fontSize).toBe(14);
      expect(terminalsStore.get(id2)!.fontSize).toBe(14);
    });

    it("zoomOutAll clamps to min font size", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1", fontSize: 8 }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      terminalsStore.setActive(id1);

      lifecycle.zoomOutAll();
      expect(terminalsStore.get(id1)!.fontSize).toBe(8);
      expect(terminalsStore.get(id2)!.fontSize).toBe(12);
    });

    it("zoomInAll clamps to max font size", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1", fontSize: 32 }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      terminalsStore.setActive(id1);

      lifecycle.zoomInAll();
      expect(terminalsStore.get(id1)!.fontSize).toBe(32);
      expect(terminalsStore.get(id2)!.fontSize).toBe(16);
    });
  });

  describe("createNewTerminal", () => {
    it("adds a terminal to the store and sets it active", async () => {
      const id = await lifecycle.createNewTerminal();
      expect(id).toBeDefined();
      expect(terminalsStore.state.activeId).toBe(id);
      expect(terminalsStore.get(id!)?.name).toMatch(/Terminal \d+/);
    });

    it("inherits cwd from active terminal", async () => {
      const first = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "First",
        cwd: "/some/path",
        awaitingInput: null,
      });
      terminalsStore.setActive(first);

      const id = await lifecycle.createNewTerminal();
      expect(terminalsStore.get(id!)?.cwd).toBe("/some/path");
    });

    it("sets status info when max sessions reached", async () => {
      mockPty.canSpawn.mockResolvedValue(false);

      const id = await lifecycle.createNewTerminal();
      expect(id).toBeUndefined();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
    });
  });

  describe("closeTerminal", () => {
    it("removes terminal from store", async () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(terminalsStore.get(id)).toBeUndefined();
    });

    it("closes PTY session if exists", async () => {
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(mockPty.close).toHaveBeenCalledWith("sess-1");
    });

    it("shows confirmation when foreground process is running", async () => {
      mockInvoke.mockResolvedValue("claude");
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test Terminal",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(mockInvoke).toHaveBeenCalledWith("has_foreground_process", { sessionId: "sess-1" });
      expect(mockDialogs.confirmCloseTerminal).toHaveBeenCalledWith("Test Terminal");
    });

    it("shows confirmation for non-agent processes (e.g. htop)", async () => {
      mockInvoke.mockResolvedValue("htop");
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "htop session",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(mockDialogs.confirmCloseTerminal).toHaveBeenCalledWith("htop session");
    });

    it("skips confirmation for plain shell (no foreground process)", async () => {
      mockInvoke.mockResolvedValue(null);
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Idle Shell",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(mockDialogs.confirmCloseTerminal).not.toHaveBeenCalled();
      expect(terminalsStore.get(id)).toBeUndefined();
    });

    it("skips confirmation when backend call fails", async () => {
      mockInvoke.mockRejectedValue(new Error("session gone"));
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Gone",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(mockDialogs.confirmCloseTerminal).not.toHaveBeenCalled();
      expect(terminalsStore.get(id)).toBeUndefined();
    });

    it("does not close when user cancels confirmation", async () => {
      mockInvoke.mockResolvedValue("claude");
      mockDialogs.confirmCloseTerminal.mockResolvedValue(false);

      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      expect(terminalsStore.get(id)).toBeDefined();
    });

    it("skips confirmation with skipConfirm flag", async () => {
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id, true);
      expect(mockDialogs.confirmCloseTerminal).not.toHaveBeenCalled();
      expect(terminalsStore.get(id)).toBeUndefined();
    });

    it("saves closed tab for reopening", async () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 16,
        name: "My Terminal",
        cwd: "/some/path",
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);

      // Verify by reopening
      await lifecycle.reopenClosedTab();
      const reopened = terminalsStore.getActive();
      expect(reopened?.name).toBe("My Terminal");
      expect(reopened?.fontSize).toBe(16);
      expect(reopened?.cwd).toBe("/some/path");
    });

    it("closes diff tabs directly", async () => {
      diffTabsStore.add("/repo", "file.ts", "M");
      const ids = diffTabsStore.getIds();
      expect(ids.length).toBe(1);

      await lifecycle.closeTerminal(ids[0]);
      expect(diffTabsStore.getIds().length).toBe(0);
    });

    it("closes markdown tabs directly", async () => {
      mdTabsStore.add("/repo", "README.md");
      const ids = mdTabsStore.getIds();
      expect(ids.length).toBe(1);

      await lifecycle.closeTerminal(ids[0]);
      expect(mdTabsStore.getIds().length).toBe(0);
    });

    it("restores terminal focus when closing last diff tab", async () => {
      const termId = terminalsStore.add(makeTerminal({ name: "T1", sessionId: "s1" }));
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.addTerminalToBranch("/repo", "main", termId);

      diffTabsStore.add("/repo", "file.ts", "M");
      const diffId = diffTabsStore.getIds()[0];
      // Simulate selecting the diff tab (sets terminalsStore.activeId to null)
      lifecycle.handleTerminalSelect(diffId);
      expect(terminalsStore.state.activeId).toBeNull();

      await lifecycle.closeTerminal(diffId);
      expect(diffTabsStore.getIds().length).toBe(0);
      expect(terminalsStore.state.activeId).toBe(termId);
    });

    it("restores terminal focus when closing last editor tab", async () => {
      const termId = terminalsStore.add(makeTerminal({ name: "T1", sessionId: "s1" }));
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.addTerminalToBranch("/repo", "main", termId);

      editorTabsStore.add("/repo/file.ts", "file.ts");
      const editId = editorTabsStore.getIds()[0];
      lifecycle.handleTerminalSelect(editId);
      expect(terminalsStore.state.activeId).toBeNull();

      await lifecycle.closeTerminal(editId);
      expect(editorTabsStore.getIds().length).toBe(0);
      expect(terminalsStore.state.activeId).toBe(termId);
    });

    it("asks for confirmation when closing a dirty editor tab", async () => {
      editorTabsStore.add("/repo/file.ts", "file.ts");
      const editId = editorTabsStore.getIds()[0];
      editorTabsStore.setDirty(editId, true);

      mockDialogs.confirm.mockResolvedValueOnce(true);
      await lifecycle.closeTerminal(editId);

      expect(mockDialogs.confirm).toHaveBeenCalledTimes(1);
      expect(mockDialogs.confirm.mock.calls[0][0]).toMatchObject({
        title: "Unsaved changes",
        kind: "warning",
      });
      expect(editorTabsStore.getIds().length).toBe(0);
    });

    it("keeps the dirty editor tab when the user cancels", async () => {
      editorTabsStore.add("/repo/file.ts", "file.ts");
      const editId = editorTabsStore.getIds()[0];
      editorTabsStore.setDirty(editId, true);

      mockDialogs.confirm.mockResolvedValueOnce(false);
      await lifecycle.closeTerminal(editId);

      expect(mockDialogs.confirm).toHaveBeenCalledTimes(1);
      expect(editorTabsStore.getIds()).toContain(editId);
    });

    it("closes a clean editor tab without prompting", async () => {
      editorTabsStore.add("/repo/file.ts", "file.ts");
      const editId = editorTabsStore.getIds()[0];

      await lifecycle.closeTerminal(editId);

      expect(mockDialogs.confirm).not.toHaveBeenCalled();
      expect(editorTabsStore.getIds().length).toBe(0);
    });

    it("selects sibling diff tab when closing one of multiple diff tabs", async () => {
      diffTabsStore.add("/repo", "a.ts", "M");
      diffTabsStore.add("/repo", "b.ts", "A");
      const ids = diffTabsStore.getIds();
      expect(ids.length).toBe(2);

      lifecycle.handleTerminalSelect(ids[0]);
      await lifecycle.closeTerminal(ids[0]);
      expect(diffTabsStore.getIds().length).toBe(1);
      // Should activate the remaining diff tab, not fall back to terminal
      expect(diffTabsStore.state.activeId).toBe(ids[1]);
    });
  });

  describe("reopenClosedTab", () => {
    it("does nothing when no closed tabs", async () => {
      const countBefore = terminalsStore.getCount();
      await lifecycle.reopenClosedTab();
      expect(terminalsStore.getCount()).toBe(countBefore);
    });

    it("restores last closed tab", async () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 18,
        name: "Closed Tab",
        cwd: "/test",
        awaitingInput: null,
      });

      await lifecycle.closeTerminal(id);
      await lifecycle.reopenClosedTab();

      const active = terminalsStore.getActive();
      expect(active?.name).toBe("Closed Tab");
      expect(active?.fontSize).toBe(18);
    });
  });

  describe("navigateTab", () => {
    it("navigates to next tab", () => {
      // Set up repo with branch and terminals
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      terminalsStore.setActive(id1);

      lifecycle.navigateTab("next");
      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("wraps around from last to first", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      terminalsStore.setActive(id2);

      lifecycle.navigateTab("next");
      expect(terminalsStore.state.activeId).toBe(id1);
    });
  });

  describe("clearTerminal", () => {
    it("calls clear on active terminal ref", () => {
      const mockClear = vi.fn();
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      terminalsStore.setActive(id);
      terminalsStore.update(id, { ref: { clear: mockClear, fit: vi.fn(), write: vi.fn(), writeln: vi.fn(), input: vi.fn(), focus: vi.fn(), getSessionId: vi.fn(), openSearch: vi.fn(), closeSearch: vi.fn(), searchBuffer: vi.fn(() => []), scrollToLine: vi.fn(), scrollToTop: vi.fn(), scrollToBottom: vi.fn(), scrollPages: vi.fn() } });

      lifecycle.clearTerminal();
      expect(mockClear).toHaveBeenCalled();
    });
  });

  describe("handleTerminalSelect", () => {
    it("sets terminal as active and deactivates others", () => {
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      diffTabsStore.add("/repo", "file.ts", "M");
      mdTabsStore.add("/repo", "README.md");

      lifecycle.handleTerminalSelect(id);
      expect(terminalsStore.state.activeId).toBe(id);
      expect(diffTabsStore.state.activeId).toBeNull();
      expect(mdTabsStore.state.activeId).toBeNull();
    });

    it("activates diff tab when id starts with diff-", () => {
      diffTabsStore.add("/repo", "file.ts", "M");
      const diffId = diffTabsStore.getIds()[0];

      lifecycle.handleTerminalSelect(diffId);
      expect(diffTabsStore.state.activeId).toBe(diffId);
      expect(terminalsStore.state.activeId).toBeNull();
    });

    it("activates markdown tab when id starts with md-", () => {
      mdTabsStore.add("/repo", "README.md");
      const mdId = mdTabsStore.getIds()[0];

      lifecycle.handleTerminalSelect(mdId);
      expect(mdTabsStore.state.activeId).toBe(mdId);
      expect(terminalsStore.state.activeId).toBeNull();
    });
  });

  describe("closeOtherTabs", () => {
    it("closes all terminal tabs except the kept one", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const id3 = terminalsStore.add(makeTerminal({ name: "T3" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      repositoriesStore.addTerminalToBranch("/repo", "main", id3);

      await lifecycle.closeOtherTabs(id2);
      expect(terminalsStore.get(id1)).toBeUndefined();
      expect(terminalsStore.get(id2)).toBeDefined();
      expect(terminalsStore.get(id3)).toBeUndefined();
    });

    it("closes other diff tabs when keeping a diff tab", async () => {
      diffTabsStore.add("/repo", "a.ts", "M");
      diffTabsStore.add("/repo", "b.ts", "A");
      const ids = diffTabsStore.getIds();

      await lifecycle.closeOtherTabs(ids[0]);

      expect(diffTabsStore.getIds().length).toBe(1);
      expect(diffTabsStore.state.activeId).toBe(ids[0]);
    });

    it("closes other md tabs when keeping an md tab", async () => {
      mdTabsStore.add("/repo", "README.md");
      mdTabsStore.add("/repo", "CHANGELOG.md");
      const ids = mdTabsStore.getIds();

      await lifecycle.closeOtherTabs(ids[0]);

      expect(mdTabsStore.getIds().length).toBe(1);
      expect(mdTabsStore.state.activeId).toBe(ids[0]);
    });
  });

  describe("closeTabsToRight", () => {
    it("closes terminal tabs to the right", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const id3 = terminalsStore.add(makeTerminal({ name: "T3" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      repositoriesStore.addTerminalToBranch("/repo", "main", id3);

      await lifecycle.closeTabsToRight(id1);

      expect(terminalsStore.get(id1)).toBeDefined();
      expect(terminalsStore.get(id2)).toBeUndefined();
      expect(terminalsStore.get(id3)).toBeUndefined();
    });

    it("closes diff tabs to the right", async () => {
      diffTabsStore.add("/repo", "a.ts", "M");
      diffTabsStore.add("/repo", "b.ts", "A");
      diffTabsStore.add("/repo", "c.ts", "D");
      const ids = diffTabsStore.getIds();

      await lifecycle.closeTabsToRight(ids[0]);

      expect(diffTabsStore.getIds().length).toBe(1);
    });

    it("closes md tabs to the right", async () => {
      mdTabsStore.add("/repo", "README.md");
      mdTabsStore.add("/repo", "CHANGELOG.md");
      const ids = mdTabsStore.getIds();

      await lifecycle.closeTabsToRight(ids[0]);

      expect(mdTabsStore.getIds().length).toBe(1);
    });
  });

  describe("copyFromTerminal", () => {
    it("copies selection to clipboard", async () => {
      const mockWriteText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText: mockWriteText, readText: vi.fn() },
        writable: true,
        configurable: true,
      });
      vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "selected text" } as Selection);

      await lifecycle.copyFromTerminal();

      expect(mockWriteText).toHaveBeenCalledWith("selected text");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Copied to clipboard");
    });

    it("does nothing when no selection", async () => {
      vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "" } as Selection);

      await lifecycle.copyFromTerminal();

      expect(mockSetStatusInfo).not.toHaveBeenCalledWith("Copied to clipboard");
    });
  });

  describe("pasteToTerminal", () => {
    it("pastes clipboard text to active terminal", async () => {
      const mockWrite = vi.fn();
      Object.defineProperty(navigator, "clipboard", {
        value: { readText: vi.fn().mockResolvedValue("pasted text"), writeText: vi.fn() },
        writable: true,
        configurable: true,
      });

      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      terminalsStore.setActive(id);
      terminalsStore.update(id, {
        ref: { write: mockWrite, clear: vi.fn(), fit: vi.fn(), writeln: vi.fn(), input: vi.fn(), focus: vi.fn(), getSessionId: vi.fn(), openSearch: vi.fn(), closeSearch: vi.fn(), searchBuffer: vi.fn(() => []), scrollToLine: vi.fn(), scrollToTop: vi.fn(), scrollToBottom: vi.fn(), scrollPages: vi.fn() },
      });

      await lifecycle.pasteToTerminal();

      expect(mockWrite).toHaveBeenCalledWith("pasted text");
    });
  });

  describe("handleTerminalFocus", () => {
    it("sets terminal as active", () => {
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));

      lifecycle.handleTerminalFocus(id);

      expect(terminalsStore.state.activeId).toBe(id);
    });

    it("blocks activation of a terminal belonging to another repo/branch", () => {
      // Set up two repos with terminals on different branches
      repositoriesStore.add({ path: "/repo-a", displayName: "Repo A" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.setActive("/repo-a");
      repositoriesStore.setActiveBranch("/repo-a", "main");

      repositoriesStore.add({ path: "/repo-b", displayName: "Repo B" });
      repositoriesStore.setBranch("/repo-b", "develop", { worktreePath: "/repo-b" });

      const termA = terminalsStore.add(makeTerminal({ name: "A1", cwd: "/repo-a" }));
      repositoriesStore.addTerminalToBranch("/repo-a", "main", termA);
      terminalsStore.setActive(termA);

      const termB = terminalsStore.add(makeTerminal({ name: "B1", cwd: "/repo-b" }));
      repositoriesStore.addTerminalToBranch("/repo-b", "develop", termB);

      // Attempt to focus the terminal from repo B while repo A is active
      lifecycle.handleTerminalFocus(termB);

      // Should be blocked — termA should remain active
      expect(terminalsStore.state.activeId).toBe(termA);
    });

    it("allows focus when no active branch context exists", () => {
      // No repo/branch set up — guard skips (activeTerminals is empty)
      const term = terminalsStore.add(makeTerminal({ name: "T1" }));

      lifecycle.handleTerminalFocus(term);

      expect(terminalsStore.state.activeId).toBe(term);
    });

    it("blocks focus for same-repo different-branch terminal", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo-feat" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const termMain = terminalsStore.add(makeTerminal({ name: "Main", cwd: "/repo" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", termMain);
      terminalsStore.setActive(termMain);

      const termFeature = terminalsStore.add(makeTerminal({ name: "Feature", cwd: "/repo-feat" }));
      repositoriesStore.addTerminalToBranch("/repo", "feature", termFeature);

      lifecycle.handleTerminalFocus(termFeature);

      // Should be blocked — termMain should remain active
      expect(terminalsStore.state.activeId).toBe(termMain);
    });
  });

  describe("closeTerminal (branch tracking)", () => {
    it("removes terminal from active branch and activates last remaining", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      terminalsStore.setActive(id1);

      await lifecycle.closeTerminal(id1, true);
      await flushRAF();

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.terminals).not.toContain(id1);
      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("sets activeId to null when closing last terminal on a branch (not cross-repo)", async () => {
      // Set up two repos, each with terminals on their own branches
      repositoriesStore.add({ path: "/repo-a", displayName: "Repo A" });
      repositoriesStore.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
      repositoriesStore.setActive("/repo-a");
      repositoriesStore.setActiveBranch("/repo-a", "main");

      repositoriesStore.add({ path: "/repo-b", displayName: "Repo B" });
      repositoriesStore.setBranch("/repo-b", "develop", { worktreePath: "/repo-b" });

      const termA = terminalsStore.add(makeTerminal({ name: "A1", cwd: "/repo-a" }));
      repositoriesStore.addTerminalToBranch("/repo-a", "main", termA);
      terminalsStore.setActive(termA);

      const termB = terminalsStore.add(makeTerminal({ name: "B1", cwd: "/repo-b" }));
      repositoriesStore.addTerminalToBranch("/repo-b", "develop", termB);

      // Close repo A's only terminal — should NOT fall back to repo B's terminal
      await lifecycle.closeTerminal(termA, true);

      expect(terminalsStore.state.activeId).toBeNull();
    });

    it("does not activate cross-repo terminal when closing last branch terminal", async () => {
      // Same repo, different branches
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo-feat" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const termMain = terminalsStore.add(makeTerminal({ name: "Main", cwd: "/repo" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", termMain);
      terminalsStore.setActive(termMain);

      const termFeature = terminalsStore.add(makeTerminal({ name: "Feature", cwd: "/repo-feat" }));
      repositoriesStore.addTerminalToBranch("/repo", "feature", termFeature);

      // Close the only terminal on main — should NOT fall back to feature branch's terminal
      await lifecycle.closeTerminal(termMain, true);

      expect(terminalsStore.state.activeId).toBeNull();
    });
  });

  describe("navigateTab (prev direction)", () => {
    it("navigates to previous tab", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      terminalsStore.setActive(id2);

      lifecycle.navigateTab("prev");
      expect(terminalsStore.state.activeId).toBe(id1);
    });

    it("wraps around from first to last", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      terminalsStore.setActive(id1);

      lifecycle.navigateTab("prev");
      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("does nothing with single terminal", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      terminalsStore.setActive(id1);

      lifecycle.navigateTab("prev");
      expect(terminalsStore.state.activeId).toBe(id1);
    });
  });

  describe("closeTerminal (pane tree collapse)", () => {
    it("removes tab from pane group and collapses when group empties", async () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const g1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
      const g2 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g2, { id: id2, type: "terminal" });
      paneLayoutStore.setRoot({
        type: "branch", direction: "vertical",
        children: [{ type: "leaf", id: g1 }, { type: "leaf", id: g2 }],
        ratios: [0.5, 0.5],
      });
      paneLayoutStore.setActiveGroup(g1);
      terminalsStore.setActive(id1);

      await lifecycle.closeTerminal(id1, true);
      await flushRAF();

      // g1 should be removed, g2 should be the only leaf (collapsed to single)
      expect(paneLayoutStore.state.groups[g1]).toBeUndefined();
      expect(paneLayoutStore.state.activeGroupId).toBe(g2);
      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("removes tab but keeps group if other tabs remain", async () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const g1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
      paneLayoutStore.addTab(g1, { id: id2, type: "terminal" });
      paneLayoutStore.setRoot({ type: "leaf", id: g1 });
      paneLayoutStore.setActiveGroup(g1);
      terminalsStore.setActive(id1);

      await lifecycle.closeTerminal(id1, true);

      // Group should still exist with id2
      expect(paneLayoutStore.state.groups[g1]).toBeDefined();
      expect(paneLayoutStore.state.groups[g1].tabs).toHaveLength(1);
      expect(paneLayoutStore.state.groups[g1].tabs[0].id).toBe(id2);
    });

    it("does not affect pane tree when closing a non-split terminal", async () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));

      await lifecycle.closeTerminal(id1, true);

      // No pane tree should be set up
      expect(paneLayoutStore.isSplit()).toBe(false);
    });

    it("does not affect pane tree when closing terminal not in any group", async () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const id3 = terminalsStore.add(makeTerminal({ name: "T3" }));
      const g1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
      const g2 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g2, { id: id2, type: "terminal" });
      paneLayoutStore.setRoot({
        type: "branch", direction: "vertical",
        children: [{ type: "leaf", id: g1 }, { type: "leaf", id: g2 }],
        ratios: [0.5, 0.5],
      });
      paneLayoutStore.setActiveGroup(g1);
      terminalsStore.setActive(id3);

      await lifecycle.closeTerminal(id3, true);

      // Split should remain unchanged
      expect(paneLayoutStore.isSplit()).toBe(true);
      expect(paneLayoutStore.getAllGroupIds()).toHaveLength(2);
    });
  });

  describe("reopenClosedTab (max sessions)", () => {
    it("shows status when max sessions reached during reopen", async () => {
      // Close a tab first
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      await lifecycle.closeTerminal(id, true);

      // Then hit max sessions
      mockPty.canSpawn.mockResolvedValue(false);
      await lifecycle.reopenClosedTab();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
    });
  });
});
