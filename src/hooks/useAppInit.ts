import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { githubStore } from "../stores/github";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import { activityStore } from "../stores/activityStore";
import { repoSettingsStore } from "../stores/repoSettings";
import { paneLayoutStore } from "../stores/paneLayout";
import { assignTabToActiveGroup } from "../utils/paneTabAssign";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { invoke, listen } from "../invoke";
import { isTauri } from "../transport";
import type { SavedTerminal } from "../types";

/** Track PTY sessions created by the browser client so we only close our own on unload */
export const browserCreatedSessions = new Set<string>();

/** Remote (MCP) sessionId → termId. Persists even after Terminal.tsx nulls sessionId
 *  on exit, so the session-closed listener can find the tab to auto-remove. */
const remoteSessionTabs = new Map<string, string>();

/** Delay before auto-removing a remote tab after the backend reports session-closed.
 *  Gives the user time to see "[Process exited]" in the terminal before it vanishes. */
const REMOTE_TAB_AUTOCLOSE_MS = 30_000;
/** Shorter delay for agent-spawned sessions — they finish their task and can be cleaned up faster. */
const AGENT_TAB_AUTOCLOSE_MS = 10_000;

/** Dependencies injected into initApp */
export interface AppInitDeps {
  pty: {
    listActiveSessions: () => Promise<Array<{ session_id: string; cwd: string | null }>>;
    close: (sessionId: string) => Promise<void>;
  };
  setQuitDialogVisible: (visible: boolean) => void;
  setStatusInfo: (msg: string) => void;
  setCurrentRepoPath: (path: string | undefined) => void;
  setCurrentBranch: (branch: string | null) => void;
  handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
  refreshAllBranchStats: () => Promise<void> | void;
  getDefaultFontSize: () => number;
  stores: {
    hydrate: () => Promise<void>;
    startPolling: () => void;
    stopPolling: () => void;
    startAutoFetch: () => void;
    startPrNotificationTimer: () => void;
    loadFontFromConfig: () => void;
    refreshDictationConfig: () => Promise<void>;
    startUserActivityListening: () => void;
  };
  applyPlatformClass: () => string;
  onCloseRequested: (handler: (event: { preventDefault: () => void }) => void) => void;
}

/** Collect terminal metadata from all repos/branches for persistence */
function collectTerminalSnapshots(): Map<string, Map<string, SavedTerminal[]>> {
  const snapshots = new Map<string, Map<string, SavedTerminal[]>>();

  for (const repoPath of repositoriesStore.getPaths()) {
    const repo = repositoriesStore.get(repoPath);
    if (!repo) continue;

    for (const [branchName, branch] of Object.entries(repo.branches)) {
      if (branch.terminals.length === 0) continue;

      const saved: SavedTerminal[] = [];
      for (const termId of branch.terminals) {
        const t = terminalsStore.get(termId);
        if (!t) continue;
        saved.push({
          name: t.name,
          cwd: t.cwd,
          fontSize: t.fontSize,
          agentType: t.agentType,
          agentSessionId: t.agentSessionId ?? null,
          tuicSession: t.tuicSession ?? null,
        });
      }

      if (saved.length > 0) {
        if (!snapshots.has(repoPath)) {
          snapshots.set(repoPath, new Map());
        }
        const branchMap = snapshots.get(repoPath);
        if (branchMap) branchMap.set(branchName, saved);
      }
    }
  }

  return snapshots;
}

/** App initialization: hydrate stores, reconnect PTY sessions, restore state */
export async function initApp(deps: AppInitDeps) {
  appLogger.info("app", `initApp called — existing terminals: [${terminalsStore.getIds().join(", ")}]`);
  appLogger.debug("app", "SolidJS App mounted");

  const platform = deps.applyPlatformClass();
  appLogger.debug("app", `Platform detected: ${platform}`);

  // Intercept window close for quit confirmation (Story 057)
  deps.onCloseRequested((event) => {
    if (!settingsStore.state.confirmBeforeQuit) return;
    const activeTerminals = terminalsStore.getIds().filter(
      (id) => terminalsStore.get(id)?.sessionId,
    );
    if (activeTerminals.length > 0) {
      event.preventDefault();
      deps.setQuitDialogVisible(true);
    }
  });

  // Periodic terminal snapshot — ensures savedTerminals is always fresh
  // so app restart recovers terminals even if beforeunload fails.
  const SNAPSHOT_INTERVAL_MS = 30_000;
  const snapshotTimer = setInterval(() => {
    const snapshots = collectTerminalSnapshots();
    if (snapshots.size > 0) {
      repositoriesStore.snapshotTerminals(snapshots);
    }
  }, SNAPSHOT_INTERVAL_MS);

  // Snapshot terminal metadata, flush pending saves, and close PTY sessions on app exit
  window.addEventListener("beforeunload", () => {
    clearInterval(snapshotTimer);
    activityStore.flushSave();

    // 1. Snapshot terminal metadata per repo/branch before closing
    const snapshots = collectTerminalSnapshots();
    if (snapshots.size > 0) {
      repositoriesStore.snapshotTerminals(snapshots);
    }

    // 2. Close PTY sessions — but NOT in Tauri mode during webview reloads
    // (Vite HMR, manual reload). The Rust backend survives the reload and
    // list_active_sessions will re-adopt the surviving sessions on re-init.
    // In Tauri, real quit is handled by the close-requested handler which
    // calls app.exit() — beforeunload during quit is a no-op for PTY cleanup.
    if (!isTauri()) {
      // Browser only closes sessions it created — leave Tauri-created ones alive
      for (const sid of browserCreatedSessions) {
        deps.pty.close(sid).catch(() => {});
      }
    }
  });

  // Hydrate all stores from Rust backend
  try {
    await deps.stores.hydrate();
  } catch (err) {
    appLogger.error("app", "Store hydration failed", err);
    deps.setStatusInfo("Warning: store(s) failed to load");
  }

  // Load .tuic.json local configs for all repos (fire-and-forget, non-blocking)
  for (const repoPath of repositoriesStore.getPaths()) {
    repoSettingsStore.loadLocalConfig(repoPath).catch(() => {});
  }

  // Recover log entries from Rust backend (survives webview reloads)
  appLogger.hydrateFromRust().catch(() => {});

  // Restore pane layout from disk (terminal tabs will be re-linked during terminal restore)
  await paneLayoutStore.loadFromDisk();

  // Remove splash screen now that stores are hydrated — prevents flash of empty
  // state (e.g. "Add Repository" button) before persisted repos have loaded.
  document.getElementById("splash")?.remove();

  // Repo watchers are started by the Rust setup closure (instant with raw notify).
  // No frontend invoke needed — avoids IPC contention during hydration.

  listen<{ repo_path: string; branch: string }>("head-changed", (event) => {
    const { repo_path, branch } = event.payload;
    const repo = repositoriesStore.get(repo_path);
    if (!repo) return;

    // Only update if branch actually changed
    if (repo.activeBranch === branch) return;

    appLogger.info("app", `HeadWatcher: ${repo_path} branch changed to ${branch}`);

    const oldBranch = repo.activeBranch;
    const oldBranchState = oldBranch ? repo.branches[oldBranch] : null;

    const isMainCheckout = oldBranch && oldBranchState &&
      (oldBranchState.worktreePath === null || oldBranchState.worktreePath === repo_path);

    if (isMainCheckout) {
      // Main checkout (not a worktree): rename the single branch entry so
      // terminals, savedTerminals, hadTerminals etc. carry over seamlessly.
      if (!repo.branches[branch]) {
        // Happy path: new branch doesn't exist yet — simple rename.
        repositoriesStore.renameBranch(repo_path, oldBranch, branch);
      } else {
        // Race: refreshAllBranchStats already created the new branch entry.
        // Merge terminal state from old → new, then remove the old entry.
        repositoriesStore.mergeBranchState(repo_path, oldBranch, branch);
        repositoriesStore.removeBranch(repo_path, oldBranch);
        repositoriesStore.setActiveBranch(repo_path, branch);
      }
    } else {
      // Worktree branch — just ensure target exists and activate it.
      if (!repo.branches[branch]) {
        repositoriesStore.setBranch(repo_path, branch, { name: branch });
      }
      repositoriesStore.setActiveBranch(repo_path, branch);
    }

    // Invalidate caches for this repo so next poll fetches fresh data
    invoke("clear_repo_caches", { path: repo_path }).catch((err) =>
      appLogger.debug("app", "Failed to clear repo caches", err),
    );
  }).catch((err) =>
    appLogger.error("app", "Failed to register head-changed listener", err),
  );

  // Listen for .git/ directory changes (index, refs, etc.) to refresh panels
  let branchStatsTimer: ReturnType<typeof setTimeout> | null = null;
  // Track the in-flight refresh so we can extend the debounce window when
  // another repo-changed arrives while one is already running. FSEvents often
  // fires a burst (worktree delete hits both .git/worktrees/ and the removed
  // directory), and back-to-back refreshes would double-close terminals and
  // thrash store subscriptions. Extended debounce + dedup (in refreshAllBranchStats)
  // collapses the burst into a single run without forcing a UI reset.
  let activeRefresh: Promise<void> | null = null;
  listen<{ repo_path: string }>("repo-changed", (event) => {
    const { repo_path } = event.payload;
    // Invalidate caches for this repo so panels fetch fresh data
    invoke("clear_repo_caches", { path: repo_path }).catch((err) =>
      appLogger.debug("app", "Failed to clear repo caches", err),
    );
    // Reload .tuic.json (may have changed)
    repoSettingsStore.loadLocalConfig(repo_path).catch(() => {});
    // Trigger immediate PR refresh (debounced 2s to coalesce rapid git events)
    githubStore.pollRepo(repo_path);
    // Signal panels to re-fetch on EVERY event. Coalescing the bump into the
    // setTimeout below loses updates when rapid events clear the pending timer,
    // leaving panels stuck on stale data (story 1277-31a0).
    repositoriesStore.bumpRevision(repo_path);
    // Discover external worktree changes. Use 500ms when idle, 1000ms when a
    // refresh is already running so the next one doesn't race it. Only the
    // branch-stats refresh is debounced; the revision bump above is not.
    const delay = activeRefresh !== null ? 1000 : 500;
    if (branchStatsTimer) clearTimeout(branchStatsTimer);
    branchStatsTimer = setTimeout(() => {
      branchStatsTimer = null;
      const result = deps.refreshAllBranchStats();
      if (result && typeof (result as Promise<void>).then === "function") {
        activeRefresh = (result as Promise<void>).finally(() => {
          activeRefresh = null;
        });
      }
    }, delay);
  }).catch((err) =>
    appLogger.error("app", "Failed to register repo-changed listener", err),
  );

  // Listen for MCP toast notifications from the Rust backend
  listen<{ title: string; message: string | null; level: string; sound: boolean | null }>("mcp-toast", (event) => {
    const { title, message, level, sound } = event.payload;
    const safeLevel = (level === "warn" || level === "error") ? level : "info";
    toastsStore.add(title, message ?? "", safeLevel, sound === true);
  }).catch((err) =>
    appLogger.error("app", "Failed to register mcp-toast listener", err),
  );

  // Listen for sessions created/closed by remote clients (browser UI or other Tauri windows)
  listen<{ session_id: string; cwd: string | null; agent_type?: string | null }>("session-created", (event) => {
    const { session_id, cwd, agent_type } = event.payload;
    // Skip if this session was created by the local browser client or is already tracked
    if (browserCreatedSessions.has(session_id)) return;
    const existing = terminalsStore.getIds().find(
      (id) => terminalsStore.get(id)?.sessionId === session_id,
    );
    if (existing) return;

    appLogger.info("app", `Remote session created: ${session_id}`);
    const id = terminalsStore.add({
      sessionId: session_id,
      fontSize: deps.getDefaultFontSize(),
      name: `PTY: Session ${terminalsStore.getCount() + 1}`,
      cwd: cwd ?? null,
      awaitingInput: null,
      isRemote: true,
    });
    remoteSessionTabs.set(session_id, id);

    // Match to repo/branch by cwd (ancestor path matching)
    let assigned = false;
    if (cwd) {
      const cwdNorm = cwd.endsWith("/") ? cwd : cwd + "/";
      const matchedRepo = repositoriesStore.getPaths().find((repoPath) => {
        const repoNorm = repoPath.endsWith("/") ? repoPath : repoPath + "/";
        // cwd is the repo root or a subdirectory of it
        if (cwd === repoPath || cwdNorm.startsWith(repoNorm)) return true;
        // cwd is a worktree path or subdirectory of one
        const repoState = repositoriesStore.get(repoPath);
        if (!repoState) return false;
        return Object.values(repoState.branches).some(
          (b) => b.worktreePath && (cwd === b.worktreePath || cwdNorm.startsWith(b.worktreePath + "/")),
        );
      });

      if (matchedRepo) {
        const repoState = repositoriesStore.get(matchedRepo);
        // Try worktree match first, then fall back to active branch
        const branchName =
          Object.values(repoState?.branches || {}).find(
            (b) => b.worktreePath && (cwd === b.worktreePath || cwdNorm.startsWith(b.worktreePath + "/")),
          )?.name || repoState?.activeBranch;

        if (branchName) {
          repositoriesStore.addTerminalToBranch(matchedRepo, branchName, id);
          assigned = true;
        }
      }
    }

    // Fallback: no repo matched cwd — assign to the currently active repo/branch
    if (!assigned) {
      const fallbackRepo = repositoriesStore.state.activeRepoPath;
      const fallbackState = fallbackRepo ? repositoriesStore.get(fallbackRepo) : undefined;
      const fallbackBranch = fallbackState?.activeBranch;
      if (fallbackRepo && fallbackBranch) {
        appLogger.warn("app", `Remote session ${session_id}: cwd "${cwd ?? "(null)"}" did not match any repo — falling back to active repo/branch`);
        repositoriesStore.addTerminalToBranch(fallbackRepo, fallbackBranch, id);
      } else {
        appLogger.error("app", `Remote session ${session_id}: no repo/branch to assign tab to — tab will be invisible`);
      }
    }

    // Auto-focus agent-spawned tabs so swarm workers are immediately visible.
    // Only activate when agent_type is present (MCP agent spawn), not for
    // manually created sessions which should stay in the background.
    if (agent_type) {
      // In split mode, ensure there is an active group so assignTabToActiveGroup
      // doesn't silently no-op and leave the tab invisible.
      if (paneLayoutStore.isSplit() && !paneLayoutStore.state.activeGroupId) {
        const leafIds = paneLayoutStore.getAllGroupIds();
        if (leafIds.length > 0) {
          paneLayoutStore.setActiveGroup(leafIds[0]);
        }
      }
      assignTabToActiveGroup(id, "terminal");
      // Only steal focus when there is no existing active terminal.
      if (!terminalsStore.state.activeId) {
        terminalsStore.setActive(id);
      }
    }
  }).catch((err) =>
    appLogger.error("app", "Failed to register session-created listener", err),
  );

  // Listen for UI tab open/update requests from MCP tools
  listen<{ id: string; title: string; html: string; pinned: boolean; url?: string; focus?: boolean; origin_repo_path?: string }>("ui-tab", (event) => {
    const { id, title, html, pinned, url, focus, origin_repo_path } = event.payload;

    // Intercept tuic:// protocol URLs — handle as commands, not iframe src
    if (url?.startsWith("tuic://")) {
      try {
        const parsed = new URL(url);
        const cmd = parsed.hostname; // "open", "edit", "terminal"
        const filePath = decodeURIComponent(parsed.pathname).replace(/^\//, "");
        if (!filePath && cmd !== "terminal") return;

        const activeRepoPath = repositoriesStore.state.activeRepoPath;
        // Resolve: absolute path → find owning repo, relative → active repo
        let repoPath: string | null = null;
        let relPath = filePath;
        if (filePath.startsWith("/")) {
          const repos = repositoriesStore.getPaths();
          repoPath = repos.find((rp) => filePath.startsWith(rp + "/") || filePath === rp) ?? null;
          if (repoPath) relPath = filePath.slice(repoPath.length + 1);
        } else {
          repoPath = activeRepoPath ?? null;
        }

        if (cmd === "open" && repoPath) {
          mdTabsStore.add(repoPath, relPath);
        } else if (cmd === "edit") {
          const line = parseInt(parsed.searchParams.get("line") || "0", 10);
          if (repoPath) {
            editorTabsStore.add(repoPath, relPath, line || undefined);
          } else if (filePath.startsWith("/")) {
            editorTabsStore.add("__external__", filePath, line || undefined, { externalEditable: true });
          } else {
            appLogger.warn("app", `tuic://edit relative path without active repo: ${filePath}`);
          }
        } else {
          appLogger.warn("app", `tuic:// unhandled: cmd=${cmd} path=${filePath} repo=${repoPath}`);
        }
      } catch (err) {
        appLogger.warn("app", `tuic:// URL parse error: ${url}`, err);
      }
      return;
    }

    mdTabsStore.openUiTab(id, title, html, pinned, url, focus ?? true, origin_repo_path);
  }).catch((err) =>
    appLogger.error("app", "Failed to register ui-tab listener", err),
  );

  // Keep remoteSessionTabs consistent if the user closes a remote tab manually
  // before the backend session-closed event arrives.
  terminalsStore.onRemove((termId) => {
    for (const [sid, tid] of remoteSessionTabs) {
      if (tid === termId) remoteSessionTabs.delete(sid);
    }
  });

  listen<{ session_id: string; agent_type?: string }>("session-closed", (event) => {
    const { session_id, agent_type } = event.payload;
    // Prefer the persistent remoteSessionTabs map: the store's reverse map may
    // have been cleared already by Terminal.tsx resetting sessionId on pty-exit.
    const termId = remoteSessionTabs.get(session_id)
      ?? terminalsStore.getTerminalForSession(session_id);
    if (!termId) return;

    remoteSessionTabs.delete(session_id);

    // Countdown + auto-remove is only for MCP-spawned (remote) tabs. Locally-created
    // tabs are managed by Terminal.tsx's pty-exit handler — applying the rename
    // here would leave the name stuck forever because the ticker's isRemote
    // guard aborts on the first tick and the setTimeout's isRemote guard skips removal.
    const t0 = terminalsStore.get(termId);
    if (!t0?.isRemote) return;

    terminalsStore.update(termId, { shellState: "exited" });

    // Agent-spawned sessions get a shorter grace period — they finish their task
    // and can be cleaned up faster than manually-opened remote sessions.
    const autoCloseMs = agent_type ? AGENT_TAB_AUTOCLOSE_MS : REMOTE_TAB_AUTOCLOSE_MS;

    appLogger.info("app", `Remote session closed: ${session_id} — tab ${termId} auto-close in ${autoCloseMs}ms`);

    // Countdown in the tab name so the user sees when it will vanish
    const baseName = t0?.name ?? termId;
    let remaining = Math.round(autoCloseMs / 1000);
    terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` });
    const ticker = setInterval(() => {
      remaining--;
      const t = terminalsStore.get(termId);
      if (!t || !t.isRemote || remaining <= 0) {
        clearInterval(ticker);
        return;
      }
      terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` });
    }, 1000);

    setTimeout(() => {
      clearInterval(ticker);
      const t = terminalsStore.get(termId);
      // Only remove if the tab still exists and is still the remote tab for this
      // session (user may have closed it manually or re-used the slot).
      if (t && t.isRemote) {
        appLogger.info("app", `Auto-removing remote tab ${termId} for closed session ${session_id}`);
        terminalsStore.remove(termId);
      }
    }, autoCloseMs);
  }).catch((err) =>
    appLogger.error("app", "Failed to register session-closed listener", err),
  );

  // Close HTML tabs whose creator session has exited
  listen<{ tab_ids: string[] }>("close-html-tabs", (event) => {
    for (const pluginId of event.payload.tab_ids) {
      mdTabsStore.closeUiTab(pluginId);
    }
  }).catch((err) =>
    appLogger.error("app", "Failed to register close-html-tabs listener", err),
  );

  // Check for surviving PTY sessions (persists across Vite HMR reloads)
  let survivingSessions: Awaited<ReturnType<typeof deps.pty.listActiveSessions>> = [];
  try {
    survivingSessions = await deps.pty.listActiveSessions();
  } catch (err) {
    appLogger.warn("app", "Failed to list active sessions (server unreachable or auth failure)", err);
  }

  // Clear stale terminal IDs from previous session
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }

  // Re-adopt surviving PTY sessions or start fresh
  if (survivingSessions.length > 0) {
    appLogger.info("app", `PTY reconnect: found ${survivingSessions.length} surviving session(s)`);
    for (const session of survivingSessions) {
      const id = terminalsStore.add({
        sessionId: session.session_id,
        fontSize: deps.getDefaultFontSize(),
        name: `Terminal ${terminalsStore.getCount() + 1}`,
        cwd: session.cwd,
        awaitingInput: null,
      });

      // Match session to repo/branch by cwd
      const matchedRepo = repositoriesStore.getPaths().find((repoPath) => {
        if (session.cwd === repoPath) return true;
        const repoState = repositoriesStore.get(repoPath);
        if (!repoState) return false;
        return Object.values(repoState.branches).some(
          (b) => b.worktreePath && session.cwd === b.worktreePath,
        );
      });

      if (matchedRepo) {
        const repoState = repositoriesStore.get(matchedRepo);
        const branchName =
          Object.values(repoState?.branches || {}).find(
            (b) => b.worktreePath && session.cwd === b.worktreePath,
          )?.name || repoState?.activeBranch;

        if (branchName) {
          repositoriesStore.addTerminalToBranch(matchedRepo, branchName, id);
        }
      }
    }
    terminalsStore.setActive(terminalsStore.getIds()[0]);
  }

  // Ensure non-git repos have a shell branch (migration for repos persisted
  // before the shell-branch feature existed, or added via external paths).
  for (const repoPath of repositoriesStore.getPaths()) {
    const repo = repositoriesStore.get(repoPath);
    if (repo && repo.isGitRepo === false && Object.keys(repo.branches).length === 0) {
      const shellBranch = "shell";
      repositoriesStore.setBranch(repoPath, shellBranch, {
        worktreePath: repoPath,
        isMain: true,
        isShell: true,
      });
      repositoriesStore.setActiveBranch(repoPath, shellBranch);
    }
  }

  // Refresh git stats for persisted repos
  deps.refreshAllBranchStats();

  // Start batch PR/CI polling for all repos
  deps.stores.startPolling();

  // Start per-repo auto-fetch timers
  deps.stores.startAutoFetch();

  // Start PR notification focus timer (auto-dismiss after 5 min focused)
  deps.stores.startPrNotificationTimer();

  // Load font preference from Rust config (single source of truth)
  deps.stores.loadFontFromConfig();

  // Load dictation config from disk
  deps.stores.refreshDictationConfig();

  // Start tracking user activity (click/keydown) for PR display timeouts
  deps.stores.startUserActivityListening();

  // Restore active repo/branch from persisted state
  const repoPaths = repositoriesStore.getPaths();
  if (repoPaths.length > 0) {
    // Use persisted active repo, falling back to first
    const persistedActive = repositoriesStore.state.activeRepoPath;
    const firstPath = (persistedActive && repoPaths.includes(persistedActive)) ? persistedActive : repoPaths[0];
    const firstRepo = repositoriesStore.get(firstPath);
    repositoriesStore.setActive(firstPath);
    deps.setCurrentRepoPath(firstPath);
    if (firstRepo?.activeBranch) {
      deps.setCurrentBranch(firstRepo.activeBranch);
      if (survivingSessions.length > 0) {
        const branch = firstRepo.branches[firstRepo.activeBranch];
        const validTerminals = branch?.terminals.filter((id) =>
          terminalsStore.getIds().includes(id),
        ) || [];
        if (validTerminals.length > 0) {
          const remembered = branch?.lastActiveTerminal;
          const target = (remembered && validTerminals.includes(remembered)) ? remembered : validTerminals[0];
          appLogger.info("terminal", `initApp RESTORE activeTerminal=${target} (remembered=${remembered}, valid=${JSON.stringify(validTerminals)})`);
          terminalsStore.setActive(target);
        } else {
          await deps.handleBranchSelect(firstPath, firstRepo.activeBranch);
        }
      } else {
        // Eagerly restore terminals when a pane layout was loaded from disk —
        // the layout references terminal IDs that must exist for panes to render.
        // Without this, the split layout shows empty boxes after a fresh start.
        await deps.handleBranchSelect(firstPath, firstRepo.activeBranch);
      }
      return;
    }
  }

  // Lazy restore: don't create terminals on startup.
  // Terminals are restored when user clicks a branch in the sidebar.
}
