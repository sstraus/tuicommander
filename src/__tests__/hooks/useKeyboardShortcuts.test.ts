import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { useKeyboardShortcuts, type ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  terminalsStore.setLayout({ direction: "none", panes: [], ratio: 0.5, activePaneIndex: 0 });
}

function createMockHandlers(): ShortcutHandlers {
  return {
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    zoomReset: vi.fn(),
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
    lazygitAvailable: vi.fn().mockReturnValue(true),
    spawnLazygit: vi.fn(),
    openLazygitPane: vi.fn(),
    toggleDiffPanel: vi.fn(),
    toggleMarkdownPanel: vi.fn(),
    toggleSidebar: vi.fn(),
    togglePromptLibrary: vi.fn(),
    toggleSettings: vi.fn(),
    toggleTaskQueue: vi.fn(),
    toggleGitOpsPanel: vi.fn(),
    toggleHelpPanel: vi.fn(),
    toggleNotesPanel: vi.fn(),
    toggleFileBrowserPanel: vi.fn(),
    findInTerminal: vi.fn(),
    toggleCommandPalette: vi.fn(),
    toggleActivityDashboard: vi.fn(),
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

  beforeEach(() => {
    resetStores();
    handlers = createMockHandlers();
    cleanup = useKeyboardShortcuts(handlers);
  });

  afterEach(() => {
    cleanup?.();
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
    it("Cmd+Shift+[ navigates to previous tab", () => {
      fireKeydown("[", { metaKey: true, shiftKey: true });
      expect(handlers.navigateTab).toHaveBeenCalledWith("prev");
    });

    it("Cmd+Shift+] navigates to next tab", () => {
      fireKeydown("]", { metaKey: true, shiftKey: true });
      expect(handlers.navigateTab).toHaveBeenCalledWith("next");
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
    it("Cmd+Shift+D toggles diff panel", () => {
      fireKeydown("D", { metaKey: true, shiftKey: true });
      expect(handlers.toggleDiffPanel).toHaveBeenCalled();
    });

    it("Cmd+M toggles markdown panel", () => {
      fireKeydown("m", { metaKey: true });
      expect(handlers.toggleMarkdownPanel).toHaveBeenCalled();
    });

    it("Cmd+K toggles prompt library", () => {
      fireKeydown("k", { metaKey: true });
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

    it("Cmd+Shift+G toggles git ops panel", () => {
      fireKeydown("G", { metaKey: true, shiftKey: true });
      expect(handlers.toggleGitOpsPanel).toHaveBeenCalled();
    });
  });

  describe("lazygit shortcuts", () => {
    it("Cmd+G spawns lazygit when available", () => {
      fireKeydown("g", { metaKey: true });
      expect(handlers.spawnLazygit).toHaveBeenCalled();
    });

    it("Cmd+G does nothing when lazygit unavailable", () => {
      (handlers.lazygitAvailable as ReturnType<typeof vi.fn>).mockReturnValue(false);
      fireKeydown("g", { metaKey: true });
      expect(handlers.spawnLazygit).not.toHaveBeenCalled();
    });

    it("Cmd+Shift+L opens lazygit pane", () => {
      fireKeydown("L", { metaKey: true, shiftKey: true });
      expect(handlers.openLazygitPane).toHaveBeenCalled();
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
    it("closes active pane and collapses split", () => {
      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
      terminalsStore.setLayout({
        direction: "vertical",
        panes: [id1, id2],
        ratio: 0.5,
        activePaneIndex: 0,
      });
      terminalsStore.setActive(id1);

      fireKeydown("w", { metaKey: true });

      expect(handlers.closeTerminal).toHaveBeenCalledWith(id1, true);
    });

    it("closes non-split active terminal", () => {
      const id = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      terminalsStore.setActive(id);

      fireKeydown("w", { metaKey: true });

      expect(handlers.closeTerminal).toHaveBeenCalledWith(id);
    });

    it("does nothing when no active terminal", () => {
      fireKeydown("w", { metaKey: true });

      expect(handlers.closeTerminal).not.toHaveBeenCalled();
    });
  });

  describe("split pane navigation", () => {
    it("Alt+ArrowRight navigates panes in vertical split", () => {
      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
      terminalsStore.setLayout({
        direction: "vertical",
        panes: [id1, id2],
        ratio: 0.5,
        activePaneIndex: 0,
      });

      fireKeydown("ArrowRight", { altKey: true });

      expect(terminalsStore.state.layout.activePaneIndex).toBe(1);
      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("Alt+ArrowLeft navigates panes in vertical split", () => {
      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
      terminalsStore.setLayout({
        direction: "vertical",
        panes: [id1, id2],
        ratio: 0.5,
        activePaneIndex: 1,
      });

      fireKeydown("ArrowLeft", { altKey: true });

      expect(terminalsStore.state.layout.activePaneIndex).toBe(0);
      expect(terminalsStore.state.activeId).toBe(id1);
    });

    it("Alt+ArrowDown navigates panes in horizontal split", () => {
      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
      terminalsStore.setLayout({
        direction: "horizontal",
        panes: [id1, id2],
        ratio: 0.5,
        activePaneIndex: 0,
      });

      fireKeydown("ArrowDown", { altKey: true });

      expect(terminalsStore.state.layout.activePaneIndex).toBe(1);
    });

    it("ignores Alt+Arrow when not in split mode", () => {
      const id = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
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
