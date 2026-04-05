import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import "../mocks/tauri";
import { makeTerminal } from "../helpers/store";
import { terminalsStore } from "../../stores/terminals";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { useKeyboardShortcuts, type ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  terminalsStore.setLayout({ direction: "none", panes: [], ratios: [], activePaneIndex: 0 });
  paneLayoutStore.reset();
  resetGroupCounter();
}

function createMockHandlers(): ShortcutHandlers {
  return {
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    zoomReset: vi.fn(),
    zoomInAll: vi.fn(),
    zoomOutAll: vi.fn(),
    zoomResetAll: vi.fn(),
    createNewTerminal: vi.fn(),
    closeTerminal: vi.fn(),
    reopenClosedTab: vi.fn(),
    navigateTab: vi.fn(),
    clearTerminal: vi.fn(),
    terminalIds: vi.fn().mockReturnValue([]),
    handleTerminalSelect: vi.fn(),
    handleSplit: vi.fn(),
    handleRunCommand: vi.fn(),
    switchToBranchByIndex: vi.fn(),
    isQuickSwitcherOpen: vi.fn().mockReturnValue(false),
    toggleMarkdownPanel: vi.fn(),
    toggleSidebar: vi.fn(),
    toggleSettings: vi.fn(),
    toggleTaskQueue: vi.fn(),
    toggleGitOpsPanel: vi.fn(),
    toggleHelpPanel: vi.fn(),
    toggleNotesPanel: vi.fn(),
    toggleFileBrowserPanel: vi.fn(),
    findInTerminal: vi.fn(),
    toggleCommandPalette: vi.fn(),
    toggleActivityDashboard: vi.fn(),
    toggleWorktreeManager: vi.fn(),
    toggleBranchSwitcher: vi.fn(),
    toggleErrorLog: vi.fn(),
    toggleBranchesTab: vi.fn(),
    toggleMcpPopup: vi.fn(),
    clearScrollback: vi.fn(),
    scrollToTop: vi.fn(),
    scrollToBottom: vi.fn(),
    scrollPageUp: vi.fn(),
    scrollPageDown: vi.fn(),
    toggleZoomPane: vi.fn(),
    closeActivePane: vi.fn(),
    togglePromptLibrary: vi.fn(),
    toggleDiffScroll: vi.fn(),
    openFile: vi.fn(),
    newFile: vi.fn(),
  };
}

function fireKeydown(key: string, opts: Partial<KeyboardEvent> = {}) {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...opts,
  });
  document.dispatchEvent(event);
  return event;
}

describe("useKeyboardShortcuts", () => {
  let handlers: ShortcutHandlers;
  let cleanup: (() => void) | undefined;
  const originalPlatform = navigator.platform;

  beforeEach(() => {
    Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true });
    resetStores();
    handlers = createMockHandlers();
    cleanup = useKeyboardShortcuts(handlers);
  });

  afterEach(() => {
    cleanup?.();
    Object.defineProperty(navigator, "platform", { value: originalPlatform, configurable: true });
  });

  describe("zoom shortcuts", () => {
    it("Cmd+= zooms in", () => {
      fireKeydown("=", { metaKey: true });
      expect(handlers.zoomIn).toHaveBeenCalled();
    });

    it("Cmd++ zooms in", () => {
      fireKeydown("+", { metaKey: true });
      expect(handlers.zoomIn).toHaveBeenCalled();
    });

    it("Cmd+- zooms out", () => {
      fireKeydown("-", { metaKey: true });
      expect(handlers.zoomOut).toHaveBeenCalled();
    });

    it("Cmd+0 resets zoom", () => {
      fireKeydown("0", { metaKey: true });
      expect(handlers.zoomReset).toHaveBeenCalled();
    });
  });

  describe("terminal shortcuts", () => {
    it("Cmd+T creates new terminal", () => {
      fireKeydown("t", { metaKey: true });
      expect(handlers.createNewTerminal).toHaveBeenCalled();
    });

    it("Cmd+W closes active terminal", () => {
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "T1",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      fireKeydown("w", { metaKey: true });
      expect(handlers.closeTerminal).toHaveBeenCalledWith(id);
    });

    it("Cmd+Shift+T reopens closed tab", () => {
      fireKeydown("T", { metaKey: true, shiftKey: true });
      expect(handlers.reopenClosedTab).toHaveBeenCalled();
    });

    it("Cmd+L clears terminal", () => {
      fireKeydown("l", { metaKey: true });
      expect(handlers.clearTerminal).toHaveBeenCalled();
    });
  });

  describe("tab navigation", () => {
    it("Ctrl+Tab navigates to next tab", () => {
      fireKeydown("Tab", { ctrlKey: true });
      expect(handlers.navigateTab).toHaveBeenCalledWith("next");
    });

    it("Ctrl+Shift+Tab navigates to previous tab", () => {
      fireKeydown("Tab", { ctrlKey: true, shiftKey: true });
      expect(handlers.navigateTab).toHaveBeenCalledWith("prev");
    });

    it("Cmd+1 selects first terminal", () => {
      (handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1", "t2"]);
      fireKeydown("1", { metaKey: true });
      expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t1");
    });

    it("Cmd+2 selects second terminal", () => {
      (handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1", "t2"]);
      fireKeydown("2", { metaKey: true });
      expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t2");
    });
  });

  describe("split pane shortcuts", () => {
    it("Cmd+\\ splits vertically", () => {
      fireKeydown("\\", { metaKey: true });
      expect(handlers.handleSplit).toHaveBeenCalledWith("vertical");
    });

    it("Cmd+Alt+\\ splits horizontally", () => {
      fireKeydown("\\", { metaKey: true, altKey: true });
      expect(handlers.handleSplit).toHaveBeenCalledWith("horizontal");
    });
  });

  describe("panel toggles", () => {
    it("Cmd+Shift+D toggles git panel", () => {
      fireKeydown("D", { metaKey: true, shiftKey: true });
      expect(handlers.toggleGitOpsPanel).toHaveBeenCalled();
    });

    it("Cmd+M toggles markdown panel", () => {
      fireKeydown("m", { metaKey: true });
      expect(handlers.toggleMarkdownPanel).toHaveBeenCalled();
    });

    it("Cmd+K clears scrollback", () => {
      fireKeydown("k", { metaKey: true });
      expect(handlers.clearScrollback).toHaveBeenCalled();
    });

    it("Cmd+Shift+K toggles prompt library", () => {
      fireKeydown("k", { metaKey: true, shiftKey: true });
      expect(handlers.togglePromptLibrary).toHaveBeenCalled();
    });

    it("Cmd+, toggles settings", () => {
      fireKeydown(",", { metaKey: true });
      expect(handlers.toggleSettings).toHaveBeenCalled();
    });

    it("Cmd+J toggles task queue", () => {
      fireKeydown("j", { metaKey: true });
      expect(handlers.toggleTaskQueue).toHaveBeenCalled();
    });

    it("Cmd+[ toggles sidebar", () => {
      fireKeydown("[", { metaKey: true });
      expect(handlers.toggleSidebar).toHaveBeenCalled();
    });

    it("Cmd+? toggles help", () => {
      fireKeydown("?", { metaKey: true });
      expect(handlers.toggleHelpPanel).toHaveBeenCalled();
    });

    it("Cmd+N toggles notes panel", () => {
      fireKeydown("n", { metaKey: true });
      expect(handlers.toggleNotesPanel).toHaveBeenCalled();
    });

    it("Cmd+E toggles file browser panel", () => {
      fireKeydown("e", { metaKey: true });
      expect(handlers.toggleFileBrowserPanel).toHaveBeenCalled();
    });

    it("Cmd+Shift+D toggles git ops panel", () => {
      fireKeydown("D", { metaKey: true, shiftKey: true });
      expect(handlers.toggleGitOpsPanel).toHaveBeenCalled();
    });
  });

  describe("run command", () => {
    it("Cmd+R runs command", () => {
      fireKeydown("r", { metaKey: true });
      expect(handlers.handleRunCommand).toHaveBeenCalledWith(false);
    });

    it("Cmd+Shift+R edits run command", () => {
      fireKeydown("R", { metaKey: true, shiftKey: true });
      expect(handlers.handleRunCommand).toHaveBeenCalledWith(true);
    });
  });

  describe("quick switcher suppression", () => {
    it("skips shortcuts when quick switcher is open", () => {
      (handlers.isQuickSwitcherOpen as ReturnType<typeof vi.fn>).mockReturnValue(true);
      fireKeydown("t", { metaKey: true });
      expect(handlers.createNewTerminal).not.toHaveBeenCalled();
    });
  });

  describe("Cmd+W in split mode", () => {
    it("calls closeActivePane when pane tree is split", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      terminalsStore.setActive(id1);

      // Set up pane tree split
      const g1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
      paneLayoutStore.setRoot({ type: "leaf", id: g1 });
      paneLayoutStore.setActiveGroup(g1);
      paneLayoutStore.split(g1, "vertical");

      fireKeydown("w", { metaKey: true });

      expect(handlers.closeActivePane).toHaveBeenCalledOnce();
      expect(handlers.closeTerminal).not.toHaveBeenCalled();
    });

    it("closes non-split active terminal", () => {
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      terminalsStore.setActive(id);

      fireKeydown("w", { metaKey: true });

      expect(handlers.closeTerminal).toHaveBeenCalledWith(id);
    });

    it("does nothing when no active terminal", () => {
      fireKeydown("w", { metaKey: true });

      expect(handlers.closeTerminal).not.toHaveBeenCalled();
      expect(handlers.closeActivePane).not.toHaveBeenCalled();
    });
  });

  describe("split pane navigation", () => {
    /** Helper: create a vertical split with two terminal groups */
    function setupVerticalSplit() {
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
      return { id1, id2, g1, g2 };
    }

    it("Alt+ArrowRight navigates panes in vertical split", () => {
      const { g2 } = setupVerticalSplit();
      fireKeydown("ArrowRight", { altKey: true });
      expect(paneLayoutStore.state.activeGroupId).toBe(g2);
    });

    it("Alt+ArrowLeft navigates panes in vertical split", () => {
      const { g1, g2, id2 } = setupVerticalSplit();
      paneLayoutStore.setActiveGroup(g2);
      terminalsStore.setActive(id2);
      fireKeydown("ArrowLeft", { altKey: true });
      expect(paneLayoutStore.state.activeGroupId).toBe(g1);
    });

    it("Alt+ArrowDown navigates panes in horizontal split", () => {
      const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
      const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
      const g1 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
      const g2 = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(g2, { id: id2, type: "terminal" });
      paneLayoutStore.setRoot({
        type: "branch", direction: "horizontal",
        children: [{ type: "leaf", id: g1 }, { type: "leaf", id: g2 }],
        ratios: [0.5, 0.5],
      });
      paneLayoutStore.setActiveGroup(g1);
      terminalsStore.setActive(id1);

      fireKeydown("ArrowDown", { altKey: true });
      expect(paneLayoutStore.state.activeGroupId).toBe(g2);
    });

    it("Alt+ArrowRight at rightmost pane stays put", () => {
      const { g2, id2 } = setupVerticalSplit();
      paneLayoutStore.setActiveGroup(g2);
      terminalsStore.setActive(id2);
      fireKeydown("ArrowRight", { altKey: true });
      expect(paneLayoutStore.state.activeGroupId).toBe(g2);
    });

    it("ignores Alt+Arrow when not in split mode", () => {
      const id = terminalsStore.add(makeTerminal({ name: "T1" }));
      terminalsStore.setActive(id);

      fireKeydown("ArrowRight", { altKey: true });

      // Should not error and terminal should stay active
      expect(terminalsStore.state.activeId).toBe(id);
    });
  });

  describe("find in terminal", () => {
    it("Cmd+F triggers findInTerminal", () => {
      fireKeydown("f", { metaKey: true });
      expect(handlers.findInTerminal).toHaveBeenCalled();
    });
  });

  describe("number key out of range", () => {
    it("does not select terminal for index beyond available", () => {
      (handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1"]);
      fireKeydown("5", { metaKey: true });
      expect(handlers.handleTerminalSelect).not.toHaveBeenCalled();
    });
  });

  describe("cleanup", () => {
    it("removes keydown listener on cleanup", () => {
      cleanup?.();
      cleanup = undefined;

      fireKeydown("t", { metaKey: true });
      expect(handlers.createNewTerminal).not.toHaveBeenCalled();
    });
  });
});
