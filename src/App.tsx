import {
  Component,
  Show,
  Suspense,
  createEffect,
  createMemo,
  createSignal,
  lazy,
  on,
  onCleanup,
  onMount,
} from "solid-js";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { TabBar } from "./components/TabBar";
import { StatusBar } from "./components/StatusBar";
import { TerminalArea } from "./components/TerminalArea";
import { PanelOrchestrator } from "./components/PanelOrchestrator";
import { editorTabsStore } from "./stores/editorTabs";
import { PromptOverlay } from "./components/PromptOverlay";
import { PromptDrawer } from "./components/PromptDrawer";
import type { SettingsContext } from "./components/SettingsPanel";
const SettingsPanel = lazy(() => import("./components/SettingsPanel").then(m => ({ default: m.SettingsPanel })));
import { TaskQueuePanel } from "./components/TaskQueuePanel";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "./components/ContextMenu";
import { RenameBranchDialog } from "./components/RenameBranchDialog";
import { CreateWorktreeDialog } from "./components/CreateWorktreeDialog";
import { PromptDialog } from "./components/PromptDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { UpdateProgressDialog } from "./components/UpdateProgressDialog";
import { PostMergeCleanupDialog, type StepId, type StepStatus, type CleanupStep } from "./components/PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "./hooks/usePostMergeCleanup";
import { getCompletionSuppression } from "./components/Terminal/completionDecision";
import { RunCommandDialog } from "./components/RunCommandDialog";
const HelpPanel = lazy(() => import("./components/HelpPanel").then(m => ({ default: m.HelpPanel })));
import { CommandPalette } from "./components/CommandPalette";
import { BranchSwitcher } from "./components/BranchSwitcher/BranchSwitcher";
const ActivityDashboard = lazy(() => import("./components/ActivityDashboard").then(m => ({ default: m.ActivityDashboard })));
import { WorktreeManager, type WorktreeActions } from "./components/WorktreeManager";
import { ErrorLogPanel } from "./components/ErrorLogPanel";
import { McpPopup } from "./components/McpPopup/McpPopup";
import { DictationToast } from "./components/DictationToast/DictationToast";
import { commandPaletteStore } from "./stores/commandPalette";
import { branchSwitcherStore } from "./stores/branchSwitcher";
import { mcpPopupStore } from "./stores/mcpPopup";
import { activityDashboardStore } from "./stores/activityDashboard";
import { worktreeManagerStore } from "./stores/worktreeManager";
import { errorLogStore } from "./stores/errorLog";
import { appLogger } from "./stores/appLogger";
import { getActionEntries } from "./actions/actionRegistry";
import { promptLibraryStore } from "./stores/promptLibrary";
import { terminalsStore } from "./stores/terminals";
import { paneLayoutStore } from "./stores/paneLayout";
import { initPaneTabAssignment } from "./utils/paneTabAssign";
import { repositoriesStore } from "./stores/repositories";
import { pluginStore } from "./stores/pluginStore";
import { mdTabsStore } from "./stores/mdTabs";
import { diffTabsStore } from "./stores/diffTabs";
import { uiStore } from "./stores/ui";
import { settingsStore } from "./stores/settings";
import { githubStore } from "./stores/github";
import { dictationStore } from "./stores/dictation";
import { notificationsStore } from "./stores/notifications";
import { repoSettingsStore } from "./stores/repoSettings";
import { repoDefaultsStore } from "./stores/repoDefaults";
import { notesStore } from "./stores/notes";
import { activityStore } from "./stores/activityStore";
import { keybindingsStore } from "./stores/keybindings";
import { prNotificationsStore } from "./stores/prNotifications";
import { updaterStore } from "./stores/updater";
import { tasksStore } from "./stores/tasks";
import { userActivityStore } from "./stores/userActivity";
import { contextMenuActionsStore } from "./stores/contextMenuActionsStore";
import { globalWorkspaceStore } from "./stores/globalWorkspace";
import { paneLayoutKey } from "./stores/savedPaneLayouts";
import { initPlugins } from "./plugins";
import { usePty } from "./hooks/usePty";
import { useRepository, tccDeniedPaths, markTccAlertShown } from "./hooks/useRepository";
import { useKeyboardRedirect } from "./hooks/useKeyboardRedirect";
import { useConfirmDialog } from "./hooks/useConfirmDialog";
import { useTerminalLifecycle } from "./hooks/useTerminalLifecycle";
import { useGitOperations } from "./hooks/useGitOperations";
import { useDictation } from "./hooks/useDictation";
import { useQuickSwitcher } from "./hooks/useQuickSwitcher";
import { useSplitPanes } from "./hooks/useSplitPanes";
import { useAgentPolling } from "./hooks/useAgentPolling";
import { useAgentDetection } from "./hooks/useAgentDetection";
import { agentConfigsStore, llmApiStore } from "./stores/agentConfigs";
import { AGENTS, type AgentType } from "./agents";
import { buildAgentLaunchCommand } from "./utils/agentSession";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { initApp } from "./hooks/useAppInit";
import { useFocusRestore } from "./hooks/useFocusRestore";
import { startAutoFetch } from "./hooks/useAutoFetch";
import { useAutoDeleteBranch } from "./hooks/useAutoDeleteBranch";
import { useWorktreeSwitchPrompt } from "./hooks/useWorktreeSwitchPrompt";
import { useCiHeal } from "./hooks/useCiHeal";
import { useSmartPrompts } from "./hooks/useSmartPrompts";
import { applyAppTheme, applyFontFamily } from "./themes";
import { createLongPressHandlerFromHotkey } from "./hooks/useLongPressHotkey";
import { sendCommand } from "./utils/sendCommand";
import { applyPlatformClass, getModifierSymbol, isQuickSwitcherActive, isQuickSwitcherRelease } from "./platform";

import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, listen } from "./invoke";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { isTauri } from "./transport";
import { setLastMenuActionTime } from "./menuDedup";
import { initDeepLinkHandler } from "./deep-link-handler";
import { MobileViewBanner } from "./components/MobileViewBanner";

const getDefaultFontSize = () => settingsStore.state.defaultFontSize;
const getMaxTabNameLength = () => settingsStore.state.maxTabNameLength;

/** Detect secondary window mode via URL query param */
const isSecondaryWindow = () => new URLSearchParams(window.location.search).get("mode") === "secondary";

const App: Component = () => {
  const [statusInfo, _setStatusInfoRaw] = createSignal("Ready");
  let statusInfoTimer: ReturnType<typeof setTimeout> | null = null;
  const setStatusInfo = (text: string) => {
    if (statusInfoTimer) clearTimeout(statusInfoTimer);
    _setStatusInfoRaw(text);
    if (text !== "Ready" && text !== "") {
      statusInfoTimer = setTimeout(() => _setStatusInfoRaw("Ready"), 30_000);
    }
  };
  if (import.meta.env.DEV) {
    (window as any).__tuic_setStatusInfo = setStatusInfo;
    (window as any).__debug = {
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
      notes: notesStore,
      keybindings: keybindingsStore,
      prNotifications: prNotificationsStore,
      updater: updaterStore,
      tasks: tasksStore,
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
  const [settingsContext, setSettingsContext] = createSignal<SettingsContext>({ kind: "global" });

  const openSettings = (tab?: string) => {
    setSettingsContext({ kind: "global" });
    setSettingsInitialTab(tab);
    setSettingsPanelVisible(true);
  };
  const [taskQueueVisible, setTaskQueueVisible] = createSignal(false);

  // Help panel state (Story 053)
  const [helpPanelVisible, setHelpPanelVisible] = createSignal(false);

  // Quit confirmation state (Story 057)
  const [quitDialogVisible, setQuitDialogVisible] = createSignal(false);

  // Quick switcher state
  const [quickSwitcherVisible, setQuickSwitcherVisible] = createSignal(false);

  // Rename branch dialog state
  const [renameBranchDialogVisible, setRenameBranchDialogVisible] = createSignal(false);

  // Run command dialog state
  const [runCommandDialogVisible, setRunCommandDialogVisible] = createSignal(false);

  // Background git operations tracking (for button loading states)
  const [runningGitOps, setRunningGitOps] = createSignal<Set<string>>(new Set());

  // Terminal rename prompt state
  const [termRenamePromptVisible, setTermRenamePromptVisible] = createSignal(false);
  const [termRenameDefault, setTermRenameDefault] = createSignal("");
  const [repoPathPromptVisible, setRepoPathPromptVisible] = createSignal(false);
  let repoPathPromptResolve: ((value: string | null) => void) | null = null;

  /** Show an in-app text-input dialog for repo path (browser mode only) */
  const promptRepoPath = (): Promise<string | null> =>
    new Promise((resolve) => {
      repoPathPromptResolve = resolve;
      setRepoPathPromptVisible(true);
    });

  // Context menu state
  const contextMenu = createContextMenu();

  const pty = usePty();
  const repo = useRepository();
  const dialogs = useConfirmDialog();

  // Redirect keyboard input from sidebar to terminal
  useKeyboardRedirect(true);

  // Centralized focus restore: when any dialog/overlay closes (DOM removed),
  // focus returns to the active terminal automatically.
  useFocusRestore();

  const terminalLifecycle = useTerminalLifecycle({
    pty,
    dialogs,
    setStatusInfo,
    getDefaultFontSize,
  });

  const gitOps = useGitOperations({
    repo,
    pty,
    dialogs: { ...dialogs, promptRepoPath, confirmOrphanCleanup: dialogs.confirmOrphanCleanup },
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
  const [worktreeCleanupStepStatuses, setWorktreeCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [worktreeCleanupStepErrors, setWorktreeCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});
  const [worktreeCleanupAction, setWorktreeCleanupAction] = createSignal<"archive" | "delete">("archive");

  const handleWorktreeCleanupExecute = async (steps: CleanupStep[], options?: { unstash?: boolean }) => {
    const ctx = gitOps.mergePendingCtx();
    if (!ctx) return;
    setWorktreeCleanupExecuting(true);
    setWorktreeCleanupStepStatuses({});
    setWorktreeCleanupStepErrors({});
    await executeCleanup({
      repoPath: ctx.repoPath,
      branchName: ctx.branchName,
      baseBranch: ctx.baseBranch,
      steps: steps.map((s) => ({ id: s.id, checked: s.checked })),
      worktreeAction: worktreeCleanupAction(),
      unstash: options?.unstash,
      onStepStart: (id) => setWorktreeCleanupStepStatuses((prev) => ({ ...prev, [id]: "running" as StepStatus })),
      onStepDone: (id, result, error) => {
        setWorktreeCleanupStepStatuses((prev) => ({ ...prev, [id]: result as StepStatus }));
        if (error) setWorktreeCleanupStepErrors((prev) => ({ ...prev, [id]: error }));
      },
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

  // Register pane tab auto-assignment hook (must be after store imports)
  initPaneTabAssignment();

  // Poll active terminal for foreground agent detection
  useAgentPolling();

  // Agent detection for context menu
  const agentDetection = useAgentDetection();
  const smartPrompts = useSmartPrompts();

  // Show a one-time dialog when macOS TCC denies access to repo directories
  createEffect(() => {
    const paths = tccDeniedPaths();
    if (paths.length === 0) return;
    markTccAlertShown();
    const repos = paths.map((p) => p.split("/").pop() ?? p).join(", ");
    void dialogs.confirm({
      title: "Permission denied",
      message: `macOS blocked access to: ${repos}\n\nRepositories inside ~/Documents, ~/Desktop, or ~/Downloads require Full Disk Access.\n\nTo fix: System Settings → Privacy & Security → Full Disk Access → add TUICommander.\n\nAlternatively, move your repositories to a non-protected folder (e.g. ~/Repositories).`,
      okLabel: "Got it",
      cancelLabel: "Dismiss",
      kind: "error",
    });
  });

  // Auto-delete local branches when their PR is merged/closed
  useAutoDeleteBranch({ confirm: (opts) => dialogs.confirm(opts) });

  // Offer to switch to newly created worktrees (from MCP) + activity notification
  useWorktreeSwitchPrompt({
    confirm: (opts) => dialogs.confirm(opts),
    handleBranchSelect: gitOps.handleBranchSelect,
  });

  // Register built-in activity sections for git and worktree notifications
  activityStore.registerSection({ id: "git-ops", label: "GIT", priority: 30, canDismissAll: true });
  activityStore.registerSection({ id: "worktrees", label: "WORKTREES", priority: 40, canDismissAll: true });

  // Auto-heal CI failures by injecting logs into agent terminals
  useCiHeal();

  // Register git-branches smart prompts as branch context menu actions.
  // Reactive: re-registers when prompts are enabled/disabled.
  createEffect(() => {
    const disposables: Array<{ dispose(): void }> = [];
    for (const prompt of promptLibraryStore.getSmartByPlacement("git-branches")) {
      const p = prompt;
      disposables.push(
        contextMenuActionsStore.registerContextAction("smart-prompts", {
          id: `smart:${p.id}`,
          label: p.name,
          target: "branch",
          action: (ctx) => {
            smartPrompts.executeSmartPrompt(p, ctx.branchName ? { branch_name: ctx.branchName } : undefined).catch((err) =>
              appLogger.error("prompts", "Smart prompt execution failed", err)
            );
          },
        }),
      );
    }
    onCleanup(() => disposables.forEach((d) => d.dispose()));
  });

  // Stop GitHub polling on component teardown — registered at body level so
  // SolidJS can track it synchronously (onCleanup inside async onMount is unreliable).
  onCleanup(() => githubStore.stopPolling());

  // Notification sounds are now played natively via Rust (rodio) —
  // no Web Audio warmup needed.

  onMount(async () => {
    await initApp({
      pty,
      setQuitDialogVisible,
      setStatusInfo,
      setCurrentRepoPath: gitOps.setCurrentRepoPath,
      setCurrentBranch: gitOps.setCurrentBranch,
      handleBranchSelect: gitOps.handleBranchSelect,
      refreshAllBranchStats: gitOps.refreshAllBranchStats,
      getDefaultFontSize,
      stores: {
        hydrate: async () => {
          const results = await Promise.allSettled([
            repositoriesStore.hydrate(),
            uiStore.hydrate(),
            settingsStore.hydrate(),
            notificationsStore.hydrate(),
            repoSettingsStore.hydrate(),
            repoDefaultsStore.hydrate(),
            promptLibraryStore.hydrate(),
            notesStore.hydrate(),
            activityStore.hydrate(),
            keybindingsStore.hydrate(),
            agentConfigsStore.hydrate(),
            llmApiStore.hydrate(),
            agentDetection.detectAll(),
          ]);
          const failures = results.filter((r) => r.status === "rejected");
          if (failures.length > 0) {
            appLogger.error("store", `${failures.length} store(s) failed to hydrate`, failures);
            throw new Error(`${failures.length} store(s) failed`);
          }
        },
        startPolling: githubStore.startPolling,
        stopPolling: githubStore.stopPolling,
        startAutoFetch,
        startPrNotificationTimer: prNotificationsStore.startFocusTimer,
        loadFontFromConfig: settingsStore.loadFontFromConfig,
        refreshDictationConfig: () => dictationStore.refreshConfig().then(() => {
          if (dictationStore.state.enabled) {
            dictationStore.refreshStatus();
          }
        }),
        startUserActivityListening: userActivityStore.startListening,
      },
      applyPlatformClass,
      onCloseRequested: (handler) => {
        if (!isTauri()) return;
        getCurrentWindow().onCloseRequested(async (event) => handler(event));
      },
    }).catch((err) => {
      appLogger.error("app", "Fatal initialization error", err);
      setStatusInfo("Error: App failed to initialize — check error log");
      document.getElementById("splash")?.remove();
    });

    // Check for updates after hydration (non-blocking)
    if (settingsStore.state.autoUpdateEnabled) {
      updaterStore.checkForUpdate().catch((err) => appLogger.debug("app", "Updater auto-check failed", err));
    }

    // Register tuic:// deep link handler
    initDeepLinkHandler({
      openSettings,
      confirm: (title, message) => dialogs.confirm({ title, message, kind: "warning" }),
      onInstallError: (msg) => appLogger.error("plugin", msg),
    });
  });


  // Apply the active theme to the entire app chrome (sidebar, tabs, toolbar, etc.)
  createEffect(() => applyAppTheme(settingsStore.state.theme));

  // Sync --font-mono CSS variable when font selection changes
  createEffect(() => applyFontFamily(settingsStore.state.font));

  // Enforce mutual exclusivity between tab stores.
  // When a non-terminal tab becomes active (e.g. from mdTabsStore.add()),
  // deactivate the terminal so its pane hides and xterm releases focus.
  // Using `on()` with `defer: true` so each effect only fires on its own store's change,
  // preventing the setActive(null) calls from triggering cascading re-runs.
  createEffect(on(() => mdTabsStore.state.activeId, (id) => {
    if (id) { terminalsStore.setActive(null); diffTabsStore.setActive(null); editorTabsStore.setActive(null); }
  }, { defer: true }));
  createEffect(on(() => diffTabsStore.state.activeId, (id) => {
    if (id) { terminalsStore.setActive(null); mdTabsStore.setActive(null); editorTabsStore.setActive(null); }
  }, { defer: true }));
  createEffect(on(() => editorTabsStore.state.activeId, (id) => {
    if (id) { terminalsStore.setActive(null); diffTabsStore.setActive(null); mdTabsStore.setActive(null); }
  }, { defer: true }));
  createEffect(on(() => terminalsStore.state.activeId, (id) => {
    if (id) { diffTabsStore.setActive(null); mdTabsStore.setActive(null); editorTabsStore.setActive(null); }
  }, { defer: true }));

  // Persist the currently-active terminal to its owning branch on every change,
  // so returning to a branch (via sidebar click or repo switch) restores the
  // tab the user last focused — not the first terminal in branch.terminals.
  createEffect(on(() => terminalsStore.state.activeId, (id) => {
    if (!id) return;
    const repoPath = repositoriesStore.getRepoPathForTerminal(id);
    if (!repoPath) return;
    const repo = repositoriesStore.state.repositories[repoPath];
    if (!repo) return;
    for (const [branchName, branch] of Object.entries(repo.branches)) {
      if (branch.terminals.includes(id)) {
        if (branch.lastActiveTerminal !== id) {
          repositoriesStore.setBranch(repoPath, branchName, { lastActiveTerminal: id });
        }
        break;
      }
    }
  }, { defer: true }));

  // Prevent system sleep while any terminal is busy (debounced — Story 258/405)
  let lastSleepBlocked: boolean | null = null;
  createEffect(() => {
    if (!isTauri()) return;
    const enabled = settingsStore.state.preventSleepWhenBusy;
    const anyBusy = terminalsStore.isAnyBusy();
    const shouldBlock = enabled && anyBusy;

    if (shouldBlock === lastSleepBlocked) return;
    lastSleepBlocked = shouldBlock;

    if (shouldBlock) {
      invoke("block_sleep").catch((err) =>
        appLogger.warn("app", "Failed to block sleep", err),
      );
    } else {
      invoke("unblock_sleep").catch((err) =>
        appLogger.warn("app", "Failed to unblock sleep", err),
      );
    }
  });

  // Completion notification: play sound when a terminal was busy for >=5s then goes idle.
  // Deferred when the agent has active sub-tasks or is an agent process (sub-agents may still be running).
  const BUSY_COMPLETION_THRESHOLD_MS = 5000;
  const DEFERRED_COMPLETION_MS = 10_000;
  const deferredCompletionTimers = new Map<string, ReturnType<typeof setTimeout>>();

  const unsubBusyToIdle = terminalsStore.onBusyToIdle((id, durationMs) => {
    if (durationMs < BUSY_COMPLETION_THRESHOLD_MS) return;
    if (terminalsStore.state.activeId === id) return;

    const fireCompletion = () => {
      deferredCompletionTimers.delete(id);
      // Re-check: terminal may have been removed during deferral.
      const terminal = terminalsStore.get(id);
      if (!terminal) return;

      const reason = getCompletionSuppression({
        isActiveTerminal: terminalsStore.state.activeId === id,
        isDebouncedBusy: !!terminalsStore.state.debouncedBusy[id],
        activeSubTasks: terminal.activeSubTasks,
        awaitingInput: terminal.awaitingInput,
        durationMs,
        thresholdMs: BUSY_COMPLETION_THRESHOLD_MS,
      });
      if (reason) {
        appLogger.debug("terminal", `[Notify] ${id} completion SUPPRESSED — ${reason}`);
        return;
      }
      appLogger.info("terminal", `[Notify] ${id} completion — busy for ${Math.round(durationMs / 1000)}s then idle`);
      terminalsStore.update(id, { activity: true, unseen: true });
      notificationsStore.playCompletion();
    };

    const t = terminalsStore.get(id);
    // Suppress immediately if agent has known active sub-tasks.
    if (t && t.activeSubTasks > 0) {
      appLogger.debug("terminal", `[Notify] ${id} completion SUPPRESSED — ${t.activeSubTasks} active sub-tasks`);
      return;
    }
    if (t?.agentType) {
      // Agent process: defer — if the terminal stays idle for 10s, it's truly done.
      clearTimeout(deferredCompletionTimers.get(id));
      deferredCompletionTimers.set(id, setTimeout(fireCompletion, DEFERRED_COMPLETION_MS));
    } else {
      fireCompletion();
    }
  });
  onCleanup(() => {
    unsubBusyToIdle();
    for (const timer of deferredCompletionTimers.values()) clearTimeout(timer);
    deferredCompletionTimers.clear();
  });

  // Initialize plugin system
  onMount(() => {
    initPlugins().catch((err) =>
      appLogger.error("plugin", "Plugin initialization failed", err instanceof Error ? { stack: err.stack } : err),
    );
  });

  // Clear dock badge when window gains focus
  onMount(() => {
    const handleFocus = () => notificationsStore.clearBadge();
    window.addEventListener("focus", handleFocus);
    onCleanup(() => window.removeEventListener("focus", handleFocus));
  });

  // Force quit - close all sessions and exit (Story 057)
  const forceQuit = async () => {
    setQuitDialogVisible(false);

    // Destroy window after a short timeout regardless of PTY cleanup
    const destroyTimer = isTauri() ? setTimeout(() => getCurrentWindow().destroy(), 500) : null;

    try {
      const sessionIds = terminalsStore.getIds()
        .map((id) => terminalsStore.get(id)?.sessionId)
        .filter((sid): sid is string => sid != null);
      await Promise.all(sessionIds.map((sid) => pty.close(sid).catch((err) => appLogger.warn("app", `Failed to close PTY ${sid} on quit`, err))));
    } catch { /* ignore */ }

    if (destroyTimer !== null) clearTimeout(destroyTimer);
    if (isTauri()) getCurrentWindow().destroy();
  };

  // Build agent submenu items for the context menu
  /** Launch an agent in the active terminal, injecting --session-id when supported */
  const launchAgentInActiveTerminal = async (agentType: AgentType, cmd: string) => {
    const active = terminalsStore.getActive();
    if (!active?.ref || !active.sessionId) return;
    // Use the tab's stable tuicSession UUID as --session-id so resume works via TUIC_SESSION
    const agentSessionId = active.tuicSession ?? (agentType === "claude" ? crypto.randomUUID() : null);
    const finalCmd = buildAgentLaunchCommand(cmd, agentSessionId);
    await sendCommand((data) => invoke("write_pty", { sessionId: active.sessionId, data }), finalCmd);
    terminalsStore.update(active.id, {
      name: AGENTS[agentType].name,
      nameIsCustom: true,
      agentSessionId,
    });
  };

  const buildAgentMenuItems = (): ContextMenuItem[] => {
    const available = agentDetection.getAvailable()
      .filter((a) => a.type !== "git" && a.type !== "api");
    if (available.length === 0) return [];

    return available.map((agent) => {
      const agentConfig = AGENTS[agent.type];
      const runConfigs = agentConfigsStore.getRunConfigs(agent.type);

      // Multiple run configs: submenu with each config
      if (runConfigs.length > 1) {
        return {
          label: agentConfig.name,
          action: () => {},
          children: runConfigs.map((rc) => ({
            label: rc.name + (rc.is_default ? " (Default)" : ""),
            action: () => launchAgentInActiveTerminal(agent.type, [rc.command, ...rc.args].join(" ")),
          })),
        };
      }

      // 0-1 run configs: flat item, click launches directly
      const cmd = runConfigs.length === 1
        ? [runConfigs[0].command, ...runConfigs[0].args].join(" ")
        : agentConfig.binary;
      return {
        label: agentConfig.name,
        action: () => launchAgentInActiveTerminal(agent.type, cmd),
      };
    });
  };

  // Build agent menu items for the sidebar branch context menu.
  // Creates a new terminal on the branch and writes the agent launch command.
  // Returns items ready to splice into the context menu:
  // - 0 enabled agents: []
  // - 1 enabled agent: [{ label: "Add <AgentName>", action }]
  // - N enabled agents: [{ label: "Add Agent", children: [...] }]
  const buildSidebarAgentMenuItems = (repoPath: string, branchName: string): ContextMenuItem[] => {
    const enabled = agentDetection.getAvailable()
      .filter((a) => a.type !== "git" && settingsStore.isAgentEnabled(a.type));
    if (enabled.length === 0) return [];

    const buildAgentEntry = (agent: typeof enabled[0]) => {
      const agentConfig = AGENTS[agent.type];
      const runConfigs = agentConfigsStore.getRunConfigs(agent.type);

      const launchAgent = async (cmd: string) => {
        const termId = await gitOps.handleAddTerminalToBranch(repoPath, branchName);
        if (termId) {
          // Use the new tab's stable tuicSession UUID as --session-id
          const term = terminalsStore.get(termId);
          const agentSessionId = term?.tuicSession ?? (agent.type === "claude" ? crypto.randomUUID() : null);
          const finalCmd = buildAgentLaunchCommand(cmd, agentSessionId);
          terminalsStore.update(termId, {
            name: agentConfig.name,
            nameIsCustom: true,
            pendingInitCommand: finalCmd,
            agentSessionId,
          });
        }
      };

      const children: ContextMenuItem[] = runConfigs.length > 0
        ? runConfigs.map((rc) => ({
            label: rc.name + (rc.is_default ? " (Default)" : ""),
            action: () => launchAgent([rc.command, ...rc.args].join(" ")),
          }))
        : [{
            label: "(Default)",
            action: () => launchAgent(agentConfig.binary),
          }];

      return { agentConfig, children };
    };

    if (enabled.length === 1) {
      const { agentConfig, children } = buildAgentEntry(enabled[0]);
      // Single agent: flat item "Add <Name>" (with sub-configs if multiple)
      if (children.length === 1) {
        return [{ label: `Add ${agentConfig.name}`, action: children[0].action }];
      }
      return [{ label: `Add ${agentConfig.name}`, action: () => {}, children }];
    }

    // Multiple agents: "Add Agent" submenu
    const agentItems = enabled.map((agent) => {
      const { agentConfig, children } = buildAgentEntry(agent);
      if (children.length === 1) {
        return { label: agentConfig.name, action: children[0].action };
      }
      return { label: agentConfig.name, action: () => {}, children };
    });

    return [{ label: "Add Agent", action: () => {}, children: agentItems }];
  };

  // Context menu items
  /** Disable split when no active terminal, or active group is at max tree depth */
  const splitDisabled = () => {
    if (!paneLayoutStore.isSplit()) return !terminalsStore.state.activeId;
    const activeGroupId = paneLayoutStore.state.activeGroupId;
    return !activeGroupId || !paneLayoutStore.canSplit(activeGroupId);
  };

  /** Check if the active terminal has a running agent (disables agent submenu) */
  const activeTerminalBusy = (): boolean => {
    const activeId = terminalsStore.state.activeId;
    if (!activeId) return true;
    const term = terminalsStore.get(activeId);
    return !!term?.agentType;
  };

  const getContextMenuItems = (): ContextMenuItem[] => [
    ...(agentDetection.getAvailable().length > 0 ? [{
      label: "Agents",
      action: () => {},
      disabled: activeTerminalBusy(),
      children: buildAgentMenuItems(),
    }] : []),
    { label: "Copy", shortcut: `${getModifierSymbol()}C`, action: terminalLifecycle.copyFromTerminal, separator: agentDetection.getAvailable().length > 0 },
    { label: "Paste", shortcut: `${getModifierSymbol()}V`, action: terminalLifecycle.pasteToTerminal },
    { label: "Split Right", shortcut: `${getModifierSymbol()}\\`, action: () => splitPanes.handleSplit("vertical"), disabled: splitDisabled() },
    { label: "Split Left", action: () => splitPanes.handleSplit("vertical"), disabled: splitDisabled() },
    { label: "Split Down", shortcut: `${getModifierSymbol()}${"\u2325"}\\`, action: () => splitPanes.handleSplit("horizontal"), disabled: splitDisabled() },
    { label: "Split Up", action: () => splitPanes.handleSplit("horizontal"), disabled: splitDisabled(), separator: true },
    { label: "Clear", shortcut: `${getModifierSymbol()}L`, action: terminalLifecycle.clearTerminal },
    {
      label: "Reset Terminal",
      action: () => {
        const activeId = terminalsStore.state.activeId;
        if (activeId) terminalsStore.get(activeId)?.ref?.write("\x1bc");
      },
    },
    {
      label: "Change Title\u2026",
      action: () => {
        const activeId = terminalsStore.state.activeId;
        if (!activeId) return;
        const current = terminalsStore.get(activeId)?.name || "";
        setTermRenameDefault(current);
        setTermRenamePromptVisible(true);
      },
      separator: true,
    },
    ...(() => {
      const pluginActions = contextMenuActionsStore.getActions();
      if (pluginActions.length === 0) return [];
      const activeId = terminalsStore.state.activeId;
      const sessionId = activeId ? terminalsStore.get(activeId)?.sessionId ?? null : null;
      const repoPath = repositoriesStore.state.activeRepoPath ?? null;
      const ctx = { sessionId, repoPath };
      return [{
        label: "Actions",
        action: () => {},
        children: pluginActions.map((a) => ({
          label: a.label,
          action: () => a.action(ctx),
          disabled: a.disabled?.(ctx) ?? false,
        })),
        separator: true,
      }];
    })(),
    {
      label: "Close Terminal",
      shortcut: `${getModifierSymbol()}W`,
      action: () => {
        const activeId = terminalsStore.state.activeId;
        if (activeId) terminalLifecycle.closeTerminal(activeId);
      },
      separator: true,
    },
  ];

  /** Open a file path from terminal output — .md/.mdx in MD viewer, others in internal editor */
  const handleOpenFilePath = (absolutePath: string, _line?: number, _col?: number) => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath) return;
    const fsRoot = gitOps.activeWorktreePath() || repoPath;

    // Convert to relative path when inside the effective root (worktree or repo), keep absolute otherwise
    const rootPrefix = fsRoot.endsWith("/") ? fsRoot : fsRoot + "/";
    const filePath = absolutePath.startsWith(rootPrefix)
      ? absolutePath.slice(rootPrefix.length)
      : absolutePath;

    if (filePath.endsWith(".md") || filePath.endsWith(".mdx")) {
      mdTabsStore.add(repoPath, filePath, fsRoot);
    } else if (filePath.endsWith(".html") || filePath.endsWith(".htm")) {
      mdTabsStore.addHtmlPreview(repoPath, filePath, fsRoot);
    } else {
      const tabId = editorTabsStore.add(fsRoot, filePath);
      terminalLifecycle.handleTerminalSelect(tabId);
    }
  };

  // Listen for file-open events from macOS file associations
  createEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string[]>("file-open", (event) => {
      for (const absolutePath of event.payload) {
        const repoPath = repositoriesStore.state.activeRepoPath ?? "";
        const fsRoot = gitOps.activeWorktreePath() || repoPath;
        const rootPrefix = fsRoot ? (fsRoot.endsWith("/") ? fsRoot : fsRoot + "/") : "";
        const filePath = rootPrefix && absolutePath.startsWith(rootPrefix)
          ? absolutePath.slice(rootPrefix.length)
          : absolutePath;
        const effectiveRepo = filePath === absolutePath ? "" : repoPath;
        const effectiveRoot = filePath === absolutePath ? "" : fsRoot;

        if (absolutePath.endsWith(".md") || absolutePath.endsWith(".mdx")) {
          mdTabsStore.add(effectiveRepo, filePath, effectiveRoot || undefined);
        } else if (absolutePath.endsWith(".html") || absolutePath.endsWith(".htm")) {
          mdTabsStore.addHtmlPreview(effectiveRepo, filePath, effectiveRoot || undefined);
        } else {
          editorTabsStore.add(effectiveRoot || effectiveRepo, filePath);
        }
      }
    }).then((fn) => { unlisten = fn; }).catch((err) => appLogger.error("app", "Failed to listen for file-open events", err));

    onCleanup(() => unlisten?.());
  });

  /** Patterns in stderr that indicate the command needs interactive terminal input */
  const NEEDS_TERMINAL_PATTERNS = [
    /terminal prompts disabled/i,  // GIT_TERMINAL_PROMPT=0 rejection
    /could not read Username/i,    // HTTP credential prompt blocked
    /could not read Password/i,
    /Permission denied.*publickey/i, // SSH auth failed (askpass cancelled/missing)
    /Host key verification failed/i,
    /Please make sure you have the correct access rights/i,
  ];

  /** Fall back to running a git command in the active terminal */
  const fallbackToTerminal = (repoPath: string, args: string[]) => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      const cmd = `cd ${JSON.stringify(repoPath)} && git ${args.join(" ")}`;
      active.ref.write(`${cmd}\r`);
      setStatusInfo(`git ${args[0]} requires auth — running in terminal`);
    }
  };

  /** Run a git command in the background via Rust, with task tracking and notifications.
   *  Falls back to terminal if the command needs interactive authentication. */
  const handleBackgroundGit = async (repoPath: string, op: string, args: string[]) => {
    // Prevent duplicate runs of the same op
    if (runningGitOps().has(op)) return;
    setRunningGitOps((prev) => new Set([...prev, op]));

    const taskId = tasksStore.create({
      name: `git ${op}`,
      description: `Running git ${args.join(" ")} in ${repoPath}`,
      agentType: "git",
    });
    tasksStore.start(taskId, `git-${op}-${Date.now()}`);
    setStatusInfo(`Running git ${op}...`);

    try {
      const result = await invoke("run_git_command", { path: repoPath, args });
      const { success, stdout, stderr } = result as { success: boolean; stdout: string; stderr: string; exit_code: number };

      if (success) {
        tasksStore.complete(taskId, 0);
        repositoriesStore.bumpRevision(repoPath);
        // Show meaningful output: prefer stderr (git progress goes there), fall back to stdout
        const output = (stderr.trim() || stdout.trim()).split("\n").pop()?.trim();
        const summary = output ? `git ${op}: ${output}` : `git ${op} completed`;
        setStatusInfo(summary);
        appLogger.info("app", `[Notify] completion — git ${op} succeeded`);
        notificationsStore.playCompletion();
        activityStore.addItem({
          id: `git-${op}-${Date.now()}`,
          pluginId: "core",
          sectionId: "git-ops",
          title: `git ${op}`,
          subtitle: output || "completed",
          icon: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.78 5.97l-4.5 4.5a.75.75 0 0 1-1.06 0l-2-2a.75.75 0 1 1 1.06-1.06L6.75 8.88l3.97-3.97a.75.75 0 1 1 1.06 1.06z"/></svg>',
          repoPath,
          dismissible: true,
        });
      } else if (NEEDS_TERMINAL_PATTERNS.some((p) => p.test(stderr))) {
        // Auth or interactive prompt needed — cancel background task, run in terminal
        tasksStore.cancel(taskId);
        fallbackToTerminal(repoPath, args);
      } else {
        const errMsg = stderr.trim() || `git ${op} failed`;
        tasksStore.fail(taskId, errMsg);
        setStatusInfo(`git ${op} failed: ${errMsg}`);
        appLogger.info("app", `[Notify] error — git ${op} failed: ${errMsg}`);
        notificationsStore.playError();
        activityStore.addItem({
          id: `git-${op}-${Date.now()}`,
          pluginId: "core",
          sectionId: "git-ops",
          title: `git ${op} failed`,
          subtitle: errMsg,
          icon: '<svg viewBox="0 0 16 16" width="14" height="14" fill="#f85149"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.36 10.3a.75.75 0 0 1-1.06 1.06L8 9.06l-2.3 2.3a.75.75 0 0 1-1.06-1.06L6.94 8 4.64 5.7a.75.75 0 0 1 1.06-1.06L8 6.94l2.3-2.3a.75.75 0 0 1 1.06 1.06L9.06 8l2.3 2.3z"/></svg>',
          repoPath,
          dismissible: true,
        });
      }
    } catch (err) {
      tasksStore.fail(taskId, String(err));
      setStatusInfo(`git ${op} error: ${err}`);
      appLogger.info("app", `[Notify] error — git ${op} exception: ${err}`);
      notificationsStore.playError();
    } finally {
      setRunningGitOps((prev) => {
        const next = new Set(prev);
        next.delete(op);
        return next;
      });
    }
  };

  /** Detach a terminal tab to a floating OS window */
  const handleDetachTab = async (tabId: string) => {
    if (!isTauri()) return;
    const term = terminalsStore.get(tabId);
    if (!term?.sessionId) return;

    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const windowLabel = `floating-${tabId}`;
    const url = `index.html#/floating?sessionId=${encodeURIComponent(term.sessionId)}&tabId=${encodeURIComponent(tabId)}&name=${encodeURIComponent(term.name)}`;

    new WebviewWindow(windowLabel, {
      url,
      title: term.name || "Terminal",
      width: 800,
      height: 600,
      center: true,
      decorations: true,
    });

    terminalsStore.detach(tabId, windowLabel);

    // Switch to next available tab
    const ids = terminalLifecycle.terminalIds().filter((id) => !terminalsStore.isDetached(id));
    if (ids.length > 0) {
      terminalLifecycle.handleTerminalSelect(ids[0]);
    }
  };

  // Listen for reattach events from floating windows
  createEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;

    listen<{ tabId: string; sessionId: string }>("reattach-terminal", (event) => {
      const { tabId } = event.payload;
      terminalsStore.reattach(tabId);
      terminalLifecycle.handleTerminalSelect(tabId);
      setStatusInfo("Tab reattached");
      // Force fit after the pane becomes visible again — the xterm canvas
      // may have lost its WebGL context while hidden with display:none.
      setTimeout(() => {
        terminalsStore.get(tabId)?.ref?.fit();
      }, 150);
    }).then((fn) => { unlisten = fn; }).catch((err) => appLogger.error("terminal", "Failed to listen for reattach events", err));

    onCleanup(() => unlisten?.());
  });

  // Quick Switcher: visible while modifier combo is held, hides on release
  // macOS: Cmd+Ctrl, Windows/Linux: Ctrl+Alt
  // Also dismiss on window blur/visibility change — keyup events are missed
  // when the window loses focus while modifier keys are held.
  createEffect(() => {
    const trackKeydown = (e: KeyboardEvent) => {
      if (isQuickSwitcherActive(e)) {
        setQuickSwitcherVisible(true);
      }
    };

    const trackKeyup = (e: KeyboardEvent) => {
      if (isQuickSwitcherRelease(e)) {
        setQuickSwitcherVisible(false);
      }
    };

    const dismiss = () => setQuickSwitcherVisible(false);

    document.addEventListener("keydown", trackKeydown);
    document.addEventListener("keyup", trackKeyup);
    window.addEventListener("blur", dismiss);
    document.addEventListener("visibilitychange", dismiss);
    onCleanup(() => {
      document.removeEventListener("keydown", trackKeydown);
      document.removeEventListener("keyup", trackKeyup);
      window.removeEventListener("blur", dismiss);
      document.removeEventListener("visibilitychange", dismiss);
    });
  });

  // Shared shortcut handlers used by both keyboard shortcuts and command palette.
  // Zoom is context-aware: when a markdown tab is active, zoom scales its content;
  // otherwise it scales the active terminal (same dispatch pattern as `findInTerminal`).
  const shortcutHandlers = {
    zoomIn: () => {
      if (mdTabsStore.state.activeId) mdTabsStore.zoomIn();
      else terminalLifecycle.zoomIn();
    },
    zoomOut: () => {
      if (mdTabsStore.state.activeId) mdTabsStore.zoomOut();
      else terminalLifecycle.zoomOut();
    },
    zoomReset: () => {
      if (mdTabsStore.state.activeId) mdTabsStore.zoomReset();
      else terminalLifecycle.zoomReset();
    },
    zoomInAll: terminalLifecycle.zoomInAll,
    zoomOutAll: terminalLifecycle.zoomOutAll,
    zoomResetAll: terminalLifecycle.zoomResetAll,
    createNewTerminal: terminalLifecycle.createNewTerminal,
    closeTerminal: terminalLifecycle.closeTerminal,
    reopenClosedTab: terminalLifecycle.reopenClosedTab,
    navigateTab: terminalLifecycle.navigateTab,
    clearTerminal: terminalLifecycle.clearTerminal,
    clearScrollback: terminalLifecycle.clearScrollback,
    scrollToTop: terminalLifecycle.scrollToTop,
    scrollToBottom: terminalLifecycle.scrollToBottom,
    scrollPageUp: terminalLifecycle.scrollPageUp,
    scrollPageDown: terminalLifecycle.scrollPageDown,
    toggleZoomPane: splitPanes.toggleZoomPane,
    closeActivePane: splitPanes.closeActivePane,
    terminalIds: terminalLifecycle.terminalIds,
    handleTerminalSelect: terminalLifecycle.handleTerminalSelect,
    handleSplit: splitPanes.handleSplit,
    handleRunCommand: (forceDialog: boolean) => gitOps.handleRunCommand(forceDialog, () => setRunCommandDialogVisible(true)),
    switchToBranchByIndex: quickSwitcher.switchToBranchByIndex,
    isQuickSwitcherOpen: quickSwitcherVisible,
    toggleMarkdownPanel: uiStore.toggleMarkdownPanel,
    toggleSidebar: uiStore.toggleSidebar,
    togglePromptLibrary: promptLibraryStore.toggleDrawer,
    toggleSettings: () => setSettingsPanelVisible((v) => !v),
    toggleTaskQueue: () => setTaskQueueVisible((v) => !v),
    toggleGitOpsPanel: uiStore.toggleGitPanel,
    toggleHelpPanel: () => setHelpPanelVisible((v) => !v),
    toggleNotesPanel: uiStore.toggleNotesPanel,
    toggleFileBrowserPanel: uiStore.toggleFileBrowserPanel,
    findInTerminal: () => {
      // Context-aware: open search in whichever tab type is active
      const diffActiveId = diffTabsStore.state.activeId;
      if (diffActiveId) {
        const handle = diffTabsStore.getHandle<{ openSearch: () => void }>(diffActiveId);
        handle?.openSearch();
        return;
      }
      const mdActiveId = mdTabsStore.state.activeId;
      if (mdActiveId) {
        const handle = mdTabsStore.getHandle<{ openSearch: () => void }>(mdActiveId);
        handle?.openSearch();
        return;
      }
      const active = terminalsStore.getActive();
      active?.ref?.openSearch();
    },
    toggleCommandPalette: () => commandPaletteStore.toggle(),
    toggleActivityDashboard: () => activityDashboardStore.toggle(),
    toggleWorktreeManager: () => worktreeManagerStore.toggle(),
    toggleBranchSwitcher: () => branchSwitcherStore.toggle(),
    toggleErrorLog: () => errorLogStore.toggle(),
    toggleBranchesTab: () => uiStore.toggleGitPanelOnTab("branches"),
    toggleMcpPopup: () => mcpPopupStore.toggle(),
    toggleGlobalWorkspace: () => {
      if (!globalWorkspaceStore.hasPromoted()) return;
      const repoPath = repositoriesStore.state.activeRepoPath;
      const repo = repoPath ? repositoriesStore.state.repositories[repoPath] : null;
      const key = repoPath && repo?.activeBranch ? paneLayoutKey(repoPath, repo.activeBranch) : undefined;
      if (globalWorkspaceStore.isActive()) {
        globalWorkspaceStore.deactivate(key);
      } else {
        globalWorkspaceStore.activate(key);
      }
    },
    toggleDiffScroll: () => {
      // Open a diff tab in scroll mode for the active repo
      const repoPath = repositoriesStore.state.activeRepoPath;
      if (!repoPath) return;
      uiStore.setDiffViewMode("scroll");
      diffTabsStore.add(repoPath, "", "M");
    },
    openFile: () => {
      const defaultPath = gitOps.activeWorktreePath() || repositoriesStore.state.activeRepoPath || undefined;
      (async () => {
        try {
          const selected = await openDialog({ multiple: false, directory: false, defaultPath });
          if (typeof selected === "string") handleOpenFilePath(selected);
        } catch (err) {
          appLogger.error("app", "Open file dialog failed", err);
        }
      })();
    },
    openSecondaryWindow: () => {
      invoke("open_secondary_window", {}).catch((err) =>
        appLogger.error("app", "Failed to open secondary window", err),
      );
    },
    newFile: () => {
      const defaultPath = gitOps.activeWorktreePath() || repositoriesStore.state.activeRepoPath || undefined;
      (async () => {
        try {
          // Prompt for name+location upfront. The file is created empty and routed
          // through handleOpenFilePath so existing extension-based dispatch applies.
          const target = await saveDialog({ title: "New File", defaultPath });
          if (typeof target !== "string") return;
          await invoke("write_external_file", { path: target, content: "" });
          handleOpenFilePath(target);
        } catch (err) {
          appLogger.error("app", "New file creation failed", err);
        }
      })();
    },
  };

  // Worktree manager action callbacks
  const worktreeActions: WorktreeActions = {
    onOpenTerminal: (repoPath, branchName) => {
      void gitOps.handleAddTerminalToBranch(repoPath, branchName);
    },
    onDelete: (repoPath, branchName) => {
      void gitOps.handleRemoveBranch(repoPath, branchName);
    },
    onMergeAndArchive: (repoPath, branchName) => {
      const repoState = repositoriesStore.get(repoPath);
      const mainBranch = repoState ? Object.values(repoState.branches).find(b => b.isMain)?.name : undefined;
      if (!mainBranch) {
        setStatusInfo("Cannot merge: no main branch found");
        return;
      }
      const effective = repoSettingsStore.getEffective(repoPath);
      const afterMerge = effective?.afterMerge ?? "archive";
      void gitOps.handleMergeAndArchive(repoPath, branchName, mainBranch, afterMerge);
    },
  };

  // Action entries for the command palette: static registry + dynamic entries
  const actionEntries = createMemo(() => {
    const entries = getActionEntries(shortcutHandlers);

    // Dynamic: one entry per active (non-parked) repo for quick switching
    const repos = Object.values(repositoriesStore.state.repositories);
    for (const repo of repos) {
      if (repo.parked) continue;
      entries.push({
        id: `switch-repo:${repo.path}`,
        label: repo.displayName,
        category: "Repository",
        keybinding: "",
        execute: () => {
          repositoriesStore.setActive(repo.path);
          const branch = repo.activeBranch || Object.keys(repo.branches)[0];
          if (branch) gitOps.handleBranchSelect(repo.path, branch);
        },
      });
    }

    // Dynamic: one entry per parked repo for unparking
    for (const repo of repos) {
      if (!repo.parked) continue;
      entries.push({
        id: `unpark-repo:${repo.path}`,
        label: `Unpark: ${repo.displayName}`,
        category: "Repository",
        keybinding: "",
        execute: () => {
          repositoriesStore.setPark(repo.path, false);
          repositoriesStore.setActive(repo.path);
          const branch = repo.activeBranch || Object.keys(repo.branches)[0];
          if (branch) gitOps.handleBranchSelect(repo.path, branch);
        },
      });
    }

    // Static extra actions (no keybinding)
    entries.push({
      id: "add-repository",
      label: "Add Repository",
      category: "Repository",
      keybinding: "",
      execute: () => gitOps.handleAddRepo(),
    });
    entries.push({
      id: "check-for-updates",
      label: "Check for Updates",
      category: "Application",
      keybinding: "",
      execute: () => updaterStore.checkForUpdate().catch((err) => appLogger.warn("app", "Updater check failed", err)),
    });
    entries.push({
      id: "reset-panel-sizes",
      label: "Reset Panel Sizes",
      category: "Application",
      keybinding: "",
      execute: () => splitPanes.resetLayout(),
    });

    // Search mode shortcuts — open palette with pre-filled prefix
    entries.push({
      id: "search-terminals",
      label: "Search Terminals",
      category: "Search",
      keybinding: "",
      execute: () => commandPaletteStore.openWithQuery("~ "),
    });
    entries.push({
      id: "search-files",
      label: "Search Files",
      category: "Search",
      keybinding: "",
      execute: () => commandPaletteStore.openWithQuery("! "),
    });
    entries.push({
      id: "search-file-contents",
      label: "Search in File Contents",
      category: "Search",
      keybinding: "",
      execute: () => commandPaletteStore.openWithQuery("? "),
    });

    // Dynamic: one entry per non-built-in plugin for enable/disable toggle
    for (const plugin of pluginStore.state.plugins) {
      if (plugin.builtIn) continue;
      const name = plugin.manifest?.name ?? plugin.id;
      entries.push({
        id: `toggle-plugin:${plugin.id}`,
        label: `${plugin.enabled ? "Disable" : "Enable"} plugin: ${name}`,
        category: "Plugins",
        keybinding: "",
        execute: () => {
          const fresh = pluginStore.getPlugin(plugin.id);
          if (!fresh) return;
          pluginStore.setEnabled(plugin.id, !fresh.enabled)
            .catch((err) => appLogger.error("plugin", `Failed to toggle plugin ${name}`, err));
        },
      });
    }

    // Dynamic: worktree move targets for the active terminal
    const activeTermId = terminalsStore.state.activeId;
    if (activeTermId) {
      for (const wt of gitOps.getWorktreeTargets(activeTermId)) {
        entries.push({
          id: `move-to-worktree:${wt.path}`,
          label: `Move to worktree: ${wt.branchName}`,
          category: "Terminal",
          keybinding: "",
          execute: () => gitOps.moveTerminalToWorktree(activeTermId, wt.path),
        });
      }
    }

    // Dynamic: smart prompts with command-palette placement
    for (const prompt of promptLibraryStore.getSmartByPlacement("command-palette")) {
      const p = prompt; // capture for closure
      entries.push({
        id: `smart:${p.id}`,
        label: `Smart: ${p.name}`,
        category: "Smart Prompts",
        keybinding: p.shortcut ?? "",
        execute: () => { smartPrompts.executeSmartPrompt(p).catch((err) => appLogger.error("prompts", "Smart prompt execution failed", err)); },
      });
    }

    // Dynamic: plugin-registered terminal actions (context menu + multi-target)
    for (const action of contextMenuActionsStore.getActions()) {
      entries.push({
        id: `plugin-action:${action.id}`,
        label: action.label,
        category: "Plugins",
        keybinding: "",
        execute: () => {
          const activeId = terminalsStore.state.activeId;
          const terminal = activeId ? terminalsStore.get(activeId) : null;
          action.action({ sessionId: terminal?.sessionId ?? null, repoPath: null });
        },
      });
    }

    return entries;
  });

  // Keyboard shortcuts
  createEffect(() => {
    const cleanup = useKeyboardShortcuts(shortcutHandlers);
    onCleanup(cleanup);
  });

  // Native menu bar events — dispatches to the same handlers as keyboard shortcuts
  createEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<string>("menu-action", (event) => {
      setLastMenuActionTime(Date.now());
      const action = event.payload;

      switch (action) {
        // File
        case "new-tab": terminalLifecycle.createNewTerminal(); break;
        case "close-tab": {
          if (paneLayoutStore.isSplit()) {
            splitPanes.closeActivePane();
          } else {
            const activeId = terminalsStore.state.activeId;
            if (activeId) terminalLifecycle.closeTerminal(activeId);
          }
          break;
        }
        case "reopen-closed-tab": terminalLifecycle.reopenClosedTab(); break;
        case "settings": setSettingsPanelVisible((v) => !v); break;
        case "quit-app": {
          if (settingsStore.state.confirmBeforeQuit) {
            const activeTerminals = terminalsStore.getIds().filter(
              (id) => terminalsStore.get(id)?.sessionId,
            );
            if (activeTerminals.length > 0) {
              setQuitDialogVisible(true);
              break;
            }
          }
          void forceQuit();
          break;
        }

        // Edit
        case "clear-terminal": terminalLifecycle.clearTerminal(); break;

        // View
        case "toggle-sidebar": uiStore.toggleSidebar(); break;
        case "split-right": splitPanes.handleSplit("vertical"); break;
        case "split-down": splitPanes.handleSplit("horizontal"); break;
        case "zoom-in": shortcutHandlers.zoomIn(); break;
        case "zoom-out": shortcutHandlers.zoomOut(); break;
        case "zoom-reset": shortcutHandlers.zoomReset(); break;
        case "zoom-in-all": terminalLifecycle.zoomInAll(); break;
        case "zoom-out-all": terminalLifecycle.zoomOutAll(); break;
        case "zoom-reset-all": terminalLifecycle.zoomResetAll(); break;
        case "diff-panel": uiStore.toggleGitPanel(); break;
        case "markdown-panel": uiStore.toggleMarkdownPanel(); break;
        case "notes-panel": uiStore.toggleNotesPanel(); break;
        case "file-browser": uiStore.toggleFileBrowserPanel(); break;

        // Go
        case "next-tab": terminalLifecycle.navigateTab("next"); break;
        case "prev-tab": terminalLifecycle.navigateTab("prev"); break;

        // Tools
        case "prompt-library": promptLibraryStore.toggleDrawer(); break;
        case "run-command": gitOps.handleRunCommand(false, () => setRunCommandDialogVisible(true)); break;
        case "edit-run-command": gitOps.handleRunCommand(true, () => setRunCommandDialogVisible(true)); break;
        case "git-operations": uiStore.toggleGitPanel(); break;
        case "diff-scroll": {
          const repoPath = repositoriesStore.state.activeRepoPath;
          if (repoPath) {
            uiStore.setDiffViewMode("scroll");
            diffTabsStore.add(repoPath, "", "M");
          }
          break;
        }
        case "branches": uiStore.toggleGitPanelOnTab("branches"); break;
        case "task-queue": setTaskQueueVisible((v) => !v); break;

        // Help
        case "help-panel": setHelpPanelVisible((v) => !v); break;
        case "command-palette": commandPaletteStore.toggle(); break;
        case "activity-dashboard": activityDashboardStore.toggle(); break;
        case "error-log": errorLogStore.toggle(); break;
        case "mcp-popup": mcpPopupStore.toggle(); break;
        case "check-for-updates": updaterStore.checkForUpdate().catch((err) => appLogger.warn("app", "Updater manual check failed", err)); break;
        case "about": setHelpPanelVisible(true); break;

        default: {
          // switch-tab-1 through switch-tab-9
          const tabMatch = action.match(/^switch-tab-(\d)$/);
          if (tabMatch) {
            const index = parseInt(tabMatch[1]) - 1;
            const ids = terminalLifecycle.terminalIds();
            if (index < ids.length) terminalLifecycle.handleTerminalSelect(ids[index]);
          }
          break;
        }
      }
    }).then((fn) => { unlisten = fn; }).catch((err) => appLogger.error("app", "Failed to register menu-action listener", err));

    onCleanup(() => unlisten?.());
  });

  // Ctrl+Tab / Ctrl+Shift+Tab — intercepted at Cocoa NSEvent level on macOS
  // because WKWebView swallows these before JS keydown fires.
  createEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("ctrl-tab", (event) => {
      const dir = event.payload;
      if (dir !== "prev" && dir !== "next") return;
      terminalLifecycle.navigateTab(dir);
    }).then((fn) => { unlisten = fn; }).catch((err) => appLogger.error("app", "Failed to register ctrl-tab listener", err));
    onCleanup(() => unlisten?.());
  });

  // Push-to-talk hotkey handler.
  // For Fn/Globe key: listens for Tauri events from the native macOS monitor.
  // For other keys: uses DOM keydown/keyup events.
  // Pauses while capturingHotkey is true so the settings UI can capture a new key.
  createEffect(() => {
    const hotkey = dictationStore.state.hotkey;
    const capturing = dictationStore.state.capturingHotkey;
    const longPressMs = dictationStore.state.longPressMs;
    if (!dictationStore.state.enabled || !hotkey || capturing) return;

    const handler = createLongPressHandlerFromHotkey(hotkey, longPressMs, {
      onStart: () => dictation.handleDictationStart(),
      onStop: () => dictation.handleDictationStop(),
    });
    if (!handler) return;

    let cleanupListeners: () => void;

    if (hotkey === "Fn") {
      // Fn/Globe key: native monitor emits Tauri events (macOS only)
      let cancelled = false;
      let unDown: (() => void) | undefined;
      let unUp: (() => void) | undefined;
      if (isTauri()) {
        listen("fn-key-down", () => {
          handler.handleEvent({ eventType: "KeyPress", key: "Fn" });
        }).then((fn) => { cancelled ? fn() : (unDown = fn); })
          .catch((err) => appLogger.error("dictation", "Failed to listen for fn-key-down", err));
        listen("fn-key-up", () => {
          handler.handleEvent({ eventType: "KeyRelease", key: "Fn" });
        }).then((fn) => { cancelled ? fn() : (unUp = fn); })
          .catch((err) => appLogger.error("dictation", "Failed to listen for fn-key-up", err));
      }
      cleanupListeners = () => { cancelled = true; unDown?.(); unUp?.(); };
    } else {
      // Regular keys: DOM events. event.code naming matches our format.
      const hasModifiers = hotkey.includes("+");
      const onKeyDown = (e: KeyboardEvent) => {
        const consumed = handler.handleEvent({ eventType: "KeyPress", key: e.code });
        if (consumed && (hasModifiers || e.repeat)) {
          e.preventDefault();
        }
      };
      const onKeyUp = (e: KeyboardEvent) => {
        handler.handleEvent({ eventType: "KeyRelease", key: e.code });
      };
      window.addEventListener("keydown", onKeyDown);
      window.addEventListener("keyup", onKeyUp);
      cleanupListeners = () => {
        window.removeEventListener("keydown", onKeyDown);
        window.removeEventListener("keyup", onKeyUp);
      };
    }

    onCleanup(() => {
      handler.cleanup();
      cleanupListeners();
    });
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
    <div id="app" classList={{ "sidebar-hidden": !uiStore.state.sidebarVisible }}>
      <MobileViewBanner />
      {/* Toolbar - drag region spanning full width */}
      <Toolbar
        repoPath={gitOps.currentRepoPath()}
        runCommand={gitOps.activeRunCommand()}
        onBranchClick={() => {
          const activeRepo = repositoriesStore.getActive();
          if (activeRepo?.activeBranch) {
            gitOps.handleOpenRenameBranchDialog(activeRepo.path, activeRepo.activeBranch);
            setRenameBranchDialogVisible(true);
          }
        }}
        onRun={(shiftKey) => gitOps.handleRunCommand(shiftKey, () => setRunCommandDialogVisible(true))}
        onReviewPr={gitOps.handleReviewPr}
        onOpenSettings={() => setSettingsPanelVisible(true)}
      />

      {/* Body: sidebar + main content side by side */}
      <div id="app-body">
        {/* Sidebar - always mounted, hidden via CSS */}
        <Sidebar
          quickSwitcherActive={quickSwitcherVisible()}
          onBranchSelect={gitOps.handleBranchSelect}
          onAddTerminal={gitOps.handleAddTerminalToBranch}
          onRemoveBranch={gitOps.handleRemoveBranch}
          onRenameBranch={(repoPath, branchName) => {
            gitOps.handleOpenRenameBranchDialog(repoPath, branchName);
            setRenameBranchDialogVisible(true);
          }}
          onAddWorktree={gitOps.handleAddWorktree}
          onCreateWorktreeFromBranch={gitOps.handleCreateWorktreeFromBranch}
          onMergeAndArchive={(repoPath, branchName) => {
            const repoState = repositoriesStore.get(repoPath);
            const mainBranch = repoState ? Object.values(repoState.branches).find(b => b.isMain)?.name : undefined;
            if (!mainBranch) {
              setStatusInfo("Cannot merge: no main branch found");
              return;
            }
            const effective = repoSettingsStore.getEffective(repoPath);
            const afterMerge = effective?.afterMerge ?? "archive";
            gitOps.handleMergeAndArchive(repoPath, branchName, mainBranch, afterMerge);
          }}
          creatingWorktreeRepos={gitOps.creatingWorktreeRepos()}
          onAddRepo={gitOps.handleAddRepo}
          onRepoSettings={(repoPath) => gitOps.handleRepoSettings(repoPath, (ctx) => {
            setSettingsContext(ctx);
            setSettingsInitialTab(undefined);
            setSettingsPanelVisible(true);
          })}
          onRemoveRepo={gitOps.handleRemoveRepo}
          onOpenSettings={() => openSettings()}
          onOpenHelp={() => setHelpPanelVisible(true)}
          buildAgentMenuItems={buildSidebarAgentMenuItems}
          onRefreshBranchStats={gitOps.refreshAllBranchStats}
          onCheckoutRemoteBranch={gitOps.handleCheckoutRemoteBranch}
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
            quickSwitcherActive={quickSwitcherVisible()}
            onTabSelect={terminalLifecycle.handleTerminalSelect}
            onTabClose={terminalLifecycle.closeTerminal}
            onCloseOthers={terminalLifecycle.closeOtherTabs}
            onCloseToRight={terminalLifecycle.closeTabsToRight}
            onNewTab={gitOps.handleNewTab}
            onSplitVertical={() => splitPanes.handleSplit("vertical")}
            onSplitHorizontal={() => splitPanes.handleSplit("horizontal")}
            onReorder={(from, to) => {
              const activeRepo = repositoriesStore.getActive();
              if (activeRepo?.activeBranch) {
                repositoriesStore.reorderTerminals(activeRepo.path, activeRepo.activeBranch, from, to);
              }
            }}
            onDetachTab={handleDetachTab}
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
            repoPath={gitOps.currentRepoPath() || null}
            fsRoot={gitOps.activeWorktreePath() || null}
            onFileOpen={(fsRoot, filePath, line) => {
              if ((filePath.endsWith(".md") || filePath.endsWith(".mdx")) && line === undefined) {
                mdTabsStore.add(gitOps.currentRepoPath() || fsRoot, filePath, fsRoot || undefined);
              } else {
                const tabId = editorTabsStore.add(fsRoot, filePath, line);
                terminalLifecycle.handleTerminalSelect(tabId);
              }
            }}
          />
        </TerminalArea>

        {/* Status bar */}
        <StatusBar
          fontSize={terminalLifecycle.activeFontSize()}
          defaultFontSize={getDefaultFontSize()}
          statusInfo={statusInfo()}
          onToggleDiff={() => uiStore.toggleGitPanel()}
          onToggleMarkdown={() => uiStore.toggleMarkdownPanel()}
          onToggleNotes={() => uiStore.toggleNotesPanel()}
          onToggleFileBrowser={() => uiStore.toggleFileBrowserPanel()}
          onToggleErrorLog={() => errorLogStore.toggle()}
          onDictationStart={dictation.handleDictationStart}
          onDictationStop={dictation.handleDictationStop}
          currentRepoPath={globalWorkspaceStore.isActive()
            ? (terminalsStore.state.activeId
              ? repositoriesStore.getRepoPathForTerminal(terminalsStore.state.activeId) ?? undefined
              : undefined)
            : gitOps.currentRepoPath()}
          cwd={gitOps.activeWorktreePath()}
          onBranchRenamed={(oldName, newName) => {
            const repoPath = gitOps.currentRepoPath();
            if (repoPath) {
              repositoriesStore.renameBranch(repoPath, oldName, newName);
            }
            if (gitOps.currentBranch() === oldName) {
              gitOps.setCurrentBranch(newName);
            }
            setStatusInfo(`Renamed branch ${oldName} to ${newName}`);
          }}
          onReviewPr={gitOps.handleReviewPr}
        />
        </main>
      </div>

      {/* Prompt overlay */}
      <PromptOverlay />

      {/* Dictation streaming toast — shows partial transcription */}
      <DictationToast />

      {/* Prompt library drawer */}
      <PromptDrawer />

      {/* Command palette (Tauri only — many actions are Tauri-specific) */}
      <Show when={isTauri()}>
        <CommandPalette actions={actionEntries()} />
      </Show>

      {/* Quick branch switcher */}
      <BranchSwitcher
        activeRepoPath={repositoriesStore.state.activeRepoPath ?? undefined}
        onSelect={(repoPath, branchName) => {
          // If the branch exists in the store (has a worktree), just switch UI view.
          // Otherwise it's a regular branch needing a real git checkout.
          const branch = repositoriesStore.get(repoPath)?.branches[branchName];
          if (branch) {
            gitOps.handleBranchSelect(repoPath, branchName);
          } else {
            gitOps.handleSwitchBranch(repoPath, branchName);
          }
        }}
        onCheckoutRemote={gitOps.handleCheckoutRemoteBranch}
      />

      {/* Activity dashboard */}
      <Suspense>
        <ActivityDashboard onSelect={terminalLifecycle.handleTerminalSelect} />
      </Suspense>

      {/* Worktree manager */}
      <WorktreeManager actions={worktreeActions} />

      {/* MCP servers popup (per-repo) */}
      <McpPopup onOpenSettings={openSettings} />

      {/* Error log panel */}
      <ErrorLogPanel />

      {/* Settings panel */}
      <Suspense>
        <SettingsPanel
          visible={settingsPanelVisible()}
          onClose={() => setSettingsPanelVisible(false)}
          initialTab={settingsInitialTab()}
          context={settingsContext()}
        />
      </Suspense>

      {/* Task queue panel */}
      <TaskQueuePanel
        visible={taskQueueVisible()}
        onClose={() => setTaskQueueVisible(false)}
      />


      {/* Context menu */}
      <ContextMenu
        items={getContextMenuItems()}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />

      {/* Rename branch dialog */}
      <RenameBranchDialog
        visible={renameBranchDialogVisible()}
        currentName={gitOps.branchToRename()?.branchName || ""}
        onClose={() => {
          setRenameBranchDialogVisible(false);
          gitOps.setBranchToRename(null);
        }}
        onRename={gitOps.handleRenameBranch}
      />

      {/* Create worktree dialog */}
      <CreateWorktreeDialog
        visible={gitOps.worktreeDialogState() !== null}
        suggestedName={gitOps.worktreeDialogState()?.suggestedName ?? ""}
        existingBranches={gitOps.worktreeDialogState()?.existingBranches ?? []}
        worktreeBranches={gitOps.worktreeDialogState()?.worktreeBranches ?? []}
        worktreesDir={gitOps.worktreeDialogState()?.worktreesDir ?? ""}
        baseRefs={gitOps.worktreeDialogState()?.baseRefs}
        onGenerateName={gitOps.generateWorktreeName}
        onClose={() => gitOps.setWorktreeDialogState(null)}
        onCreate={gitOps.confirmCreateWorktree}
      />

      {/* Run command dialog */}
      <RunCommandDialog
        visible={runCommandDialogVisible()}
        savedCommand={gitOps.activeRunCommand() || ""}
        onClose={() => setRunCommandDialogVisible(false)}
        onSaveAndRun={(command) => {
          setRunCommandDialogVisible(false);
          gitOps.executeRunCommand(command);
        }}
      />

      {/* Terminal rename prompt */}
      <PromptDialog
        visible={termRenamePromptVisible()}
        title="Terminal Title"
        placeholder="Enter title"
        defaultValue={termRenameDefault()}
        confirmLabel="Rename"
        onClose={() => setTermRenamePromptVisible(false)}
        onConfirm={(newName) => {
          const activeId = terminalsStore.state.activeId;
          if (activeId && newName !== termRenameDefault()) {
            terminalsStore.update(activeId, { name: newName, nameIsCustom: true });
          }
        }}
      />

      {/* Add repository: path prompt (browser mode only) */}
      <PromptDialog
        visible={repoPathPromptVisible()}
        title="Add Repository"
        placeholder="Enter absolute path to repository"
        confirmLabel="Add"
        onClose={() => {
          setRepoPathPromptVisible(false);
          repoPathPromptResolve?.(null);
          repoPathPromptResolve = null;
        }}
        onConfirm={(path) => {
          setRepoPathPromptVisible(false);
          repoPathPromptResolve?.(path);
          repoPathPromptResolve = null;
        }}
      />

      {/* In-app confirm dialog (replaces native OS dialogs) */}
      <ConfirmDialog
        visible={dialogs.dialogState() !== null}
        title={dialogs.dialogState()?.title ?? ""}
        message={dialogs.dialogState()?.message ?? ""}
        confirmLabel={dialogs.dialogState()?.confirmLabel}
        cancelLabel={dialogs.dialogState()?.cancelLabel}
        kind={dialogs.dialogState()?.kind}
        onClose={dialogs.handleClose}
        onConfirm={dialogs.handleConfirm}
      />

      {/* Update download progress dialog */}
      <UpdateProgressDialog />

      {/* Post-merge worktree cleanup — shown when afterMerge=ask */}
      <Show when={gitOps.mergePendingCtx() !== null}>
        {(() => {
          const ctx = gitOps.mergePendingCtx()!;
          const repoState = repositoriesStore.get(ctx.repoPath);
          const activeBranch = repoState?.activeBranch ?? "";
          const isOnBaseBranch = activeBranch === ctx.baseBranch;
          const branchState = repoState?.branches[ctx.branchName];
          const isDefaultBranch = branchState?.isMain ?? false;
          const hasTerminals = (branchState?.terminals.length ?? 0) > 0;
          return (
            <PostMergeCleanupDialog
              branchName={ctx.branchName}
              baseBranch={ctx.baseBranch}
              repoPath={ctx.repoPath}
              isOnBaseBranch={isOnBaseBranch}
              isDefaultBranch={isDefaultBranch}
              hasTerminals={hasTerminals}
              hasDirtyFiles={ctx.hasDirtyFiles}
              worktreeAction={worktreeCleanupAction()}
              onWorktreeActionChange={setWorktreeCleanupAction}
              executing={worktreeCleanupExecuting()}
              stepStatuses={worktreeCleanupStepStatuses()}
              stepErrors={worktreeCleanupStepErrors()}
              onExecute={handleWorktreeCleanupExecute}
              onSkip={handleWorktreeCleanupSkip}
            />
          );
        })()}
      </Show>

      {/* Help panel (Story 053) */}
      <Suspense>
        <HelpPanel
          visible={helpPanelVisible()}
          onClose={() => setHelpPanelVisible(false)}
        />
      </Suspense>

      {/* Quit confirmation dialog (Story 057) */}
      <Show when={quitDialogVisible()}>
        <div class="quit-dialog-overlay" onClick={() => setQuitDialogVisible(false)}>
          <div class="quit-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>Quit TUICommander?</h3>
            <p>
              You have {terminalsStore.getIds().filter((id) => terminalsStore.get(id)?.sessionId).length} active
              terminal session(s). Quitting will close all sessions.
            </p>
            <div class="quit-dialog-actions">
              <button class="quit-dialog-cancel" onClick={() => setQuitDialogVisible(false)}>
                Cancel
              </button>
              <button class="quit-dialog-quit" onClick={forceQuit}>
                Quit
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default App;
