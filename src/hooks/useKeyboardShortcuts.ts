import { terminalsStore } from "../stores/terminals";
import { paneLayoutStore } from "../stores/paneLayout";
import { isQuickSwitcherActive, isMacOS } from "../platform";
import { lastMenuActionTime } from "../menuDedup";
import { keybindingsStore } from "../stores/keybindings";
import { normalizeCombo } from "../keybindingDefaults";
import type { ActionName } from "../keybindingDefaults";
import { pluginRegistry } from "../plugins/pluginRegistry";
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
  normalizeCombo("Cmd+P"),       // print dialog
  normalizeCombo("Cmd+Shift+P"), // private/incognito window
  normalizeCombo("Cmd+Shift+A"), // search tabs (Chrome)
]);

/** All action callbacks the keyboard shortcut handler needs */
export interface ShortcutHandlers {
  zoomIn: () => void;
  zoomOut: () => void;
  zoomReset: () => void;
  zoomInAll: () => void;
  zoomOutAll: () => void;
  zoomResetAll: () => void;
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
  toggleMarkdownPanel: () => void;
  toggleSidebar: () => void;
  toggleSettings: () => void;
  toggleTaskQueue: () => void;
  toggleGitOpsPanel: () => void;
  toggleHelpPanel: () => void;
  toggleNotesPanel: () => void;
  toggleFileBrowserPanel: () => void;
  findInTerminal: () => void;
  toggleCommandPalette: () => void;
  toggleActivityDashboard: () => void;
  toggleWorktreeManager: () => void;
  toggleBranchSwitcher: () => void;
  toggleErrorLog: () => void;
  toggleBranchesTab: () => void;
  toggleMcpPopup: () => void;
  clearScrollback: () => void;
  scrollToTop: () => void;
  scrollToBottom: () => void;
  scrollPageUp: () => void;
  scrollPageDown: () => void;
  toggleZoomPane: () => void;
  closeActivePane?: () => void;
  togglePromptLibrary: () => void;
  toggleDiffScroll: () => void;
  toggleGlobalWorkspace: () => void;
  openFile: () => void;
  newFile: () => void;
  openFolder: () => void;
  openPath: () => void;
  openSecondaryWindow: () => void;
  toggleCommandOverview: () => void;
}

/** Keys that are modifiers only — not real shortcut targets */
const modifierKeys = new Set(["control", "meta", "alt", "shift"]);

/**
 * Shift changes the character produced by a key (e.g. Shift+[ → {).
 * Map shifted characters back to their unshifted base so combos match
 * the binding definitions which use the unshifted key name.
 */
const SHIFTED_KEY_MAP: Record<string, string> = {
  "{": "[", "}": "]", "+": "=", "~": "`", "!": "1", "@": "2",
  "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8",
  "(": "9", ")": "0", "_": "-", "|": "\\", ":": ";", "\"": "'",
  "<": ",", ">": ".", "?": "/",
};

/**
 * Convert a KeyboardEvent into a normalized combo string that matches our keybinding format.
 * "Cmd" maps to the platform primary modifier: metaKey on macOS, ctrlKey on Windows/Linux.
 * "Ctrl" is the literal Control key on macOS (separate from Cmd).
 * On macOS bare Ctrl+letter must NOT match "Cmd+key" shortcuts — those are terminal
 * control codes (Ctrl+A = SOH, Ctrl+E = ENQ, etc.) that must reach the PTY.
 * Ctrl+non-letter (e.g. Ctrl+Tab) is safe and gets the "ctrl" modifier.
 */
export function eventToCombo(e: KeyboardEvent): string {
  const parts: string[] = [];
  const mac = isMacOS();
  const primaryMod = mac ? e.metaKey : e.ctrlKey;
  if (primaryMod) parts.push("cmd");
  // On macOS, expose Ctrl as a separate "ctrl" modifier (for Ctrl+Tab etc.)
  if (mac && e.ctrlKey && !e.metaKey) parts.push("ctrl");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");

  // For modifier-only keydowns (e.g. pressing Shift alone), key would be "Shift"
  // — skip those since they're not real shortcuts
  let key = e.key.toLowerCase();
  if (modifierKeys.has(key)) return "";

  // Un-shift the key so combos match binding definitions
  if (e.shiftKey && SHIFTED_KEY_MAP[key]) {
    key = SHIFTED_KEY_MAP[key];
  }

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
    case "zoom-in-all": handlers.zoomInAll(); return true;
    case "zoom-out-all": handlers.zoomOutAll(); return true;
    case "zoom-reset-all": handlers.zoomResetAll(); return true;

    // Terminal management
    case "new-terminal": handlers.createNewTerminal(); return true;
    case "close-terminal": {
      if (paneLayoutStore.isSplit()) {
        handlers.closeActivePane?.();
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
    case "toggle-markdown": handlers.toggleMarkdownPanel(); return true;
    case "toggle-notes": handlers.toggleNotesPanel(); return true;
    case "toggle-file-browser": handlers.toggleFileBrowserPanel(); return true;
    case "toggle-settings": handlers.toggleSettings(); return true;
    case "toggle-task-queue": handlers.toggleTaskQueue(); return true;
    case "toggle-sidebar": handlers.toggleSidebar(); return true;
    case "toggle-git-ops": handlers.toggleGitOpsPanel(); return true;
    case "toggle-help": handlers.toggleHelpPanel(); return true;
    case "find-in-terminal": handlers.findInTerminal(); return true;
    case "command-palette": handlers.toggleCommandPalette(); return true;
    case "activity-dashboard": handlers.toggleActivityDashboard(); return true;
    case "worktree-manager": handlers.toggleWorktreeManager(); return true;
    case "quick-branch-switch": handlers.toggleBranchSwitcher(); return true;
    case "toggle-error-log": handlers.toggleErrorLog(); return true;
    case "toggle-branches-tab": handlers.toggleBranchesTab(); return true;
    case "toggle-mcp-popup": handlers.toggleMcpPopup(); return true;
    case "clear-scrollback": handlers.clearScrollback(); return true;
    case "scroll-to-top": handlers.scrollToTop(); return true;
    case "scroll-to-bottom": handlers.scrollToBottom(); return true;
    case "scroll-page-up": handlers.scrollPageUp(); return true;
    case "scroll-page-down": handlers.scrollPageDown(); return true;
    case "zoom-pane": handlers.toggleZoomPane(); return true;
    case "prompt-library": handlers.togglePromptLibrary(); return true;
    case "toggle-diff-scroll": handlers.toggleDiffScroll(); return true;
    case "toggle-global-workspace": handlers.toggleGlobalWorkspace(); return true;
    case "open-file": handlers.openFile(); return true;
    case "new-file": handlers.newFile(); return true;
    case "open-folder": handlers.openFolder(); return true;
    case "open-path": handlers.openPath(); return true;
    case "open-secondary-window": handlers.openSecondaryWindow(); return true;
    case "command-overview": handlers.toggleCommandOverview(); return true;

    // Tab navigation
    case "prev-tab": handlers.navigateTab("prev"); return true;
    case "next-tab": handlers.navigateTab("next"); return true;

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
    // Ctrl+Tab / Ctrl+Shift+Tab — must run BEFORE dedup guard because the native
    // menu accelerator also fires for this combo; we still need preventDefault
    // here to stop the Tab keypress from reaching the terminal.
    // Ctrl+Tab on macOS is handled by a native NSEvent monitor (tab_shortcut.rs)
    // that emits "ctrl-tab" Tauri events. On Win/Linux, JS sees the keydown:
    if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
      e.preventDefault();
      if (paneLayoutStore.isSplit()) {
        // Cycle tabs within active pane group only
        const group = paneLayoutStore.getActiveGroup();
        if (group && group.tabs.length > 1) {
          const currentIdx = group.tabs.findIndex(t => t.id === group.activeTabId);
          const delta = e.shiftKey ? -1 : 1;
          const nextIdx = (currentIdx + delta + group.tabs.length) % group.tabs.length;
          paneLayoutStore.setActiveTab(group.id, group.tabs[nextIdx].id);
        }
      } else {
        handlers.navigateTab(e.shiftKey ? "prev" : "next");
      }
      return;
    }

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
    // Skip when focus is in an input field (Alt+Arrow = move cursor by word on macOS)
    const activeTag = (e.target as HTMLElement)?.tagName;
    const inInputField = activeTag === "INPUT" || activeTag === "TEXTAREA" || activeTag === "SELECT";
    if (e.altKey && !(e.metaKey || e.ctrlKey) && !e.shiftKey && !inInputField) {
      if (paneLayoutStore.isSplit()) {
        const arrowMap: Record<string, "left" | "right" | "up" | "down"> = {
          ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down",
        };
        const dir = arrowMap[e.key];
        if (dir) {
          e.preventDefault();
          const targetGroupId = paneLayoutStore.navigatePane(dir);
          if (targetGroupId) {
            const group = paneLayoutStore.state.groups[targetGroupId];
            const termTab = group?.tabs.find(t => t.type === "terminal");
            if (termTab) {
              terminalsStore.setActive(termTab.id);
              requestAnimationFrame(() => terminalsStore.get(termTab.id)?.ref?.focus());
            }
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

    if (action) {
      // Plugin-registered commands are dispatched through pluginRegistry.
      // Built-in actions go through the static dispatchAction switch.
      if (action.startsWith("plugin:")) {
        if (pluginRegistry.invokePluginCommand(action)) {
          e.preventDefault();
        }
      } else if (dispatchAction(action as ActionName, handlers)) {
        e.preventDefault();
      }
    }
  };

  document.addEventListener("keydown", handleKeydown);
  return () => document.removeEventListener("keydown", handleKeydown);
}
