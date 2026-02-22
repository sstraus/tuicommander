import { terminalsStore } from "../stores/terminals";
import { isQuickSwitcherActive } from "../platform";
import { lastMenuActionTime } from "../menuDedup";

/** All action callbacks the keyboard shortcut handler needs */
export interface ShortcutHandlers {
  zoomIn: () => void;
  zoomOut: () => void;
  zoomReset: () => void;
  createNewTerminal: () => void;
  closeTerminal: (id: string, skipConfirm?: boolean) => void;
  reopenClosedTab: () => void;
  navigateTab: (direction: "prev" | "next") => void;
  clearTerminal: () => void;
  terminalIds: () => string[];
  handleTerminalSelect: (id: string) => void;
  handleSplit: (direction: "vertical" | "horizontal") => void;
  handleRunCommand: (forceDialog: boolean) => void;
  switchToBranchByIndex: (index: number) => void;
  isQuickSwitcherOpen: () => boolean;
  lazygitAvailable: () => boolean;
  spawnLazygit: () => void;
  openLazygitPane: () => void;
  toggleDiffPanel: () => void;
  toggleMarkdownPanel: () => void;
  toggleSidebar: () => void;
  togglePromptLibrary: () => void;
  toggleSettings: () => void;
  toggleTaskQueue: () => void;
  toggleGitOpsPanel: () => void;
  toggleHelpPanel: () => void;
  toggleNotesPanel: () => void;
  toggleFileBrowserPanel: () => void;
}

/** Register keyboard shortcuts. Returns cleanup function. */
export function useKeyboardShortcuts(handlers: ShortcutHandlers): () => void {
  const handleKeydown = (e: KeyboardEvent) => {
    // Skip if a native menu accelerator already handled this shortcut (dedup guard)
    if (Date.now() - lastMenuActionTime < 200) return;

    // Quick switch to branch by index (Cmd+Ctrl+N on macOS, Ctrl+Alt+N on Win/Linux)
    if (isQuickSwitcherActive(e) && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      handlers.switchToBranchByIndex(parseInt(e.key));
      return;
    }

    // When quick switcher is open, skip other shortcuts
    if (handlers.isQuickSwitcherOpen()) return;

    const isMeta = e.metaKey || e.ctrlKey;

    // Zoom controls
    if (isMeta && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      handlers.zoomIn();
      return;
    }
    if (isMeta && e.key === "-") {
      e.preventDefault();
      handlers.zoomOut();
      return;
    }
    if (isMeta && e.key === "0") {
      e.preventDefault();
      handlers.zoomReset();
      return;
    }

    // Terminal management
    if (isMeta && e.key === "t") {
      e.preventDefault();
      handlers.createNewTerminal();
      return;
    }
    if (isMeta && e.key === "w") {
      e.preventDefault();
      const layout = terminalsStore.state.layout;
      if (layout.direction !== "none" && layout.panes.length === 2) {
        // In split mode: close active pane (closeTerminal handles split collapse)
        const closingId = layout.panes[layout.activePaneIndex];
        if (closingId) handlers.closeTerminal(closingId, true);
      } else {
        const activeId = terminalsStore.state.activeId;
        if (activeId) handlers.closeTerminal(activeId);
      }
      return;
    }

    // Split pane shortcuts
    if (isMeta && e.key === "\\") {
      e.preventDefault();
      handlers.handleSplit(e.altKey ? "horizontal" : "vertical");
      return;
    }

    // Navigate between split panes (Alt+Arrow)
    if (e.altKey && !isMeta && !e.shiftKey) {
      const layout = terminalsStore.state.layout;
      if (layout.direction !== "none" && layout.panes.length === 2) {
        const isNavKey =
          (layout.direction === "vertical" && (e.key === "ArrowLeft" || e.key === "ArrowRight")) ||
          (layout.direction === "horizontal" && (e.key === "ArrowUp" || e.key === "ArrowDown"));
        if (isNavKey) {
          e.preventDefault();
          const newIndex: 0 | 1 = layout.activePaneIndex === 0 ? 1 : 0;
          terminalsStore.setActivePaneIndex(newIndex);
          const targetId = layout.panes[newIndex];
          if (targetId) {
            terminalsStore.setActive(targetId);
            requestAnimationFrame(() => terminalsStore.get(targetId)?.ref?.focus());
          }
          return;
        }
      }
    }

    // Run command (Cmd+R / Cmd+Shift+R to edit)
    if (isMeta && (e.key === "r" || e.key === "R")) {
      e.preventDefault();
      handlers.handleRunCommand(e.shiftKey);
      return;
    }

    // Panel toggles
    if (isMeta && e.shiftKey && e.key === "D") {
      e.preventDefault();
      handlers.toggleDiffPanel();
      return;
    }
    if (isMeta && e.key === "m") {
      e.preventDefault();
      handlers.toggleMarkdownPanel();
      return;
    }
    if (isMeta && e.key === "n") {
      e.preventDefault();
      handlers.toggleNotesPanel();
      return;
    }
    if (isMeta && e.key === "e") {
      e.preventDefault();
      handlers.toggleFileBrowserPanel();
      return;
    }

    // Prompt library (Cmd+K)
    if (isMeta && e.key === "k") {
      e.preventDefault();
      handlers.togglePromptLibrary();
      return;
    }

    // Settings (Cmd+,)
    if (isMeta && e.key === ",") {
      e.preventDefault();
      handlers.toggleSettings();
      return;
    }

    // Task queue (Cmd+J)
    if (isMeta && e.key === "j") {
      e.preventDefault();
      handlers.toggleTaskQueue();
      return;
    }

    // Reopen closed tab (Cmd+Shift+T)
    if (isMeta && e.shiftKey && e.key === "T") {
      e.preventDefault();
      handlers.reopenClosedTab();
      return;
    }

    // Toggle sidebar (Cmd+[)
    if (isMeta && !e.shiftKey && e.key === "[") {
      e.preventDefault();
      handlers.toggleSidebar();
      return;
    }

    // Previous tab (Cmd+Shift+[)
    if (isMeta && e.shiftKey && e.key === "[") {
      e.preventDefault();
      handlers.navigateTab("prev");
      return;
    }

    // Next tab (Cmd+Shift+])
    if (isMeta && e.shiftKey && e.key === "]") {
      e.preventDefault();
      handlers.navigateTab("next");
      return;
    }

    // Clear terminal (Cmd+L)
    if (isMeta && e.key === "l") {
      e.preventDefault();
      handlers.clearTerminal();
      return;
    }

    // Lazygit (Cmd+G)
    if (isMeta && !e.shiftKey && e.key === "g") {
      e.preventDefault();
      if (handlers.lazygitAvailable()) handlers.spawnLazygit();
      return;
    }

    // Git operations panel (Cmd+Shift+G)
    if (isMeta && e.shiftKey && e.key === "G") {
      e.preventDefault();
      handlers.toggleGitOpsPanel();
      return;
    }

    // Help panel (Cmd+?)
    if (isMeta && e.key === "?") {
      e.preventDefault();
      handlers.toggleHelpPanel();
      return;
    }

    // Lazygit split pane (Cmd+Shift+L) (Story 047)
    if (isMeta && e.shiftKey && e.key === "L") {
      e.preventDefault();
      if (handlers.lazygitAvailable()) handlers.openLazygitPane();
      return;
    }

    // Navigate terminals with number keys
    if (isMeta && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      const index = parseInt(e.key) - 1;
      const ids = handlers.terminalIds();
      if (index < ids.length) {
        handlers.handleTerminalSelect(ids[index]);
      }
      return;
    }
  };

  document.addEventListener("keydown", handleKeydown);
  return () => document.removeEventListener("keydown", handleKeydown);
}
