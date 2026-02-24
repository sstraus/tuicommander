import { terminalsStore } from "../stores/terminals";
import { isQuickSwitcherActive, isMacOS } from "../platform";
import { lastMenuActionTime } from "../menuDedup";
import { keybindingsStore } from "../stores/keybindings";
import { normalizeCombo } from "../keybindingDefaults";
import type { ActionName } from "../keybindingDefaults";
import { isTauri } from "../transport";

/**
 * Normalized combos that are reserved by browsers and should not be intercepted
 * when running in browser mode (non-Tauri). These would block essential browser
 * functionality like page refresh, new tab, close tab, etc.
 */
const BROWSER_RESERVED_COMBOS = new Set([
  normalizeCombo("Cmd+R"),       // refresh
  normalizeCombo("Cmd+Shift+R"), // hard refresh
  normalizeCombo("Cmd+T"),       // new tab
  normalizeCombo("Cmd+W"),       // close tab
  normalizeCombo("Cmd+N"),       // new window
  normalizeCombo("Cmd+L"),       // address bar
  normalizeCombo("Cmd+Shift+T"), // reopen closed tab
  normalizeCombo("Cmd+Shift+P"), // private/incognito window
  normalizeCombo("Cmd+Shift+A"), // search tabs (Chrome)
]);

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
  findInTerminal: () => void;
  toggleCommandPalette: () => void;
  toggleActivityDashboard: () => void;
}

/**
 * Convert a KeyboardEvent into a normalized combo string that matches our keybinding format.
 * "Cmd" maps to the platform primary modifier: metaKey on macOS, ctrlKey on Windows/Linux.
 * On macOS bare Ctrl+key must NOT match "Cmd+key" shortcuts — those are terminal
 * control codes (Ctrl+A = SOH, Ctrl+E = ENQ, etc.) that must reach the PTY.
 */
export function eventToCombo(e: KeyboardEvent): string {
  const parts: string[] = [];
  const primaryMod = isMacOS() ? e.metaKey : e.ctrlKey;
  if (primaryMod) parts.push("cmd");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");

  // For modifier-only keydowns (e.g. pressing Shift alone), key would be "Shift"
  // — skip those since they're not real shortcuts
  const key = e.key.toLowerCase();
  const modifierKeys = new Set(["control", "meta", "alt", "shift"]);
  if (modifierKeys.has(key)) return "";

  parts.sort();
  parts.push(key);
  return parts.join("+");
}

/** Dispatch an action to the appropriate handler. Returns true if handled. */
function dispatchAction(action: ActionName, handlers: ShortcutHandlers): boolean {
  switch (action) {
    // Zoom
    case "zoom-in": handlers.zoomIn(); return true;
    case "zoom-out": handlers.zoomOut(); return true;
    case "zoom-reset": handlers.zoomReset(); return true;

    // Terminal management
    case "new-terminal": handlers.createNewTerminal(); return true;
    case "close-terminal": {
      const layout = terminalsStore.state.layout;
      if (layout.direction !== "none" && layout.panes.length === 2) {
        const closingId = layout.panes[layout.activePaneIndex];
        if (closingId) handlers.closeTerminal(closingId, true);
      } else {
        const activeId = terminalsStore.state.activeId;
        if (activeId) handlers.closeTerminal(activeId);
      }
      return true;
    }
    case "reopen-closed-tab": handlers.reopenClosedTab(); return true;
    case "clear-terminal": handlers.clearTerminal(); return true;

    // Split panes
    case "split-vertical": handlers.handleSplit("vertical"); return true;
    case "split-horizontal": handlers.handleSplit("horizontal"); return true;

    // Run command
    case "run-command": handlers.handleRunCommand(false); return true;
    case "edit-command": handlers.handleRunCommand(true); return true;

    // Panel toggles
    case "toggle-diff": handlers.toggleDiffPanel(); return true;
    case "toggle-markdown": handlers.toggleMarkdownPanel(); return true;
    case "toggle-notes": handlers.toggleNotesPanel(); return true;
    case "toggle-file-browser": handlers.toggleFileBrowserPanel(); return true;
    case "toggle-prompt-library": handlers.togglePromptLibrary(); return true;
    case "toggle-settings": handlers.toggleSettings(); return true;
    case "toggle-task-queue": handlers.toggleTaskQueue(); return true;
    case "toggle-sidebar": handlers.toggleSidebar(); return true;
    case "toggle-git-ops": handlers.toggleGitOpsPanel(); return true;
    case "toggle-help": handlers.toggleHelpPanel(); return true;
    case "find-in-terminal": handlers.findInTerminal(); return true;
    case "command-palette": handlers.toggleCommandPalette(); return true;
    case "activity-dashboard": handlers.toggleActivityDashboard(); return true;

    // Tab navigation
    case "prev-tab": handlers.navigateTab("prev"); return true;
    case "next-tab": handlers.navigateTab("next"); return true;

    // Lazygit
    case "open-lazygit":
      if (handlers.lazygitAvailable()) handlers.spawnLazygit();
      return true;
    case "open-lazygit-pane":
      if (handlers.lazygitAvailable()) handlers.openLazygitPane();
      return true;

    default: {
      // switch-tab-N
      const tabMatch = action.match(/^switch-tab-(\d)$/);
      if (tabMatch) {
        const index = parseInt(tabMatch[1]) - 1;
        const ids = handlers.terminalIds();
        if (index < ids.length) handlers.handleTerminalSelect(ids[index]);
        return true;
      }

      // switch-branch-N
      const branchMatch = action.match(/^switch-branch-(\d)$/);
      if (branchMatch) {
        handlers.switchToBranchByIndex(parseInt(branchMatch[1]));
        return true;
      }

      return false;
    }
  }
}

/** Register keyboard shortcuts. Returns cleanup function. */
export function useKeyboardShortcuts(handlers: ShortcutHandlers): () => void {
  const handleKeydown = (e: KeyboardEvent) => {
    // Skip if a native menu accelerator already handled this shortcut (dedup guard)
    if (Date.now() - lastMenuActionTime < 200) return;

    // Quick switch to branch by index (Cmd+Ctrl+N on macOS, Ctrl+Alt+N on Win/Linux)
    // This uses platform-specific modifier detection, handled separately
    if (isQuickSwitcherActive(e) && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      handlers.switchToBranchByIndex(parseInt(e.key));
      return;
    }

    // When quick switcher is open, skip other shortcuts
    if (handlers.isQuickSwitcherOpen()) return;

    // Navigate between split panes (Alt+Arrow) — layout-dependent, not configurable
    if (e.altKey && !(e.metaKey || e.ctrlKey) && !e.shiftKey) {
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

    // Convert event to normalized combo and look up action
    const combo = eventToCombo(e);
    if (!combo) return;

    // In browser mode, don't intercept combos reserved by the browser
    if (!isTauri() && BROWSER_RESERVED_COMBOS.has(combo)) return;

    // Handle "+" key as alias for "=" in zoom-in (Cmd+= and Cmd++ both zoom in)
    let action = keybindingsStore.getActionForCombo(combo);
    if (!action && e.key === "+" && (e.metaKey || e.ctrlKey)) {
      const altCombo = normalizeCombo("Cmd+=");
      action = keybindingsStore.getActionForCombo(altCombo);
    }

    if (action && dispatchAction(action, handlers)) {
      e.preventDefault();
    }
  };

  document.addEventListener("keydown", handleKeydown);
  return () => document.removeEventListener("keydown", handleKeydown);
}
