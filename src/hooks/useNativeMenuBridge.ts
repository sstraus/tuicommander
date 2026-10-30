import { createEffect, onCleanup, type Setter } from "solid-js";
import { invoke, listen } from "../invoke";
import { setLastMenuActionTime } from "../menuDedup";
import { togglePanel } from "../panelRouter";
import { appLogger } from "../stores/appLogger";
import { commandPaletteStore } from "../stores/commandPalette";
import { diffTabsStore } from "../stores/diffTabs";
import { errorLogStore } from "../stores/errorLog";
import { mcpPopupStore } from "../stores/mcpPopup";
import { promptLibraryStore } from "../stores/promptLibrary";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { tunnelPanelStore } from "../stores/tunnelPanel";
import { uiStore } from "../stores/ui";
import { updaterStore } from "../stores/updater";
import { handleOpenUrl } from "../utils/openUrl";
import { globalWorkspaceTabIds } from "../utils/paneTabOrder";
import type { useGitOperations } from "./useGitOperations";
import type { ShortcutHandlers } from "./useKeyboardShortcuts";
import type { useSplitPanes } from "./useSplitPanes";
import type { useTerminalLifecycle } from "./useTerminalLifecycle";

interface NativeMenuBridgeOptions {
	shortcutHandlers: ShortcutHandlers;
	terminalLifecycle: ReturnType<typeof useTerminalLifecycle>;
	gitOps: ReturnType<typeof useGitOperations>;
	splitPanes: ReturnType<typeof useSplitPanes>;
	closeActiveTabOrPane: () => void;
	setSettingsPanelVisible: Setter<boolean>;
	setRunCommandDialogVisible: Setter<boolean>;
	setTaskQueueVisible: Setter<boolean>;
	setShowProcessManager: Setter<boolean>;
	setHelpPanelVisible: Setter<boolean>;
}

/** Dispatches native application-menu commands through the same feature handlers as keyboard shortcuts. */
export function dispatchNativeMenuAction(action: string, options: NativeMenuBridgeOptions): void {
	const { shortcutHandlers, terminalLifecycle, gitOps, splitPanes } = options;
	switch (action) {
		case "new-tab":
			gitOps.handleNewTab();
			break;
		case "close-tab":
			options.closeActiveTabOrPane();
			break;
		case "reopen-closed-tab":
			terminalLifecycle.reopenClosedTab();
			break;
		case "new-file":
			shortcutHandlers.newFile();
			break;
		case "open-file":
			shortcutHandlers.openFile();
			break;
		case "open-folder":
			shortcutHandlers.openFolder();
			break;
		case "open-path":
			shortcutHandlers.openPath();
			break;
		case "settings":
			options.setSettingsPanelVisible((visible) => !visible);
			break;
		case "quit-app":
			void invoke("request_quit").catch((error) => appLogger.error("app", "Quit request failed", error));
			break;

		case "copy":
			terminalLifecycle.copyFromTerminal();
			break;
		case "clear-terminal":
			terminalLifecycle.clearTerminal();
			break;
		case "clear-scrollback":
			terminalLifecycle.clearScrollback();
			break;
		case "refresh-terminal":
			terminalLifecycle.refreshTerminal();
			break;
		case "find-in-terminal":
			shortcutHandlers.findInTerminal();
			break;
		case "toggle-sidebar":
			uiStore.toggleSidebar();
			break;
		case "split-right":
			splitPanes.handleSplit("vertical");
			break;
		case "split-down":
			splitPanes.handleSplit("horizontal");
			break;
		case "zoom-in":
			shortcutHandlers.zoomIn();
			break;
		case "zoom-out":
			shortcutHandlers.zoomOut();
			break;
		case "zoom-reset":
			shortcutHandlers.zoomReset();
			break;
		case "zoom-in-all":
			shortcutHandlers.zoomInAll();
			break;
		case "zoom-out-all":
			shortcutHandlers.zoomOutAll();
			break;
		case "zoom-reset-all":
			shortcutHandlers.zoomResetAll();
			break;
		case "diff-panel":
			togglePanel("git");
			break;
		case "markdown-panel":
			uiStore.toggleMarkdownPanel();
			break;
		case "notes-panel":
			uiStore.toggleIdeasPanel();
			break;
		case "file-browser":
			uiStore.toggleFileBrowserPanel();
			break;
		case "outline-panel":
			uiStore.toggleOutlinePanel();
			break;
		case "ai-chat":
			if (settingsStore.isAiChatEnabled()) shortcutHandlers.toggleAiChatPanel();
			break;
		case "compose-panel":
			shortcutHandlers.toggleComposePanel();
			break;
		case "zoom-pane":
			splitPanes.toggleZoomPane();
			break;
		case "focus-mode":
			uiStore.toggleFocusMode();
			break;
		case "global-workspace":
			shortcutHandlers.toggleGlobalWorkspace();
			break;
		case "next-tab":
			terminalLifecycle.navigateTab("next");
			break;
		case "prev-tab":
			terminalLifecycle.navigateTab("prev");
			break;
		case "block-prev":
			shortcutHandlers.blockPrev();
			break;
		case "block-next":
			shortcutHandlers.blockNext();
			break;
		case "block-fold-toggle":
			shortcutHandlers.blockFoldToggle();
			break;
		case "block-search-toggle":
			shortcutHandlers.blockSearchToggle();
			break;
		case "prompt-library":
			promptLibraryStore.toggleDrawer();
			break;
		case "run-command":
			gitOps.handleRunCommand(false, () => options.setRunCommandDialogVisible(true));
			break;
		case "edit-run-command":
			gitOps.handleRunCommand(true, () => options.setRunCommandDialogVisible(true));
			break;
		case "git-operations":
			togglePanel("git");
			break;
		case "diff-scroll": {
			const repoPath = repositoriesStore.state.activeRepoPath;
			if (repoPath) {
				uiStore.setDiffViewMode("scroll");
				diffTabsStore.add(repoPath, "", "M");
			}
			break;
		}
		case "branches":
			uiStore.isDetached("git") ? togglePanel("git") : uiStore.toggleGitPanelOnTab("branches");
			break;
		case "worktree-manager":
			shortcutHandlers.toggleWorktreeManager();
			break;
		case "quick-branch-switch":
			shortcutHandlers.toggleBranchSwitcher();
			break;
		case "task-queue":
			options.setTaskQueueVisible((visible) => !visible);
			break;
		case "content-search":
			commandPaletteStore.open();
			commandPaletteStore.setQuery("?");
			break;
		case "tunnels":
			tunnelPanelStore.toggle();
			break;
		case "process-manager":
			options.setShowProcessManager((visible) => !visible);
			break;
		case "help-panel":
			options.setHelpPanelVisible((visible) => !visible);
			break;
		case "command-palette":
			commandPaletteStore.toggle();
			break;
		case "activity-dashboard":
			shortcutHandlers.toggleActivityDashboard();
			break;
		case "error-log":
			errorLogStore.toggle();
			break;
		case "mcp-popup":
			mcpPopupStore.toggle();
			break;
		case "check-for-updates":
			updaterStore.checkForUpdate().catch((error) => appLogger.warn("app", "Updater manual check failed", error));
			break;
		case "online-guide":
			handleOpenUrl("https://tuicommander.com/docs/");
			break;
		case "changelog":
			handleOpenUrl("https://github.com/sstraus/tuicommander/blob/main/CHANGELOG.md");
			break;
		case "about":
			options.setHelpPanelVisible(true);
			break;
		default: {
			const tabMatch = action.match(/^switch-tab-(\d)$/);
			if (!tabMatch) break;
			const index = Number.parseInt(tabMatch[1], 10) - 1;
			const ids = globalWorkspaceTabIds() ?? terminalLifecycle.terminalIds();
			if (index < ids.length) terminalLifecycle.handleTerminalSelect(ids[index]);
		}
	}
}

export function useNativeMenuBridge(options: NativeMenuBridgeOptions): void {
	createEffect(() => {
		let unlisten: (() => void) | undefined;
		listen<string>("menu-action", (event) => {
			setLastMenuActionTime(Date.now());
			dispatchNativeMenuAction(event.payload, options);
		})
			.then((dispose) => {
				unlisten = dispose;
			})
			.catch((error) => appLogger.error("app", "Failed to register menu-action listener", error));
		onCleanup(() => unlisten?.());
	});

	createEffect(() => {
		let unlisten: (() => void) | undefined;
		listen<string>("ctrl-tab", (event) => {
			if (event.payload === "prev" || event.payload === "next") {
				options.terminalLifecycle.navigateTab(event.payload);
			}
		})
			.then((dispose) => {
				unlisten = dispose;
			})
			.catch((error) => appLogger.error("app", "Failed to register ctrl-tab listener", error));
		onCleanup(() => unlisten?.());
	});
}
