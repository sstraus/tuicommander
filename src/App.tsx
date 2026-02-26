import {
  Component,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { Terminal } from "./components/Terminal";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { TabBar } from "./components/TabBar";
import { StatusBar } from "./components/StatusBar";
import { TerminalArea } from "./components/TerminalArea";
import { PanelOrchestrator } from "./components/PanelOrchestrator";
import { editorTabsStore } from "./stores/editorTabs";
import { PromptOverlay } from "./components/PromptOverlay";
import { PromptDrawer } from "./components/PromptDrawer";
import { SettingsPanel, type SettingsContext } from "./components/SettingsPanel";
import { TaskQueuePanel } from "./components/TaskQueuePanel";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "./components/ContextMenu";
import { GitOperationsPanel } from "./components/GitOperationsPanel";
import { RenameBranchDialog } from "./components/RenameBranchDialog";
import { CreateWorktreeDialog } from "./components/CreateWorktreeDialog";
import { PromptDialog } from "./components/PromptDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { RunCommandDialog } from "./components/RunCommandDialog";
import { HelpPanel } from "./components/HelpPanel";
import { CommandPalette } from "./components/CommandPalette";
import { ActivityDashboard } from "./components/ActivityDashboard";
import { ErrorLogPanel } from "./components/ErrorLogPanel";
import { commandPaletteStore } from "./stores/commandPalette";
import { activityDashboardStore } from "./stores/activityDashboard";
import { errorLogStore } from "./stores/errorLog";
import { appLogger } from "./stores/appLogger";
import { getActionEntries } from "./actions/actionRegistry";
import { promptLibraryStore } from "./stores/promptLibrary";
import { terminalsStore } from "./stores/terminals";
import { repositoriesStore } from "./stores/repositories";
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
import { initPlugins } from "./plugins";
import { usePty } from "./hooks/usePty";
import { useRepository } from "./hooks/useRepository";
import { useKeyboardRedirect } from "./hooks/useKeyboardRedirect";
import { useConfirmDialog } from "./hooks/useConfirmDialog";
import { useTerminalLifecycle } from "./hooks/useTerminalLifecycle";
import { useGitOperations } from "./hooks/useGitOperations";
import { useAppLazygit } from "./hooks/useAppLazygit";
import { useDictation } from "./hooks/useDictation";
import { useQuickSwitcher } from "./hooks/useQuickSwitcher";
import { useSplitPanes } from "./hooks/useSplitPanes";
import { useAgentPolling } from "./hooks/useAgentPolling";
import { useAgentDetection } from "./hooks/useAgentDetection";
import { agentConfigsStore } from "./stores/agentConfigs";
import { AGENTS } from "./agents";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { initApp } from "./hooks/useAppInit";
import { applyAppTheme } from "./themes";
import { hotkeyToTauriShortcut, isValidHotkey } from "./utils";
import {
  register as registerShortcut,
  unregister as unregisterShortcut,
  isRegistered as isShortcutRegistered,
} from "@tauri-apps/plugin-global-shortcut";
import { applyPlatformClass, getModifierSymbol, isQuickSwitcherActive, isQuickSwitcherRelease } from "./platform";

import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, listen } from "./invoke";
import { isTauri } from "./transport";
import { setLastMenuActionTime } from "./menuDedup";
import { initDeepLinkHandler } from "./deep-link-handler";

const getDefaultFontSize = () => settingsStore.state.defaultFontSize;
const getMaxTabNameLength = () => settingsStore.state.maxTabNameLength;

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
  const [gitOpsPanelVisible, setGitOpsPanelVisible] = createSignal(false);

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

  // Context menu state
  const contextMenu = createContextMenu();

  const pty = usePty();
  const repo = useRepository();
  const dialogs = useConfirmDialog();

  // Redirect keyboard input from sidebar to terminal
  useKeyboardRedirect(true);

  const terminalLifecycle = useTerminalLifecycle({
    pty,
    dialogs,
    setStatusInfo,
    getDefaultFontSize,
  });

  const gitOps = useGitOperations({
    repo,
    pty,
    dialogs,
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

  const lazygit = useAppLazygit({
    pty,
    getCurrentRepoPath: gitOps.currentRepoPath,
    getDefaultFontSize,
  });

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

  // Poll active terminal for foreground agent detection
  useAgentPolling();

  // Agent detection for context menu
  const agentDetection = useAgentDetection();

  // Stop GitHub polling on component teardown — registered at body level so
  // SolidJS can track it synchronously (onCleanup inside async onMount is unreliable).
  onCleanup(() => githubStore.stopPolling());

  onMount(async () => {
    await initApp({
      pty,
      setLazygitAvailable: lazygit.setLazygitAvailable,
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
        startPrNotificationTimer: prNotificationsStore.startFocusTimer,
        loadFontFromConfig: settingsStore.loadFontFromConfig,
        refreshDictationConfig: () => dictationStore.refreshConfig().then(() => {
          if (dictationStore.state.enabled) {
            dictationStore.refreshStatus();
          }
        }),
        startUserActivityListening: userActivityStore.startListening,
      },
      detectBinary: (binary) => invoke("detect_agent_binary", { binary }),
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
    initDeepLinkHandler({ openSettings });
  });


  // Apply the active theme to the entire app chrome (sidebar, tabs, toolbar, etc.)
  createEffect(() => applyAppTheme(settingsStore.state.theme));

  // Enforce mutual exclusivity between tab stores.
  // When a non-terminal tab becomes active (e.g. from mdTabsStore.add()),
  // deactivate the terminal so its pane hides and xterm releases focus.
  createEffect(() => {
    if (mdTabsStore.state.activeId) {
      terminalsStore.setActive(null);
      diffTabsStore.setActive(null);
      editorTabsStore.setActive(null);
    }
  });
  createEffect(() => {
    if (diffTabsStore.state.activeId) {
      terminalsStore.setActive(null);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
    }
  });
  createEffect(() => {
    if (editorTabsStore.state.activeId) {
      terminalsStore.setActive(null);
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
    }
  });
  createEffect(() => {
    if (terminalsStore.state.activeId) {
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
    }
  });

  // Prevent system sleep while any terminal is busy (Story 258)
  createEffect(() => {
    if (!isTauri()) return;
    const enabled = settingsStore.state.preventSleepWhenBusy;
    const terminals = terminalsStore.state.terminals;
    const anyBusy = Object.values(terminals).some((t) => t.shellState === "busy");

    if (enabled && anyBusy) {
      invoke("block_sleep").catch((err) =>
        appLogger.warn("app", "Failed to block sleep", err),
      );
    } else {
      invoke("unblock_sleep").catch((err) =>
        appLogger.warn("app", "Failed to unblock sleep", err),
      );
    }
  });

  // Initialize plugin system
  onMount(() => {
    initPlugins();
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
  const buildAgentMenuItems = (): ContextMenuItem[] => {
    const available = agentDetection.getAvailable();
    if (available.length === 0) return [];

    return available.map((agent) => {
      const agentConfig = AGENTS[agent.type];
      const runConfigs = agentConfigsStore.getRunConfigs(agent.type);

      // Build sub-items: one per run config, or a single "(Default)" if none
      const children: ContextMenuItem[] = runConfigs.length > 0
        ? runConfigs.map((rc) => ({
            label: rc.name + (rc.is_default ? " (Default)" : ""),
            action: () => {
              const active = terminalsStore.getActive();
              if (!active?.ref) return;
              const cmd = [rc.command, ...rc.args].join(" ");
              active.ref.write(`${cmd}\r`);
              terminalsStore.update(active.id, { name: agentConfig.name, nameIsCustom: true });
            },
          }))
        : [{
            label: "(Default)",
            action: () => {
              const active = terminalsStore.getActive();
              if (!active?.ref) return;
              active.ref.write(`${agentConfig.binary}\r`);
              terminalsStore.update(active.id, { name: agentConfig.name, nameIsCustom: true });
            },
          }];

      return {
        label: agentConfig.name,
        action: () => {},
        children,
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
          terminalsStore.update(termId, {
            name: agentConfig.name,
            nameIsCustom: true,
            pendingResumeCommand: cmd,
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
  const isSplit = () => terminalsStore.state.layout.direction !== "none" && terminalsStore.state.layout.panes.length === 2;

  /** Check if the active terminal has a running agent (disables agent submenu) */
  const activeTerminalBusy = (): boolean => {
    const activeId = terminalsStore.state.activeId;
    if (!activeId) return true;
    const term = terminalsStore.get(activeId);
    return !!term?.agentType;
  };

  const getContextMenuItems = createMemo((): ContextMenuItem[] => [
    { label: "Copy", shortcut: `${getModifierSymbol()}C`, action: terminalLifecycle.copyFromTerminal },
    { label: "Paste", shortcut: `${getModifierSymbol()}V`, action: terminalLifecycle.pasteToTerminal },
    { label: "Split Right", shortcut: `${getModifierSymbol()}\\`, action: () => splitPanes.handleSplit("vertical"), disabled: isSplit() },
    { label: "Split Left", action: () => splitPanes.handleSplit("vertical"), disabled: isSplit() },
    { label: "Split Down", shortcut: `${getModifierSymbol()}${"\u2325"}\\`, action: () => splitPanes.handleSplit("horizontal"), disabled: isSplit() },
    { label: "Split Up", action: () => splitPanes.handleSplit("horizontal"), disabled: isSplit(), separator: true },
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
    ...(lazygit.lazygitAvailable() ? [{ label: "Open Lazygit", shortcut: `${getModifierSymbol()}G`, action: lazygit.spawnLazygit, separator: true }] : []),
    ...(agentDetection.getAvailable().length > 0 ? [{
      label: "Agents",
      action: () => {},
      disabled: activeTerminalBusy(),
      children: buildAgentMenuItems(),
      separator: true,
    }] : []),
    {
      label: "Close Terminal",
      shortcut: `${getModifierSymbol()}W`,
      action: () => {
        const activeId = terminalsStore.state.activeId;
        if (activeId) terminalLifecycle.closeTerminal(activeId);
      },
      separator: true,
    },
  ]);

  /** Open a file path from terminal output — .md/.mdx in MD viewer, others in internal editor */
  const handleOpenFilePath = (absolutePath: string, _line?: number, _col?: number) => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath) return;

    if (absolutePath.endsWith(".md") || absolutePath.endsWith(".mdx")) {
      mdTabsStore.add(repoPath, absolutePath);
      uiStore.setMarkdownPanelVisible(true);
    } else {
      const tabId = editorTabsStore.add(repoPath, absolutePath);
      terminalLifecycle.handleTerminalSelect(tabId);
    }
  };



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
        setStatusInfo(output ? `git ${op}: ${output}` : `git ${op} completed`);
        appLogger.info("app", `[Notify] completion — git ${op} succeeded`);
        notificationsStore.playCompletion();
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

  // Shared shortcut handlers used by both keyboard shortcuts and command palette
  const shortcutHandlers = {
    zoomIn: terminalLifecycle.zoomIn,
    zoomOut: terminalLifecycle.zoomOut,
    zoomReset: terminalLifecycle.zoomReset,
    createNewTerminal: terminalLifecycle.createNewTerminal,
    closeTerminal: terminalLifecycle.closeTerminal,
    reopenClosedTab: terminalLifecycle.reopenClosedTab,
    navigateTab: terminalLifecycle.navigateTab,
    clearTerminal: terminalLifecycle.clearTerminal,
    terminalIds: terminalLifecycle.terminalIds,
    handleTerminalSelect: terminalLifecycle.handleTerminalSelect,
    handleSplit: splitPanes.handleSplit,
    handleRunCommand: (forceDialog: boolean) => gitOps.handleRunCommand(forceDialog, () => setRunCommandDialogVisible(true)),
    switchToBranchByIndex: quickSwitcher.switchToBranchByIndex,
    isQuickSwitcherOpen: quickSwitcherVisible,
    lazygitAvailable: lazygit.lazygitAvailable,
    spawnLazygit: lazygit.spawnLazygit,
    openLazygitPane: lazygit.openLazygitPane,
    toggleDiffPanel: uiStore.toggleDiffPanel,
    toggleMarkdownPanel: uiStore.toggleMarkdownPanel,
    toggleSidebar: uiStore.toggleSidebar,
    togglePromptLibrary: promptLibraryStore.toggleDrawer,
    toggleSettings: () => setSettingsPanelVisible((v) => !v),
    toggleTaskQueue: () => setTaskQueueVisible((v) => !v),
    toggleGitOpsPanel: () => setGitOpsPanelVisible((v) => !v),
    toggleHelpPanel: () => setHelpPanelVisible((v) => !v),
    toggleNotesPanel: uiStore.toggleNotesPanel,
    toggleFileBrowserPanel: uiStore.toggleFileBrowserPanel,
    findInTerminal: () => {
      const active = terminalsStore.getActive();
      active?.ref?.openSearch();
    },
    toggleCommandPalette: () => commandPaletteStore.toggle(),
    toggleActivityDashboard: () => activityDashboardStore.toggle(),
    toggleErrorLog: () => errorLogStore.toggle(),
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
          const layout = terminalsStore.state.layout;
          if (layout.direction !== "none" && layout.panes.length === 2) {
            const closingIndex = layout.activePaneIndex;
            const closingId = layout.panes[closingIndex];
            terminalsStore.closeSplitPane(closingIndex);
            if (closingId) terminalLifecycle.closeTerminal(closingId, true);
            const survivorId = terminalsStore.state.layout.panes[0];
            if (survivorId) {
              terminalsStore.setActive(survivorId);
              requestAnimationFrame(() => terminalsStore.get(survivorId)?.ref?.focus());
            }
          } else {
            const activeId = terminalsStore.state.activeId;
            if (activeId) terminalLifecycle.closeTerminal(activeId);
          }
          break;
        }
        case "reopen-closed-tab": terminalLifecycle.reopenClosedTab(); break;
        case "settings": setSettingsPanelVisible((v) => !v); break;

        // Edit
        case "clear-terminal": terminalLifecycle.clearTerminal(); break;

        // View
        case "toggle-sidebar": uiStore.toggleSidebar(); break;
        case "split-right": splitPanes.handleSplit("vertical"); break;
        case "split-down": splitPanes.handleSplit("horizontal"); break;
        case "zoom-in": terminalLifecycle.zoomIn(); break;
        case "zoom-out": terminalLifecycle.zoomOut(); break;
        case "zoom-reset": terminalLifecycle.zoomReset(); break;
        case "diff-panel": uiStore.toggleDiffPanel(); break;
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
        case "lazygit": if (lazygit.lazygitAvailable()) lazygit.spawnLazygit(); break;
        case "lazygit-split": if (lazygit.lazygitAvailable()) lazygit.openLazygitPane(); break;
        case "git-operations": setGitOpsPanelVisible((v) => !v); break;
        case "task-queue": setTaskQueueVisible((v) => !v); break;

        // Help
        case "help-panel": setHelpPanelVisible((v) => !v); break;
        case "command-palette": commandPaletteStore.toggle(); break;
        case "activity-dashboard": activityDashboardStore.toggle(); break;
        case "error-log": errorLogStore.toggle(); break;
        case "check-for-updates": updaterStore.checkForUpdate().catch((err) => appLogger.warn("app", "Updater manual check failed", err)); break;
        case "about": openSettings("about"); break;

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

  // Push-to-talk hotkey handler via OS-level global shortcut
  // Uses tauri-plugin-global-shortcut so the hotkey works even without window focus.
  // Unregisters while capturingHotkey is true so the browser can capture the keypress.
  createEffect(() => {
    if (!isTauri()) return;
    const hotkey = dictationStore.state.hotkey;
    const capturing = dictationStore.state.capturingHotkey;
    if (!dictationStore.state.enabled || !hotkey || capturing) return;
    if (!isValidHotkey(hotkey)) return;

    const shortcut = hotkeyToTauriShortcut(hotkey);
    let registered = false;

    const setup = async () => {
      try {
        if (await isShortcutRegistered(shortcut)) {
          await unregisterShortcut(shortcut);
        }
        await registerShortcut(shortcut, (event) => {
          if (event.state === "Pressed") {
            dictation.handleDictationStart();
          } else if (event.state === "Released") {
            dictation.handleDictationStop();
          }
        });
        registered = true;
      } catch (err) {
        appLogger.error("dictation", "Failed to register push-to-talk shortcut", err);
      }
    };

    setup().catch((err) =>
      appLogger.error("app", "Failed to register push-to-talk shortcut", err),
    );

    onCleanup(() => {
      if (registered) {
        unregisterShortcut(shortcut).catch((err) =>
          appLogger.warn("dictation", "Failed to unregister push-to-talk shortcut", err),
        );
      }
    });
  });


  return (
    <div id="app" classList={{ "sidebar-hidden": !uiStore.state.sidebarVisible }}>
      {/* Toolbar - drag region spanning full width */}
      <Toolbar
        repoPath={gitOps.currentRepoPath()}
        runCommand={gitOps.activeRunCommand()}
        quickSwitcherActive={quickSwitcherVisible()}
        onBranchClick={() => {
          const activeRepo = repositoriesStore.getActive();
          if (activeRepo?.activeBranch) {
            gitOps.handleOpenRenameBranchDialog(activeRepo.path, activeRepo.activeBranch);
            setRenameBranchDialogVisible(true);
          }
        }}
        onRun={(shiftKey) => gitOps.handleRunCommand(shiftKey, () => setRunCommandDialogVisible(true))}
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
          onSwitchBranch={gitOps.handleSwitchBranch}
          switchBranchLists={gitOps.switchBranchLists()}
          currentBranches={gitOps.currentBranches()}
          onBackgroundGit={handleBackgroundGit}
          runningGitOps={runningGitOps()}
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
          />
        </div>

        {/* Terminal container - render ALL terminals so they never unmount (preserves PTY sessions) */}
        <TerminalArea
          onTerminalFocus={terminalLifecycle.handleTerminalFocus}
          onCloseTab={terminalLifecycle.closeTerminal}
          onOpenFilePath={handleOpenFilePath}
          onContextMenu={contextMenu.open}
          lazygitPaneVisible={lazygit.lazygitPaneVisible()}
          lazygitTermId={lazygit.lazygitTermId()}
          lazygitFloating={lazygit.lazygitFloating()}
          lazygitRepoPath={gitOps.currentRepoPath() || null}
          lazygitCmd={gitOps.currentRepoPath() ? lazygit.buildLazygitCmd(gitOps.currentRepoPath()!) : null}
          onLazygitFloat={() => lazygit.setLazygitFloating(true)}
          onLazygitClose={lazygit.closeLazygitPane}
        >
          {/* Side panels (right panes inside #terminal-container) */}
          <PanelOrchestrator
            repoPath={gitOps.currentRepoPath() || null}
            onFileOpen={(repoPath, filePath) => {
              if (filePath.endsWith(".md") || filePath.endsWith(".mdx")) {
                mdTabsStore.add(repoPath, filePath);
                uiStore.setMarkdownPanelVisible(true);
              } else {
                const tabId = editorTabsStore.add(repoPath, filePath);
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
          quickSwitcherActive={quickSwitcherVisible()}
          onToggleDiff={() => uiStore.toggleDiffPanel()}
          onToggleMarkdown={() => uiStore.toggleMarkdownPanel()}
          onToggleNotes={() => uiStore.toggleNotesPanel()}
          onToggleFileBrowser={() => uiStore.toggleFileBrowserPanel()}
          onToggleErrorLog={() => errorLogStore.toggle()}
          onDictationStart={dictation.handleDictationStart}
          onDictationStop={dictation.handleDictationStop}
          currentRepoPath={gitOps.currentRepoPath()}
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
        />
        </main>
      </div>

      {/* Prompt overlay */}
      <PromptOverlay />

      {/* Prompt library drawer */}
      <PromptDrawer />

      {/* Command palette (Tauri only — many actions are Tauri-specific) */}
      <Show when={isTauri()}>
        <CommandPalette actions={actionEntries()} />
      </Show>

      {/* Activity dashboard */}
      <ActivityDashboard />

      {/* Error log panel */}
      <ErrorLogPanel />

      {/* Settings panel */}
      <SettingsPanel
        visible={settingsPanelVisible()}
        onClose={() => setSettingsPanelVisible(false)}
        initialTab={settingsInitialTab()}
        context={settingsContext()}
      />

      {/* Task queue panel */}
      <TaskQueuePanel
        visible={taskQueueVisible()}
        onClose={() => setTaskQueueVisible(false)}
      />

      {/* Git operations panel */}
      <GitOperationsPanel
        visible={gitOpsPanelVisible()}
        repoPath={gitOps.currentRepoPath() || null}
        currentBranch={gitOps.currentBranch()}
        repoStatus={gitOps.repoStatus()}
        onClose={() => setGitOpsPanelVisible(false)}
        onBranchChange={() => {
          // Refresh repo info after git operation
          if (gitOps.currentRepoPath()) {
            repo.getInfo(gitOps.currentRepoPath()!).then((info) => {
              gitOps.setCurrentBranch(info.branch);
              gitOps.setRepoStatus(info.status === "not-git" ? "unknown" : info.status);
            }).catch((err) => {
              appLogger.error("git", "Failed to refresh repo info", err);
              gitOps.setRepoStatus("unknown");
            });
          }
        }}
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

      {/* Lazygit floating window (Story 051) */}
      <Show when={lazygit.lazygitFloating() && lazygit.lazygitPaneVisible() && lazygit.lazygitTermId()}>
        <div class="lazygit-floating">
          <div class="lazygit-floating-header">
            <span class="lazygit-pane-title">
              <span>⎇</span> lazygit (floating)
            </span>
            <div style={{ display: "flex", gap: "4px" }}>
              <button
                class="lazygit-pane-close"
                onClick={() => lazygit.setLazygitFloating(false)}
                title="Dock (reattach)"
              >
                ⇲
              </button>
              <button class="lazygit-pane-close" onClick={lazygit.closeLazygitPane}>
                &times;
              </button>
            </div>
          </div>
          <div class="lazygit-floating-content">
            <Terminal
              id={lazygit.lazygitTermId()!}
              cwd={gitOps.currentRepoPath() || null}
              alwaysVisible
            />
          </div>
        </div>
      </Show>

      {/* Help panel (Story 053) */}
      <HelpPanel
        visible={helpPanelVisible()}
        onClose={() => setHelpPanelVisible(false)}
        onOpenShortcuts={() => {
          setHelpPanelVisible(false);
          openSettings("shortcuts");
        }}
      />

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
