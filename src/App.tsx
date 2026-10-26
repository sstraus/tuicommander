import { type Component, createSignal, lazy, Show, Suspense } from "solid-js";
import { ApplicationOverlays } from "./components/ApplicationOverlays/ApplicationOverlays";
import { BranchSwitcher } from "./components/BranchSwitcher/BranchSwitcher";
import { CommandPalette } from "./components/CommandPalette";
import { urlPasteActions } from "./components/CommandPalette/urlPaste";
import { createContextMenu } from "./components/ContextMenu";
import { PanelOrchestrator } from "./components/PanelOrchestrator";
import type { CleanupStep, StepId, StepStatus } from "./components/PostMergeCleanupDialog/PostMergeCleanupDialog";
import { PromptDrawer } from "./components/PromptDrawer";
import { PromptInspector } from "./components/PromptInspector/PromptInspector";
import { PromptOverlay } from "./components/PromptOverlay";
import type { SettingsContext } from "./components/SettingsPanel";
import { SETTINGS_SECTION_EGO } from "./components/SettingsPanel/sections";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TabBar } from "./components/TabBar";
import { TerminalArea } from "./components/TerminalArea";
import { Toolbar } from "./components/Toolbar";
import { useOpenMarkdownPublication } from "./hooks/useOpenMarkdownPublication";
import { executeCleanup } from "./hooks/usePostMergeCleanup";
import { editorTabsStore } from "./stores/editorTabs";
import { getAppTheme } from "./themes";

const ActivityDashboard = lazy(() =>
	import("./components/ActivityDashboard").then((m) => ({ default: m.ActivityDashboard })),
);

const TunnelsPanel = lazy(() => import("./components/TunnelsPanel").then((m) => ({ default: m.TunnelsPanel })));

import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { DictationToast } from "./components/DictationToast/DictationToast";
import { ErrorLogPanel } from "./components/ErrorLogPanel";
import { McpPopup } from "./components/McpPopup/McpPopup";
import { MobileViewBanner } from "./components/MobileViewBanner";
import { ToastContainer } from "./components/ToastContainer/ToastContainer";
import { type WorktreeActions, WorktreeManager } from "./components/WorktreeManager";
import { createBackgroundGitRunner } from "./hooks/git/createBackgroundGitRunner";
import { getWorkItemLinkPort, startWork } from "./hooks/git/startWork";
import { useActiveTerminalSync } from "./hooks/useActiveTerminalSync";
import { useAgentDetection } from "./hooks/useAgentDetection";
import { useAgentPolling } from "./hooks/useAgentPolling";
import { useAppBootstrap } from "./hooks/useAppBootstrap";
import { useAppearanceSync } from "./hooks/useAppearanceSync";
import { useAppShortcutHandlers } from "./hooks/useAppShortcutHandlers";
import { useAutoDeleteBranch } from "./hooks/useAutoDeleteBranch";
import { useCiHeal } from "./hooks/useCiHeal";
import { useCommandPaletteActions } from "./hooks/useCommandPaletteActions";
import { useConfirmDialog } from "./hooks/useConfirmDialog";
import { useDetachedPanelBridge } from "./hooks/useDetachedPanelBridge";
import { useDialogIntegrations } from "./hooks/useDialogIntegrations";
import { useDictation } from "./hooks/useDictation";
import { useDictationHotkey } from "./hooks/useDictationHotkey";
import { useFileOpenBridge } from "./hooks/useFileOpenBridge";
import { useFocusRestore } from "./hooks/useFocusRestore";
import { useFocusTracker } from "./hooks/useFocusTracker";
import { useGitOperations } from "./hooks/useGitOperations";
import { useKeyboardRedirect } from "./hooks/useKeyboardRedirect";
import { useNativeMenuBridge } from "./hooks/useNativeMenuBridge";
import { usePluginContextActions } from "./hooks/usePluginContextActions";
import { usePluginRuntime } from "./hooks/usePluginRuntime";
import { usePty } from "./hooks/usePty";
import { useQuickSwitcher } from "./hooks/useQuickSwitcher";
import { useQuickSwitcherVisibility } from "./hooks/useQuickSwitcherVisibility";
import { useRepository } from "./hooks/useRepository";
import { useShortcutRegistration } from "./hooks/useShortcutRegistration";
import { useSmartPrompts } from "./hooks/useSmartPrompts";
import { useSplitPanes } from "./hooks/useSplitPanes";
import { useSystemLifecycle } from "./hooks/useSystemLifecycle";
import { useTerminalCompletionNotifications } from "./hooks/useTerminalCompletionNotifications";
import { useTerminalContextMenus } from "./hooks/useTerminalContextMenus";
import { useTerminalLifecycle } from "./hooks/useTerminalLifecycle";
import { useTerminalReattachBridge } from "./hooks/useTerminalReattachBridge";
import { useTerminalShellExit } from "./hooks/useTerminalShellExit";
import { useWorktreeConsolidation } from "./hooks/useWorktreeConsolidation";
import { useWorktreeSwitchPrompt } from "./hooks/useWorktreeSwitchPrompt";
import { t } from "./i18n";
import { invoke, listen } from "./invoke";
import { activityPanelAdapter } from "./panelAdapters/activity";
import { aiChatPanelAdapter } from "./panelAdapters/aiChat";
import { fileBrowserPanelAdapter } from "./panelAdapters/fileBrowser";
import { gitPanelAdapter } from "./panelAdapters/git";
import { ideasPanelAdapter } from "./panelAdapters/ideas";
import { markdownPanelAdapter } from "./panelAdapters/markdown";
import { createMarkdownDocumentPanelAdapter } from "./panelAdapters/markdownDocument";
import { outlinePanelAdapter } from "./panelAdapters/outline";
import { storiesPanelAdapter } from "./panelAdapters/stories";
import { detachPanel, reattachPanel, registerPanel, renderPanelMode, togglePanel } from "./panelRouter";
import { activityDashboardStore } from "./stores/activityDashboard";
import { activityStore } from "./stores/activityStore";
import { agentConfigsStore } from "./stores/agentConfigs";
import { appLogger } from "./stores/appLogger";
import { contextMenuActionsStore } from "./stores/contextMenuActionsStore";
import { dictationStore } from "./stores/dictation";
import { diffTabsStore } from "./stores/diffTabs";
import { errorLogStore } from "./stores/errorLog";
import { githubStore } from "./stores/github";
import { globalWorkspaceStore } from "./stores/globalWorkspace";
import { ideasStore } from "./stores/ideas";
import { keybindingsStore } from "./stores/keybindings";
import { mdTabsStore } from "./stores/mdTabs";
import { notificationsStore } from "./stores/notifications";
import { paneLayoutStore } from "./stores/paneLayout";
import { pluginStore } from "./stores/pluginStore";
import { prNotificationsStore } from "./stores/prNotifications";
import { promptLibraryStore } from "./stores/promptLibrary";
import { repoDefaultsStore } from "./stores/repoDefaults";
import { repoSettingsStore } from "./stores/repoSettings";
import { repositoriesStore } from "./stores/repositories";
import { settingsStore } from "./stores/settings";
import { tasksStore } from "./stores/tasks";
import { terminalsStore } from "./stores/terminals";
import { TOAST_ACTIVITY_SECTION_ID } from "./stores/toasts";
import { uiStore } from "./stores/ui";
import { updaterStore } from "./stores/updater";
import { userActivityStore } from "./stores/userActivity";
import { worktreeManagerStore } from "./stores/worktreeManager";
import { isTauri } from "./transport";
import { openFileAction, openTerminalFilePath } from "./utils/filePreview";
import { markdownDocumentPanelId } from "./utils/markdownDocumentPanelId";
import { navigateToTerminal } from "./utils/navigateToTerminal";
import { initPaneTabAssignment } from "./utils/paneTabAssign";
import { quickSwitcherHints } from "./utils/quickSwitcherHints";

const getDefaultFontSize = () => settingsStore.state.defaultFontSize;
const getMaxTabNameLength = () => settingsStore.state.maxTabNameLength;

/** Detect secondary window mode via URL query param */
const isSecondaryWindow = () => new URLSearchParams(window.location.search).get("mode") === "secondary";

registerPanel(activityPanelAdapter);
registerPanel(gitPanelAdapter);
registerPanel(fileBrowserPanelAdapter);
registerPanel(markdownPanelAdapter);
registerPanel(ideasPanelAdapter);
registerPanel(outlinePanelAdapter);
registerPanel(storiesPanelAdapter);
registerPanel(aiChatPanelAdapter);

const App: Component = () => {
	// Detached panel mode: full-viewport single panel.
	// Must be checked FIRST — before any createEffect/onMount registrations —
	// otherwise panel windows run initApp() (re-adopts PTY sessions → "Terminal 1"
	// flashing), sync providers, keybindings, and every other main-window effect.
	const panelEl = renderPanelMode();
	if (panelEl) return panelEl;

	const [statusInfo, _setStatusInfoRaw] = createSignal("Ready");
	let statusInfoTimer: ReturnType<typeof setTimeout> | null = null;
	const setStatusInfo = (text: string) => {
		if (statusInfoTimer) clearTimeout(statusInfoTimer);
		_setStatusInfoRaw(text);
		if (text !== "Ready" && text !== "") {
			statusInfoTimer = setTimeout(() => _setStatusInfoRaw("Ready"), 30_000);
		}
	};
	// Production: used by CanvasTerminal for status feedback
	(window as unknown as Record<string, unknown>).__tuic_setStatusInfo = setStatusInfo;
	if (import.meta.env.DEV) {
		(window as unknown as Record<string, unknown>).__debug = {
			// Stores
			terminals: terminalsStore,
			repositories: repositoriesStore,
			settings: settingsStore,
			github: githubStore,
			prompts: promptLibraryStore,
			agentConfigs: agentConfigsStore,
			plugins: pluginStore,
			ui: uiStore,
			notifications: notificationsStore,
			activity: activityStore,
			activityDashboard: activityDashboardStore,
			repoSettings: repoSettingsStore,
			repoDefaults: repoDefaultsStore,
			diffTabs: diffTabsStore,
			mdTabs: mdTabsStore,
			editorTabs: editorTabsStore,
			notes: ideasStore,
			keybindings: keybindingsStore,
			prNotifications: prNotificationsStore,
			updater: updaterStore,
			tasks: tasksStore,
			showWhatsNew: (v: string) => setWhatsNewVersion(v),
			dictation: dictationStore,
			userActivity: userActivityStore,
			contextMenuActions: contextMenuActionsStore,
			worktreeManager: worktreeManagerStore,
			errorLog: errorLogStore,
			logger: appLogger,
			// Tauri bridge
			invoke,
			listen,
			isTauri,
		};
	}
	const [settingsPanelVisible, setSettingsPanelVisible] = createSignal(false);
	const [settingsInitialTab, setSettingsInitialTab] = createSignal<string | undefined>(undefined);
	const [settingsInitialSection, setSettingsInitialSection] = createSignal<string | undefined>(undefined);
	const [settingsContext, setSettingsContext] = createSignal<SettingsContext>({ kind: "global" });

	/** `section` is the DOM id of a block to scroll to — see SettingsPanel/sections.ts */
	const openSettings = (tab?: string, section?: string) => {
		setSettingsContext({ kind: "global" });
		setSettingsInitialTab(tab);
		setSettingsInitialSection(section);
		setSettingsPanelVisible(true);
	};
	const [taskQueueVisible, setTaskQueueVisible] = createSignal(false);

	// Help panel state (Story 053)
	const [helpPanelVisible, setHelpPanelVisible] = createSignal(false);

	// Quit confirmation state (Story 057)

	// Quick switcher state
	const [quickSwitcherVisible, setQuickSwitcherVisible] = createSignal(false);
	const [tabHintsVisible, setTabHintsVisible] = createSignal(false);

	// Rename branch dialog state
	const [renameBranchDialogVisible, setRenameBranchDialogVisible] = createSignal(false);
	// Create branch dialog state
	const [createBranchDialogVisible, setCreateBranchDialogVisible] = createSignal(false);

	// Run command dialog state
	const [runCommandDialogVisible, setRunCommandDialogVisible] = createSignal(false);

	// Background git operations tracking (for button loading states)
	const [runningGitOps, setRunningGitOps] = createSignal<Set<string>>(new Set());

	// Terminal rename prompt state
	const [termRenamePromptVisible, setTermRenamePromptVisible] = createSignal(false);
	const [termRenameDefault, setTermRenameDefault] = createSignal("");
	const [repoPathPromptVisible, setRepoPathPromptVisible] = createSignal(false);
	let repoPathPromptResolve: ((value: string | null) => void) | null = null;

	/** Browse the serving machine for a repo path (browser mode only) */
	const promptRepoPath = (): Promise<string | null> => {
		const { promise, resolve, reject } = Promise.withResolvers<string | null>();
		try {
			repoPathPromptResolve = resolve;
			setRepoPathPromptVisible(true);
		} catch (error) {
			reject(error);
		}
		return promise;
	};
	const resolveRepoPathPrompt = (value: string | null) => {
		setRepoPathPromptVisible(false);
		repoPathPromptResolve?.(value);
		repoPathPromptResolve = null;
	};

	/** Which machine the remote repo picker is browsing; null when it is closed. */
	const [remotePickerConnectionId, setRemotePickerConnectionId] = createSignal<string | null>(null);
	let remotePickerResolve: ((value: string | null) => void) | null = null;

	/** Open the remote browser and resolve with the absolute path picked on that machine. */
	const pickRemoteRepoPath = (connectionId: string): Promise<string | null> => {
		const { promise, resolve, reject } = Promise.withResolvers<string | null>();
		try {
			remotePickerResolve = resolve;
			setRemotePickerConnectionId(connectionId);
		} catch (error) {
			reject(error);
		}
		return promise;
	};
	const resolveRemotePicker = (value: string | null) => {
		setRemotePickerConnectionId(null);
		remotePickerResolve?.(value);
		remotePickerResolve = null;
	};

	// Arbitrary path prompt — powers "Open Path…" to route files to the viewer/editor
	// and folders into the file browser.
	const [openPathPromptVisible, setOpenPathPromptVisible] = createSignal(false);
	let openPathPromptResolve: ((value: string | null) => void) | null = null;
	const promptOpenPath = (): Promise<string | null> => {
		const { promise, resolve, reject } = Promise.withResolvers<string | null>();
		try {
			openPathPromptResolve = resolve;
			setOpenPathPromptVisible(true);
		} catch (error) {
			reject(error);
		}
		return promise;
	};
	const resolveOpenPathPrompt = (value: string | null) => {
		setOpenPathPromptVisible(false);
		openPathPromptResolve?.(value);
		openPathPromptResolve = null;
	};

	// Context menu state
	const contextMenu = createContextMenu();

	const pty = usePty();
	const repo = useRepository();
	const dialogs = useConfirmDialog();

	const [showProcessManager, setShowProcessManager] = createSignal(false);
	const [showGenerators, setShowGenerators] = createSignal(false);
	const [showRemoteQr, setShowRemoteQr] = createSignal(false);
	const [whatsNewVersion, setWhatsNewVersion] = createSignal<string | null>(null);

	// Drop on folder: when source is a directory, we pause to ask for confirmation.
	const [pendingFolderDrop, setPendingFolderDrop] = createSignal<
		import("./hooks/useFileDrop").FolderDropRequest | null
	>(null);
	useDialogIntegrations({ setPendingFolderDrop, confirm: (options) => dialogs.confirm(options) });

	// Redirect keyboard input from sidebar to terminal
	useKeyboardRedirect(true);

	// Centralized focus restore: when any dialog/overlay closes (DOM removed),
	// focus returns to the active terminal automatically.
	useFocusRestore();

	// Remember last-focused element per-repo + globally, and restore it on
	// repo switch / window focus regain.
	useFocusTracker();

	// Keep each repo's consolidated worktree view in sync with its worktrees,
	// and show it when the active repo has the toggle on (#e767).
	useWorktreeConsolidation();

	const terminalLifecycle = useTerminalLifecycle({
		pty,
		dialogs,
		setStatusInfo,
		getDefaultFontSize,
	});

	const gitOps = useGitOperations({
		repo,
		pty,
		dialogs: {
			...dialogs,
			promptRepoPath,
			pickRemoteRepoPath,
			confirmOrphanCleanup: dialogs.confirmOrphanCleanup,
			confirmRemoveLockedWorktree: dialogs.confirmRemoveLockedWorktree,
			confirmDirtyWorktreeCleanup: dialogs.confirmDirtyWorktreeCleanup,
		},
		closeTerminal: terminalLifecycle.closeTerminal,
		createNewTerminal: terminalLifecycle.createNewTerminal,
		setStatusInfo,
		getDefaultFontSize,
		getMaxTabNameLength,
		getPromptOnCreate: (repoPath: string) => {
			const effective = repoSettingsStore.getEffective(repoPath);
			return effective?.promptOnCreate ?? repoDefaultsStore.state.promptOnCreate;
		},
	});

	// ── Post-merge worktree cleanup dialog state ──
	const [worktreeCleanupExecuting, setWorktreeCleanupExecuting] = createSignal(false);
	const [worktreeCleanupStepStatuses, setWorktreeCleanupStepStatuses] = createSignal<
		Partial<Record<StepId, StepStatus>>
	>({});
	const [worktreeCleanupStepErrors, setWorktreeCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});
	const [worktreeCleanupStepNotes, setWorktreeCleanupStepNotes] = createSignal<Partial<Record<StepId, string>>>({});
	const [worktreeCleanupAction, setWorktreeCleanupAction] = createSignal<"archive" | "delete">("archive");

	const handleWorktreeCleanupExecute = async (steps: CleanupStep[], options?: { unstash?: boolean }) => {
		const ctx = gitOps.mergePendingCtx();
		if (!ctx) return;
		setWorktreeCleanupExecuting(true);
		setWorktreeCleanupStepStatuses({});
		setWorktreeCleanupStepErrors({});
		setWorktreeCleanupStepNotes({});
		await executeCleanup({
			repoPath: ctx.repoPath,
			workspaceId: ctx.workspaceId,
			branchName: ctx.branchName,
			baseBranch: ctx.baseBranch,
			steps: steps.map((s) => ({ id: s.id, checked: s.checked })),
			worktreeAction: worktreeCleanupAction(),
			worktreeDirty: ctx.worktreeDirty,
			worktreeFingerprint: ctx.worktreeFingerprint,
			unstash: options?.unstash,
			onStepStart: (id) => setWorktreeCleanupStepStatuses((prev) => ({ ...prev, [id]: "running" as StepStatus })),
			onStepDone: (id, result, error) => {
				setWorktreeCleanupStepStatuses((prev) => ({ ...prev, [id]: result as StepStatus }));
				if (error) setWorktreeCleanupStepErrors((prev) => ({ ...prev, [id]: error }));
			},
			onStepNote: (id, note) => setWorktreeCleanupStepNotes((prev) => ({ ...prev, [id]: note })),
			closeTerminalsForBranch: gitOps.closeTerminalsForBranch,
		});
		// Brief delay so user sees final statuses
		setTimeout(() => {
			setWorktreeCleanupExecuting(false);
			gitOps.dismissMergePending();
		}, 600);
	};

	const handleWorktreeCleanupSkip = () => {
		gitOps.dismissMergePending();
	};

	const dictation = useDictation({
		pty,
		dictation: dictationStore,
		setStatusInfo,
		openSettings,
	});

	const quickSwitcher = useQuickSwitcher({
		handleBranchSelect: gitOps.handleBranchSelect,
	});

	const splitPanes = useSplitPanes();

	// Single decision for every "close terminal / close tab" entry point
	// (keyboard, command palette, native menu). In split mode close the active
	// pane — which cancels a freshly-split empty pane instead of destroying a
	// terminal; otherwise close the active terminal tab.
	const closeActiveTabOrPane = () => {
		if (paneLayoutStore.isSplit()) {
			splitPanes.closeActivePane();
		} else {
			const activeId = terminalsStore.state.activeId;
			if (activeId) terminalLifecycle.closeTerminal(activeId);
		}
	};

	// Register pane tab auto-assignment hook (must be after store imports)
	initPaneTabAssignment();

	// Poll active terminal for foreground agent detection
	useAgentPolling();

	// Agent detection for context menu
	const agentDetection = useAgentDetection();
	const smartPrompts = useSmartPrompts();

	// Auto-delete local branches when their PR is merged/closed
	useAutoDeleteBranch({ confirm: (opts) => dialogs.confirm(opts), setStatusInfo });

	// Offer to switch to newly created worktrees (from MCP) + activity notification,
	// and prune the sidebar row when a worktree is removed backend-side.
	useWorktreeSwitchPrompt({
		handleBranchSelect: gitOps.handleBranchSelect,
		closeTerminalsForBranch: gitOps.closeTerminalsForBranch,
	});

	// Register built-in activity sections for git and worktree notifications
	activityStore.registerSection({ id: "terminals", label: "TERMINALS", priority: 10, canDismissAll: true });
	activityStore.registerSection({
		id: TOAST_ACTIVITY_SECTION_ID,
		label: "MESSAGES",
		priority: 20,
		canDismissAll: true,
	});
	activityStore.registerSection({ id: "git-ops", label: "GIT", priority: 30, canDismissAll: true });
	activityStore.registerSection({ id: "worktrees", label: "WORKTREES", priority: 40, canDismissAll: true });

	// Auto-heal CI failures by injecting logs into agent terminals
	useCiHeal();

	usePluginContextActions(smartPrompts);
	usePluginRuntime();
	useSystemLifecycle();

	const detachedPanelBridge = useDetachedPanelBridge(() => openSettings("general", SETTINGS_SECTION_EGO));

	// Notification sounds are now played natively via Rust (rodio) —
	// no Web Audio warmup needed.

	useAppBootstrap({
		pty,
		setStatusInfo,
		handleBranchSelect: gitOps.handleBranchSelect,
		refreshAllBranchStats: gitOps.refreshAllBranchStats,
		getDefaultFontSize,
		detectAgents: agentDetection.detectAll,
		restoreDetachedPanels: detachedPanelBridge.restoreDetachedPanels,
		resumeAgentTabsAfterUpdate: gitOps.resumeAgentTabsAfterUpdate,
		setWhatsNewVersion,
		openSettings,
		openRepoPath: gitOps.addRepoByPath,
		confirm: (options) => dialogs.confirm(options),
	});

	useAppearanceSync();

	useActiveTerminalSync();
	useOpenMarkdownPublication();

	useTerminalShellExit(terminalLifecycle.closeTerminal);

	useTerminalCompletionNotifications({ navigateToTerminal });

	const [promptInspectorSessionId, setPromptInspectorSessionId] = createSignal<string | null>(null);
	const terminalContextMenus = useTerminalContextMenus({
		openPromptInspector: setPromptInspectorSessionId,
		agentDetection,
		gitOps,
		splitPanes,
		terminalLifecycle,
		closeActiveTabOrPane,
		setTermRenameDefault,
		setTermRenamePromptVisible,
	});

	/**
	 * Reveal an arbitrary folder in the FileBrowser panel. Sets an ephemeral
	 * external root so browsing works outside the active repo — git badges and
	 * gitignore integration degrade gracefully when the folder isn't a repo.
	 */
	const revealFolderInBrowser = (absolutePath: string) => {
		uiStore.setFileBrowserExternalRoot(absolutePath);
		uiStore.setFileBrowserPanelVisible(true);
	};

	/** Open a file path from terminal output — .md/.mdx in MD viewer, others in internal editor */
	const handleOpenFilePath = (absolutePath: string, line?: number, col?: number) => {
		// Scoped to the repo that owns the PATH. It used to relativize against the
		// active worktree, so a path printed by an agent working in another repo
		// opened as a tab filed under whichever repo the user happened to be on.
		openTerminalFilePath(
			absolutePath,
			(tabId) => {
				terminalLifecycle.handleTerminalSelect(tabId);
			},
			line,
			col,
		);
	};

	useFileOpenBridge();

	const handleBackgroundGit = createBackgroundGitRunner({ setStatusInfo, runningGitOps, setRunningGitOps });

	/** Detach a terminal tab to a floating OS window */
	const handleDetachTab = async (tabId: string) => {
		if (!isTauri()) return;
		if (tabId.startsWith("md-")) {
			if (mdTabsStore.get(tabId)?.type !== "file") return;
			const adapter = createMarkdownDocumentPanelAdapter(tabId, terminalLifecycle.handleTerminalSelect);
			registerPanel(adapter);
			await detachPanel(adapter.id);
			return;
		}
		const term = terminalsStore.get(tabId);
		if (!term?.sessionId) return;

		const windowLabel = `floating-${tabId}`;
		const url = `index.html#/floating?sessionId=${encodeURIComponent(term.sessionId)}&tabId=${encodeURIComponent(tabId)}&name=${encodeURIComponent(term.name)}`;

		const floatingWin = new WebviewWindow(windowLabel, {
			url,
			title: term.name || "Terminal",
			width: 800,
			height: 600,
			center: true,
			decorations: true,
			backgroundColor: getAppTheme(settingsStore.state.theme).bgPrimary,
		});

		floatingWin.once("tauri://destroyed", () => {
			if (terminalsStore.isDetached(tabId)) {
				reattachFallback(tabId);
				setTimeout(() => {
					const ref = terminalsStore.get(tabId)?.ref;
					if (ref) {
						ref.refresh();
						ref.fit();
					}
				}, 150);
			}
		});

		terminalsStore.detach(tabId, windowLabel);

		// Switch to next available tab
		const ids = terminalLifecycle.terminalIds().filter((id) => !terminalsStore.isDetached(id));
		if (ids.length > 0) {
			terminalLifecycle.handleTerminalSelect(ids[0]);
		}
	};

	const reattachFallback = (tabId: string) => {
		terminalsStore.reattach(tabId);
		terminalLifecycle.handleTerminalSelect(tabId);
	};

	/** Focus a detached terminal's floating window */
	const handleFocusDetachedTab = async (tabId: string) => {
		if (!isTauri()) return;
		if (tabId.startsWith("md-")) {
			togglePanel(markdownDocumentPanelId(tabId));
			return;
		}
		const windowLabel = terminalsStore.state.detachedWindows[tabId];
		if (!windowLabel) return;
		try {
			const win = await WebviewWindow.getByLabel(windowLabel);
			if (win) {
				await win.unminimize();
				await win.setFocus();
			} else {
				appLogger.info("terminal", `Floating window ${windowLabel} gone, reattaching ${tabId}`);
				reattachFallback(tabId);
			}
		} catch (err) {
			appLogger.warn("terminal", `Error focusing floating window ${windowLabel}, reattaching`, err);
			reattachFallback(tabId);
		}
	};

	/** Reattach a detached terminal by closing its floating window */
	const handleReattachTab = async (tabId: string) => {
		if (!isTauri()) return;
		if (tabId.startsWith("md-")) {
			await reattachPanel(markdownDocumentPanelId(tabId));
			return;
		}
		const windowLabel = terminalsStore.state.detachedWindows[tabId];
		if (!windowLabel) return;
		try {
			const win = await WebviewWindow.getByLabel(windowLabel);
			await win?.close();
		} catch (err) {
			appLogger.warn("terminal", `Failed to close floating window ${windowLabel}`, err);
		}
		reattachFallback(tabId);
	};

	useTerminalReattachBridge({ reattach: reattachFallback, setStatusInfo });

	useQuickSwitcherVisibility(setQuickSwitcherVisible, setTabHintsVisible);
	const shortcutHandlers = useAppShortcutHandlers({
		terminalLifecycle,
		gitOps,
		splitPanes,
		quickSwitcher,
		quickSwitcherVisible,
		closeActiveTabOrPane,
		setRunCommandDialogVisible,
		setSettingsPanelVisible,
		setTaskQueueVisible,
		setHelpPanelVisible,
		setShowProcessManager,
		setShowGenerators,
		setShowRemoteQr,
		promptOpenPath,
		handleOpenFilePath,
		revealFolderInBrowser,
		executeSmartPrompt: smartPrompts.executeSmartPrompt,
	});

	// Worktree manager action callbacks. Every one of them addresses a WORKSPACE:
	// the manager rows are workspaces, and `mainBranch` below is the merge target,
	// which is a ref and stays a branch name.
	const worktreeActions: WorktreeActions = {
		onOpenTerminal: (repoPath, workspaceId) => {
			void gitOps.handleAddTerminalToWorkspace(repoPath, workspaceId);
		},
		onDelete: (repoPath, workspaceId) => {
			void gitOps.handleRemoveWorkspace(repoPath, workspaceId);
		},
		onMergeAndArchive: (repoPath, workspaceId) => {
			void gitOps.handleMergeAndArchiveIntoMain(repoPath, workspaceId);
		},
	};

	const actionEntries = useCommandPaletteActions({
		shortcutHandlers,
		gitOps,
		splitPanes,
		executeSmartPrompt: smartPrompts.executeSmartPrompt,
	});

	const paletteUrlActions = (text: string, signal: AbortSignal) =>
		urlPasteActions(
			text,
			{
				activeRepoPath: () => repositoriesStore.state.activeRepoPath,
				links: getWorkItemLinkPort,
				workspaceIdOnBranch: (repoPath, branch) => repositoriesStore.workspaceIdOnBranch(repoPath, branch),
				openWorkspace: (repoPath, workspaceId) => {
					repositoriesStore.setActive(repoPath);
					void gitOps.handleBranchSelect(repoPath, workspaceId);
				},
				startWork,
				notify: setStatusInfo,
			},
			signal,
		);

	useShortcutRegistration(shortcutHandlers);

	useNativeMenuBridge({
		shortcutHandlers,
		terminalLifecycle,
		gitOps,
		splitPanes,
		closeActiveTabOrPane,
		setSettingsPanelVisible,
		setRunCommandDialogVisible,
		setTaskQueueVisible,
		setShowProcessManager,
		setHelpPanelVisible,
	});
	useDictationHotkey({
		onStart: () => void dictation.handleDictationStart(dictationStore.state.hotkey === "Fn" ? "fn" : "hotkey"),
		onStop: dictation.handleDictationStop,
	});

	// Secondary window: minimal pane-only layout
	if (isSecondaryWindow()) {
		return (
			<div id="app" class="sidebar-hidden secondary-window">
				<div id="app-body">
					<main id="main">
						<TerminalArea
							onTerminalFocus={terminalLifecycle.handleTerminalFocus}
							onCloseTab={terminalLifecycle.closeTerminal}
							onOpenFilePath={handleOpenFilePath}
							onContextMenu={contextMenu.open}
							onCwdChange={gitOps.handleTerminalCwdChange}
							onNewTerminal={(groupId) => {
								paneLayoutStore.setActiveGroup(groupId);
								gitOps.handleNewTab();
							}}
						/>
					</main>
				</div>
			</div>
		);
	}

	return (
		<div
			id="app"
			classList={{
				"sidebar-hidden": !uiStore.state.sidebarVisible,
				"focus-mode": uiStore.state.focusMode,
			}}
		>
			<MobileViewBanner />
			{/* Toolbar - drag region spanning full width */}
			<Toolbar
				repoPath={gitOps.currentRepoPath()}
				runCommand={gitOps.activeRunCommand()}
				onBranchClick={() => {
					const activeRepo = repositoriesStore.getActive();
					if (activeRepo?.activeWorkspaceId) {
						if (gitOps.handleOpenRenameBranchDialog(activeRepo.path, activeRepo.activeWorkspaceId)) {
							setRenameBranchDialogVisible(true);
						}
					}
				}}
				onRun={(shiftKey) => gitOps.handleRunCommand(shiftKey, () => setRunCommandDialogVisible(true))}
				onReviewPr={gitOps.handleReviewPr}
				// Toolbar forwards this straight to SmartPromptsDropdown's
				// "Manage Smart Prompts..." link.
				onOpenSettings={() => openSettings("smart-prompts")}
				onShowWhatsNew={(v) => setWhatsNewVersion(v)}
				onRestartNow={async () => {
					const count = updaterStore.busyCount();
					const ok =
						count === 0 ||
						(await dialogs.confirm({
							title: t("updater.restartNow.title", "Restart to finish the update?"),
							message: t(
								"updater.restartNow.message",
								"{count} busy or waiting session(s) will be interrupted by the restart.",
								{
									count: String(count),
								},
							),
							okLabel: t("updater.restartNow.confirm", "Restart now"),
						}));
					if (ok) await updaterStore.restartNow();
				}}
			/>

			{/* Body: sidebar + main content side by side */}
			<div id="app-body">
				{/* Sidebar - always mounted, hidden via CSS */}
				<Sidebar
					quickSwitcherActive={quickSwitcherHints(quickSwitcherVisible(), globalWorkspaceStore.isActive()).sidebar}
					onBranchSelect={gitOps.handleBranchSelect}
					onAddTerminal={gitOps.handleAddTerminalToWorkspace}
					onRemoveBranch={gitOps.handleRemoveWorkspace}
					onRenameBranch={(repoPath, branchName) => {
						if (gitOps.handleOpenRenameBranchDialog(repoPath, branchName)) {
							setRenameBranchDialogVisible(true);
						}
					}}
					onCreateBranch={(repoPath, fromBranch) => {
						gitOps.handleOpenCreateBranchDialog(repoPath, fromBranch);
						setCreateBranchDialogVisible(true);
					}}
					onAddWorktree={gitOps.handleAddWorktree}
					onCreateWorktreeFromBranch={gitOps.handleCreateWorktreeFromBranch}
					onMergeAndArchive={(repoPath, branchName) => {
						void gitOps.handleMergeAndArchiveIntoMain(repoPath, branchName);
					}}
					creatingWorktreeRepos={gitOps.creatingWorktreeRepos()}
					removingBranches={gitOps.removingBranches()}
					onAddRepo={gitOps.handleAddRepo}
					onAddRemoteRepo={gitOps.handleAddRemoteRepo}
					onRepoSettings={(repoPath) =>
						gitOps.handleRepoSettings(repoPath, (ctx) => {
							setSettingsContext(ctx);
							setSettingsInitialTab(undefined);
							setSettingsPanelVisible(true);
						})
					}
					onRemoveRepo={gitOps.handleRemoveRepo}
					onOpenSettings={() => openSettings()}
					onOpenRemoteMachines={() => openSettings("remote-machines")}
					onOpenHelp={() => setHelpPanelVisible(true)}
					buildAgentMenuItems={terminalContextMenus.buildSidebarAgentMenuItems}
					onRefreshBranchStats={gitOps.refreshAllBranchStats}
					onCheckoutRemoteBranch={gitOps.handleCheckoutRemoteBranch}
					onAutofixIssue={gitOps.handleAutofixIssue}
					onConflictAssist={gitOps.handleConflictAssist}
					onSwitchBranch={gitOps.handleSwitchBranch}
					switchBranchLists={gitOps.switchBranchLists()}
					currentBranches={gitOps.currentBranches()}
					onBackgroundGit={handleBackgroundGit}
					runningGitOps={runningGitOps()}
					onReviewPr={gitOps.handleReviewPr}
				/>

				{/* Main content */}
				<main id="main">
					{/* Tab bar */}
					<div id="tab-bar">
						<TabBar
							tabHintsActive={tabHintsVisible()}
							quickSwitcherActive={quickSwitcherHints(quickSwitcherVisible(), globalWorkspaceStore.isActive()).tabBar}
							onTabSelect={terminalLifecycle.handleTerminalSelect}
							onTabClose={terminalLifecycle.closeTerminal}
							onCloseOthers={terminalLifecycle.closeOtherTabs}
							onCloseToRight={terminalLifecycle.closeTabsToRight}
							onNewTab={gitOps.handleNewTab}
							getNewAgentMenuItems={() => {
								const target = gitOps.resolveNewTabTarget();
								if (!target) return [];
								return terminalContextMenus.buildSidebarAgentMenuItems(target.repoPath, target.branchName);
							}}
							onReorder={(from, to) => {
								const activeRepo = repositoriesStore.getActive();
								if (activeRepo?.activeWorkspaceId) {
									repositoriesStore.reorderTerminals(activeRepo.path, activeRepo.activeWorkspaceId, from, to);
								}
							}}
							onDetachTab={handleDetachTab}
							onReattachTab={handleReattachTab}
							onFocusDetachedTab={handleFocusDetachedTab}
							getWorktreeTargets={gitOps.getWorktreeTargets}
							onMoveToWorktree={gitOps.moveTerminalToWorktree}
						/>
					</div>

					{/* Terminal container - render ALL terminals so they never unmount (preserves PTY sessions) */}
					<TerminalArea
						onTerminalFocus={terminalLifecycle.handleTerminalFocus}
						onCloseTab={terminalLifecycle.closeTerminal}
						onOpenFilePath={handleOpenFilePath}
						onContextMenu={contextMenu.open}
						onCwdChange={gitOps.handleTerminalCwdChange}
						onNewTerminal={(groupId) => {
							paneLayoutStore.setActiveGroup(groupId);
							gitOps.handleNewTab();
						}}
					>
						{/* Side panels (right panes inside #terminal-container) */}
						<PanelOrchestrator
							onOpenSettings={openSettings}
							repoPath={gitOps.currentRepoPath() || null}
							fsRoot={gitOps.activeWorktreePath() || null}
							onFileOpen={(fsRoot, filePath, line) => {
								const repoPath = gitOps.currentRepoPath() || fsRoot;
								openFileAction(filePath, repoPath, fsRoot || undefined, line, (tabId) => {
									terminalLifecycle.handleTerminalSelect(tabId);
								});
							}}
						/>
					</TerminalArea>

					{/* Status bar */}
					<StatusBar
						fontSize={terminalLifecycle.activeFontSize()}
						defaultFontSize={getDefaultFontSize()}
						statusInfo={statusInfo()}
						onToggleDiff={() => togglePanel("git")}
						onToggleMarkdown={() => uiStore.toggleMarkdownPanel()}
						onToggleNotes={() => uiStore.toggleIdeasPanel()}
						onToggleFileBrowser={() => uiStore.toggleFileBrowserPanel()}
						onToggleAiChat={() => togglePanel("ai-chat")}
						onToggleErrorLog={() => errorLogStore.toggle()}
						onDictationStart={dictation.handleDictationStart}
						onDictationStop={dictation.handleDictationStop}
						currentRepoPath={
							globalWorkspaceStore.isActive()
								? terminalsStore.state.activeId
									? (repositoriesStore.getRepoPathForTerminal(terminalsStore.state.activeId) ?? undefined)
									: undefined
								: gitOps.currentRepoPath()
						}
						cwd={terminalsStore.getActive()?.cwd || gitOps.activeWorktreePath()}
						repoRoot={gitOps.activeWorktreePath()}
						onBranchRenamed={(oldName, newName) => {
							const repoPath = gitOps.currentRepoPath();
							if (repoPath) {
								repositoriesStore.renameBranch(repoPath, oldName, newName);
							}
							setStatusInfo(`Renamed branch ${oldName} to ${newName}`);
						}}
						onReviewPr={gitOps.handleReviewPr}
					/>
				</main>
			</div>

			{/* Prompt overlay */}
			<PromptOverlay />

			{/* AI knowledge history overlay */}

			{/* Dictation streaming toast — shows partial transcription */}
			<DictationToast />
			<ToastContainer />

			{/* Prompt library drawer */}
			<PromptDrawer />

			{/* Browser mode uses the same palette with its HTTP-safe action subset. */}
			<CommandPalette actions={actionEntries()} browserMode={!isTauri()} urlActions={paletteUrlActions} />

			{/* Quick branch switcher */}
			<BranchSwitcher
				activeRepoPath={repositoriesStore.state.activeRepoPath ?? undefined}
				onSelect={(repoPath, branchName) => {
					// The switcher lists BRANCHES, so this resolves through the single
					// branch->id seam. A hit means some workspace already has that branch
					// checked out and switching is a UI move; a miss means a real
					// `git checkout` in the main worktree.
					const workspaceId = repositoriesStore.workspaceIdOnBranch(repoPath, branchName);
					if (workspaceId) {
						gitOps.handleBranchSelect(repoPath, workspaceId);
					} else {
						gitOps.handleSwitchBranch(repoPath, branchName);
					}
				}}
				onCheckoutRemote={gitOps.handleCheckoutRemoteBranch}
			/>

			<PromptInspector sessionId={promptInspectorSessionId()} onClose={() => setPromptInspectorSessionId(null)} />
			{/* Activity dashboard — unmount when closed to release memos/subscriptions */}
			<Show when={!uiStore.isDetached("activity") && activityDashboardStore.state.isOpen}>
				<Suspense>
					<ActivityDashboard onSelect={terminalLifecycle.handleTerminalSelect} />
				</Suspense>
			</Show>

			{/* SSH Tunnels panel (experimental) */}
			<Show when={settingsStore.state.experimentalFeaturesEnabled}>
				<Suspense>
					<TunnelsPanel />
				</Suspense>
			</Show>

			{/* Worktree manager */}
			<WorktreeManager actions={worktreeActions} />

			{/* MCP servers popup (per-repo) */}
			<McpPopup onOpenSettings={openSettings} />

			{/* Error log panel */}
			<ErrorLogPanel />

			<ApplicationOverlays
				panels={{
					settingsVisible: settingsPanelVisible,
					closeSettings: () => setSettingsPanelVisible(false),
					settingsInitialTab,
					settingsInitialSection,
					settingsContext,
					taskQueueVisible,
					closeTaskQueue: () => setTaskQueueVisible(false),
					helpVisible: helpPanelVisible,
					closeHelp: () => setHelpPanelVisible(false),
				}}
				contextMenu={contextMenu}
				getContextMenuItems={terminalContextMenus.getContextMenuItems}
				git={{
					renameVisible: renameBranchDialogVisible,
					closeRename: () => {
						setRenameBranchDialogVisible(false);
						gitOps.setBranchToRename(null);
					},
					branchToRename: gitOps.branchToRename,
					onRename: gitOps.handleRenameBranch,
					createVisible: createBranchDialogVisible,
					closeCreate: () => {
						setCreateBranchDialogVisible(false);
						gitOps.setBranchToCreate(null);
					},
					branchToCreate: gitOps.branchToCreate,
					onCreate: gitOps.handleCreateBranch,
					worktreeState: gitOps.worktreeDialogState,
					closeWorktree: () => gitOps.setWorktreeDialogState(null),
					onGenerateWorktreeName: gitOps.generateWorktreeName,
					onCreateWorktree: gitOps.confirmCreateWorktree,
					runVisible: runCommandDialogVisible,
					closeRun: () => setRunCommandDialogVisible(false),
					activeRunCommand: gitOps.activeRunCommand,
					onRun: gitOps.executeRunCommand,
				}}
				prompts={{
					terminalRenameVisible: termRenamePromptVisible,
					closeTerminalRename: () => setTermRenamePromptVisible(false),
					terminalRenameDefault: termRenameDefault,
					openPathVisible: openPathPromptVisible,
					resolveOpenPath: resolveOpenPathPrompt,
					repoPathVisible: repoPathPromptVisible,
					resolveRepoPath: resolveRepoPathPrompt,
					remotePickerConnectionId,
					resolveRemotePicker,
				}}
				confirmations={{
					dialogState: dialogs.dialogState,
					onClose: dialogs.handleClose,
					onConfirm: dialogs.handleConfirm,
					onDiscard: dialogs.handleDiscard,
					pendingFolderDrop,
					setPendingFolderDrop,
				}}
				utilities={{
					processManagerVisible: showProcessManager,
					closeProcessManager: () => setShowProcessManager(false),
					generatorsVisible: showGenerators,
					closeGenerators: () => setShowGenerators(false),
					remoteQrVisible: showRemoteQr,
					closeRemoteQr: () => setShowRemoteQr(false),
					whatsNewVersion,
					setWhatsNewVersion,
				}}
				cleanup={{
					context: gitOps.mergePendingCtx,
					action: worktreeCleanupAction,
					setAction: setWorktreeCleanupAction,
					executing: worktreeCleanupExecuting,
					stepStatuses: worktreeCleanupStepStatuses,
					stepErrors: worktreeCleanupStepErrors,
					stepNotes: worktreeCleanupStepNotes,
					onExecute: handleWorktreeCleanupExecute,
					onSkip: handleWorktreeCleanupSkip,
				}}
			/>
		</div>
	);
};

export default App;
