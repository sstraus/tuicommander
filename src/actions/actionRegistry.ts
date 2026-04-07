/**
 * Centralized action registry — single source of truth for all executable actions,
 * their labels, categories, and keybindings.
 *
 * Consumed by the Command Palette and KeyboardShortcutsTab.
 */
import type { ActionName } from "../keybindingDefaults";
import { keybindingsStore } from "../stores/keybindings";
import { comboToDisplay } from "../utils/hotkey";
import type { ShortcutHandlers } from "../hooks/useKeyboardShortcuts";

export interface ActionEntry {
  id: string;  // ActionName for static entries, or dynamic IDs like "switch-repo:/path"
  label: string;
  category: string;
  keybinding: string;
  execute: () => void;
}

/** Action metadata: label and category for each ActionName */
interface ActionMeta {
  label: string;
  category: string;
}

/** Static descriptions for all registered actions */
const ACTION_META: Partial<Record<ActionName, ActionMeta>> = {
  "new-terminal": { label: "New terminal tab", category: "Terminal" },
  "close-terminal": { label: "Close terminal tab", category: "Terminal" },
  "reopen-closed-tab": { label: "Reopen closed tab", category: "Terminal" },
  "clear-terminal": { label: "Clear terminal", category: "Terminal" },
  "find-in-terminal": { label: "Find in content", category: "Terminal" },
  "run-command": { label: "Run saved command", category: "Terminal" },
  "edit-command": { label: "Edit saved command", category: "Terminal" },
  "prev-tab": { label: "Previous tab", category: "Terminal" },
  "next-tab": { label: "Next tab", category: "Terminal" },

  "zoom-in": { label: "Zoom in", category: "Zoom" },
  "zoom-out": { label: "Zoom out", category: "Zoom" },
  "zoom-reset": { label: "Reset zoom", category: "Zoom" },
  "zoom-in-all": { label: "Zoom in all terminals", category: "Zoom" },
  "zoom-out-all": { label: "Zoom out all terminals", category: "Zoom" },
  "zoom-reset-all": { label: "Reset zoom all terminals", category: "Zoom" },

  "toggle-markdown": { label: "Toggle markdown panel", category: "Panels" },
  "toggle-settings": { label: "Open settings", category: "Panels" },
  "toggle-task-queue": { label: "Toggle task queue", category: "Panels" },
  "toggle-notes": { label: "Toggle ideas panel", category: "Panels" },
  "toggle-help": { label: "Toggle help panel", category: "Panels" },
  "toggle-file-browser": { label: "Toggle file browser", category: "Panels" },
  "toggle-file-browser-content-search": { label: "Search file contents", category: "File Browser" },
  "toggle-diff-scroll": { label: "Branch diff scroll view", category: "Git" },
  "toggle-git-ops": { label: "Git panel", category: "Git" },
  "toggle-branches-tab": { label: "Branches tab", category: "Git" },

  "toggle-global-workspace": { label: "Toggle global workspace", category: "Navigation" },
  "split-vertical": { label: "Split vertically", category: "Split Panes" },
  "split-horizontal": { label: "Split horizontally", category: "Split Panes" },

  "toggle-sidebar": { label: "Toggle sidebar", category: "Navigation" },

  "command-palette": { label: "Command palette", category: "Navigation" },
  "activity-dashboard": { label: "Activity dashboard", category: "Navigation" },
  "worktree-manager": { label: "Worktree manager", category: "Git" },
  "quick-branch-switch": { label: "Quick branch switch", category: "Git" },
  "toggle-error-log": { label: "Error log", category: "Navigation" },
  "toggle-mcp-popup": { label: "MCP servers (per-repo)", category: "Panels" },
  "clear-scrollback": { label: "Clear scrollback", category: "Terminal" },
  "scroll-to-top": { label: "Scroll to top", category: "Terminal" },
  "scroll-to-bottom": { label: "Scroll to bottom", category: "Terminal" },
  "scroll-page-up": { label: "Scroll page up", category: "Terminal" },
  "scroll-page-down": { label: "Scroll page down", category: "Terminal" },
  "zoom-pane": { label: "Maximize/restore pane", category: "Split Panes" },
  "prompt-library": { label: "Prompt Library", category: "Navigation" },

  "open-file": { label: "Open file…", category: "File" },
  "new-file": { label: "New file…", category: "File" },
};

/**
 * Build the full list of executable actions from handlers and current keybindings.
 *
 * Skips numbered tab/branch switching actions (switch-tab-1..9, switch-branch-1..9)
 * since those aren't useful in a palette.
 */
export function getActionEntries(handlers: ShortcutHandlers): ActionEntry[] {
  const entries: ActionEntry[] = [];

  const handlerMap: Partial<Record<ActionName, () => void>> = {
    "new-terminal": handlers.createNewTerminal,
    "close-terminal": () => {
      const activeId = handlers.terminalIds()[0]; // simplified - close active
      if (activeId) handlers.closeTerminal(activeId);
    },
    "reopen-closed-tab": handlers.reopenClosedTab,
    "clear-terminal": handlers.clearTerminal,
    "find-in-terminal": handlers.findInTerminal,
    "run-command": () => handlers.handleRunCommand(false),
    "edit-command": () => handlers.handleRunCommand(true),
    "prev-tab": () => handlers.navigateTab("prev"),
    "next-tab": () => handlers.navigateTab("next"),
    "zoom-in": handlers.zoomIn,
    "zoom-out": handlers.zoomOut,
    "zoom-reset": handlers.zoomReset,
    "zoom-in-all": handlers.zoomInAll,
    "zoom-out-all": handlers.zoomOutAll,
    "zoom-reset-all": handlers.zoomResetAll,
    "toggle-markdown": handlers.toggleMarkdownPanel,
    "toggle-settings": handlers.toggleSettings,
    "toggle-task-queue": handlers.toggleTaskQueue,
    "toggle-notes": handlers.toggleNotesPanel,
    "toggle-help": handlers.toggleHelpPanel,
    "toggle-file-browser": handlers.toggleFileBrowserPanel,
    "toggle-git-ops": handlers.toggleGitOpsPanel,
    "toggle-diff-scroll": handlers.toggleDiffScroll,
    "toggle-global-workspace": handlers.toggleGlobalWorkspace,
    "toggle-branches-tab": handlers.toggleBranchesTab,
    "split-vertical": () => handlers.handleSplit("vertical"),
    "split-horizontal": () => handlers.handleSplit("horizontal"),
    "toggle-sidebar": handlers.toggleSidebar,
    "command-palette": handlers.toggleCommandPalette,
    "activity-dashboard": handlers.toggleActivityDashboard,
    "worktree-manager": handlers.toggleWorktreeManager,
    "quick-branch-switch": handlers.toggleBranchSwitcher,
    "toggle-error-log": handlers.toggleErrorLog,
    "toggle-mcp-popup": handlers.toggleMcpPopup,
    "clear-scrollback": handlers.clearScrollback,
    "scroll-to-top": handlers.scrollToTop,
    "scroll-to-bottom": handlers.scrollToBottom,
    "scroll-page-up": handlers.scrollPageUp,
    "scroll-page-down": handlers.scrollPageDown,
    "zoom-pane": handlers.toggleZoomPane,
    "prompt-library": handlers.togglePromptLibrary,
    "open-file": handlers.openFile,
    "new-file": handlers.newFile,
  };

  for (const [actionId, meta] of Object.entries(ACTION_META)) {
    if (!meta) continue;
    const handler = handlerMap[actionId as ActionName];
    if (!handler) continue;

    const combo = keybindingsStore.getKeyForAction(actionId as ActionName);
    entries.push({
      id: actionId as ActionName,
      label: meta.label,
      category: meta.category,
      keybinding: combo ? comboToDisplay(combo) : "",
      execute: handler,
    });
  }

  return entries;
}
