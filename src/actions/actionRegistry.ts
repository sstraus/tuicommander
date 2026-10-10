/**
 * Centralized action registry — single source of truth for all executable actions,
 * their labels, categories, and keybindings.
 *
 * Consumed by the Command Palette and KeyboardShortcutsTab.
 */

import type { ShortcutHandlers } from "../hooks/useKeyboardShortcuts";
import type { ActionName } from "../keybindingDefaults";
import { automationsUi } from "../stores/automations";
import { dictationStore } from "../stores/dictation";
import { keybindingsStore } from "../stores/keybindings";
import { progressStore } from "../stores/progress";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";
import { tunnelPanelStore } from "../stores/tunnelPanel";
import { isTauri } from "../transport";
import { comboToDisplay } from "../utils/hotkey";
import { isHandsFreeArmed, toggleHandsFreeConversation } from "./handsFreeConversation";

export interface ActionEntry {
	id: string; // ActionName for static entries, or dynamic IDs like "switch-repo:/path"
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
	"refresh-terminal": { label: "Refresh terminal (fix glyphs)", category: "Terminal" },
	"find-in-terminal": { label: "Find in content", category: "Terminal" },
	"run-command": { label: "Run saved command", category: "Terminal" },
	"edit-command": { label: "Edit saved command", category: "Terminal" },
	"prev-tab": { label: "Previous tab", category: "Terminal" },
	"next-tab": { label: "Next tab", category: "Terminal" },
	"focus-last-terminal": { label: "Return to last terminal", category: "Navigation" },
	"history-back": { label: "Back to previous terminal in history", category: "Navigation" },
	"history-forward": { label: "Forward to next terminal in history", category: "Navigation" },
	"jump-waiting-terminal": { label: "Jump to waiting terminal", category: "Navigation" },

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
	"toggle-outline": { label: "Toggle outline panel", category: "Panels" },
	"toggle-file-browser-content-search": { label: "Search file contents", category: "File Browser" },
	"toggle-diff-scroll": { label: "Branch diff scroll view", category: "Git" },
	"toggle-git-ops": { label: "Git panel", category: "Git" },
	"toggle-branches-tab": { label: "Branches tab", category: "Git" },

	"toggle-global-workspace": { label: "Toggle global workspace", category: "Navigation" },
	"split-vertical": { label: "Split vertically", category: "Split Panes" },
	"split-horizontal": { label: "Split horizontally", category: "Split Panes" },

	"toggle-sidebar": { label: "Toggle sidebar", category: "Navigation" },

	"command-palette": { label: "Command palette", category: "Navigation" },
	progress: { label: "Open Terminal Progress", category: "Navigation" },
	"activity-dashboard": { label: "Activity dashboard", category: "Navigation" },
	"worktree-manager": { label: "Worktree manager", category: "Git" },
	"quick-branch-switch": { label: "Quick branch switch", category: "Git" },
	"toggle-error-log": { label: "Error log", category: "Navigation" },
	"toggle-ai-chat": { label: "Toggle AI Chat", category: "Panels" },
	"toggle-mcp-popup": { label: "MCP servers (per-repo)", category: "Panels" },
	"clear-scrollback": { label: "Clear scrollback", category: "Terminal" },
	"scroll-to-top": { label: "Scroll to top", category: "Terminal" },
	"scroll-to-bottom": { label: "Scroll to bottom", category: "Terminal" },
	"scroll-page-up": { label: "Scroll page up", category: "Terminal" },
	"scroll-page-down": { label: "Scroll page down", category: "Terminal" },
	"zoom-pane": { label: "Maximize/restore pane", category: "Split Panes" },
	"toggle-focus-mode": { label: "Toggle focus mode", category: "Panels" },
	"prompt-library": { label: "Prompt Library", category: "Navigation" },

	"open-file": { label: "Open file…", category: "File" },
	"new-file": { label: "New file…", category: "File" },
	"open-folder": { label: "Open folder…", category: "File" },
	"open-path": { label: "Open path…", category: "File" },
	"open-secondary-window": { label: "Open secondary window", category: "Navigation" },
	"command-overview": { label: "Command overview", category: "Panels" },
	"detach-activity-dashboard": { label: "Open Activity Dashboard in separate window", category: "Navigation" },
	"toggle-tunnels": { label: "SSH Tunnels", category: "Panels" },
	"process-manager": { label: "Process Manager", category: "Navigation" },
	"open-generators": { label: "Open generators", category: "Generators" },
	"show-remote-qr": { label: "QR for Remote Mobile Connection", category: "Remote" },
	"block-prev": { label: "Previous command block", category: "Terminal" },
	"block-next": { label: "Next command block", category: "Terminal" },
	"block-fold-toggle": { label: "Toggle block fold", category: "Terminal" },
	"block-search-toggle": { label: "Search in block", category: "Terminal" },
	"answers-only": { label: "Toggle answers-only view", category: "Terminal" },
	"toggle-compose-panel": { label: "Toggle compose panel", category: "Panels" },
	// Label is replaced per build by what the toggle will do — see getActionEntries.
	"toggle-hands-free": { label: "Start hands-free conversation", category: "Dictation" },
};

/**
 * Build the full list of executable actions from handlers and current keybindings.
 *
 * Skips numbered tab/branch switching actions (switch-tab-1..9, switch-branch-1..9)
 * since those aren't useful in a palette.
 */
export function getActionEntries(handlers: ShortcutHandlers): ActionEntry[] {
	const entries: ActionEntry[] = [
		{ id: "automations", label: "Automations", category: "Navigation", keybinding: "", execute: automationsUi.open },
	];

	const handlerMap: Partial<Record<ActionName, () => void>> = {
		"new-terminal": handlers.createNewTerminal,
		"close-terminal": handlers.closeActiveTabOrPane,
		"reopen-closed-tab": handlers.reopenClosedTab,
		"clear-terminal": handlers.clearTerminal,
		"refresh-terminal": handlers.refreshTerminal,
		"find-in-terminal": handlers.findInTerminal,
		"run-command": () => handlers.handleRunCommand(false),
		"edit-command": () => handlers.handleRunCommand(true),
		"prev-tab": () => handlers.navigateTab("prev"),
		"next-tab": () => handlers.navigateTab("next"),
		"focus-last-terminal": handlers.focusLastTerminal,
		"history-back": handlers.historyBack,
		"history-forward": handlers.historyForward,
		"jump-waiting-terminal": handlers.jumpWaitingTerminal,
		"zoom-in": handlers.zoomIn,
		"zoom-out": handlers.zoomOut,
		"zoom-reset": handlers.zoomReset,
		"zoom-in-all": handlers.zoomInAll,
		"zoom-out-all": handlers.zoomOutAll,
		"zoom-reset-all": handlers.zoomResetAll,
		"toggle-markdown": handlers.toggleMarkdownPanel,
		"toggle-settings": handlers.toggleSettings,
		"toggle-task-queue": handlers.toggleTaskQueue,
		"toggle-notes": handlers.toggleIdeasPanel,
		"toggle-help": handlers.toggleHelpPanel,
		"toggle-file-browser": handlers.toggleFileBrowserPanel,
		"toggle-file-browser-content-search": handlers.requestFileBrowserContentSearch,
		"toggle-outline": handlers.toggleOutlinePanel,
		"toggle-git-ops": handlers.toggleGitOpsPanel,
		"toggle-diff-scroll": handlers.toggleDiffScroll,
		"toggle-global-workspace": handlers.toggleGlobalWorkspace,
		"toggle-branches-tab": handlers.toggleBranchesTab,
		"split-vertical": () => handlers.handleSplit("vertical"),
		"split-horizontal": () => handlers.handleSplit("horizontal"),
		"toggle-sidebar": handlers.toggleSidebar,
		"command-palette": handlers.toggleCommandPalette,
		progress: progressStore.toggle,
		"activity-dashboard": handlers.toggleActivityDashboard,
		"worktree-manager": handlers.toggleWorktreeManager,
		"quick-branch-switch": handlers.toggleBranchSwitcher,
		"toggle-error-log": handlers.toggleErrorLog,
		"toggle-ai-chat": handlers.toggleAiChatPanel,
		"toggle-mcp-popup": handlers.toggleMcpPopup,
		"clear-scrollback": handlers.clearScrollback,
		"scroll-to-top": handlers.scrollToTop,
		"scroll-to-bottom": handlers.scrollToBottom,
		"scroll-page-up": handlers.scrollPageUp,
		"scroll-page-down": handlers.scrollPageDown,
		"zoom-pane": handlers.toggleZoomPane,
		"toggle-focus-mode": handlers.toggleFocusMode,
		"prompt-library": handlers.togglePromptLibrary,
		"open-file": handlers.openFile,
		"new-file": handlers.newFile,
		"open-folder": handlers.openFolder,
		"open-path": handlers.openPath,
		"command-overview": handlers.toggleCommandOverview,
		"detach-activity-dashboard": handlers.detachActivityDashboard,
		"toggle-tunnels": () => tunnelPanelStore.toggle(),
		"process-manager": handlers.toggleProcessManager,
		"open-generators": handlers.toggleGenerators,
		"show-remote-qr": handlers.showRemoteQr,
		"block-prev": handlers.blockPrev,
		"block-next": handlers.blockNext,
		"block-fold-toggle": handlers.blockFoldToggle,
		"block-search-toggle": handlers.blockSearchToggle,
		"answers-only": () => terminalsStore.toggleAnswersOnly(),
		"toggle-compose-panel": handlers.toggleComposePanel,
		"toggle-hands-free": () => void toggleHandsFreeConversation(),
	};

	// Defensive dedup-by-id — today ACTION_META is a Record so ids are unique by
	// construction, but we keep the guard so a future dynamic-action addition
	// can't silently surface duplicate entries in the palette.
	const seen = new Set<string>();
	for (const [actionId, meta] of Object.entries(ACTION_META)) {
		if (!meta) continue;
		if (seen.has(actionId)) continue;
		if (actionId === "toggle-ai-chat" && !settingsStore.isAiChatEnabled()) continue;
		// A browser tab never loads the dictation config (`refreshConfig` is
		// desktop-only), so there the backend's answer to arming is the gate.
		if (actionId === "toggle-hands-free" && isTauri() && !dictationStore.state.enabled) continue;
		const handler = handlerMap[actionId as ActionName];
		if (!handler) continue;

		const combo = keybindingsStore.getKeyForAction(actionId as ActionName);
		const label = actionId === "toggle-hands-free" && isHandsFreeArmed() ? "Stop hands-free conversation" : meta.label;
		entries.push({
			id: actionId as ActionName,
			label,
			category: meta.category,
			keybinding: combo ? comboToDisplay(combo) : "",
			execute: handler,
		});
		seen.add(actionId);
	}

	return entries;
}
