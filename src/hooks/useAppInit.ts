import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { githubStore } from "../stores/github";
import { invoke, listen } from "../invoke";
import { isTauri } from "../transport";
import type { SavedTerminal } from "../types";

/** Track PTY sessions created by the browser client so we only close our own on unload */
export const browserCreatedSessions = new Set<string>();

/** Dependencies injected into initApp */
export interface AppInitDeps {
  pty: {
    listActiveSessions: () => Promise<Array<{ session_id: string; cwd: string | null }>>;
    close: (sessionId: string) => Promise<void>;
  };
  setLazygitAvailable: (available: boolean) => void;
  setQuitDialogVisible: (visible: boolean) => void;
  setStatusInfo: (msg: string) => void;
  setCurrentRepoPath: (path: string | undefined) => void;
  setCurrentBranch: (branch: string | null) => void;
  handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
  refreshAllBranchStats: () => void;
  getDefaultFontSize: () => number;
  stores: {
    hydrate: () => Promise<void>;
    startPolling: () => void;
    stopPolling: () => void;
    startPrNotificationTimer: () => void;
    loadFontFromConfig: () => void;
    refreshDictationConfig: () => Promise<void>;
  };
  detectBinary: (binary: string) => Promise<{ path: string | null; version: string | null }>;
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
  console.log("[SolidJS] App mounted");

  const platform = deps.applyPlatformClass();
  console.log(`[Platform] Detected: ${platform}`);

  // Detect lazygit binary (Story 048)
  try {
    const detection = await deps.detectBinary("lazygit");
    deps.setLazygitAvailable(detection.path !== null);
    if (detection.path) {
      console.log(`[Lazygit] Found: ${detection.path} (${detection.version ?? "unknown version"})`);
    } else {
      console.log("[Lazygit] Not found — lazygit features disabled");
    }
  } catch {
    console.warn("[Lazygit] Detection failed");
    deps.setLazygitAvailable(false);
  }

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

  // Snapshot terminal metadata and close all PTY sessions on app exit
  window.addEventListener("beforeunload", () => {
    // 1. Snapshot terminal metadata per repo/branch before closing
    const snapshots = collectTerminalSnapshots();
    if (snapshots.size > 0) {
      repositoriesStore.snapshotTerminals(snapshots);
    }

    // 2. Close PTY sessions
    if (isTauri()) {
      // Tauri owns all sessions — close them all
      for (const id of terminalsStore.getIds()) {
        const t = terminalsStore.get(id);
        if (t?.sessionId) {
          deps.pty.close(t.sessionId).catch((err) =>
            console.warn(`Failed to close PTY session ${t.sessionId} on unload:`, err),
          );
        }
      }
    } else {
      // Browser only closes sessions it created — leave Tauri-created ones alive
      for (const sid of browserCreatedSessions) {
        deps.pty.close(sid).catch(() => {});
      }
    }
  });

  // Hydrate all stores from Rust backend
  try {
    await deps.stores.hydrate();
  } catch {
    deps.setStatusInfo("Warning: 1 store(s) failed to load");
  }

  // Remove splash screen now that stores are hydrated — prevents flash of empty
  // state (e.g. "Add Repository" button) before persisted repos have loaded.
  document.getElementById("splash")?.remove();

  // Start HEAD and repo file watchers for all known repos (Tauri-only: no HTTP equivalent)
  if (isTauri()) {
    for (const repoPath of repositoriesStore.getPaths()) {
      invoke("start_head_watcher", { repoPath }).catch((err) =>
        console.warn(`[HeadWatcher] Failed to start for ${repoPath}:`, err),
      );
      invoke("start_repo_watcher", { repoPath }).catch((err) =>
        console.warn(`[RepoWatcher] Failed to start for ${repoPath}:`, err),
      );
    }
  }

  listen<{ repo_path: string; branch: string }>("head-changed", (event) => {
    const { repo_path, branch } = event.payload;
    const repo = repositoriesStore.get(repo_path);
    if (!repo) return;

    // Only update if branch actually changed
    if (repo.activeBranch === branch) return;

    console.log(`[HeadWatcher] ${repo_path}: branch changed to ${branch}`);

    // Ensure the branch exists in the store
    if (!repo.branches[branch]) {
      repositoriesStore.setBranch(repo_path, branch, { name: branch });
    }

    repositoriesStore.setActiveBranch(repo_path, branch);

    // Invalidate caches so next poll fetches fresh data
    invoke("clear_caches").catch((err) =>
      console.debug("[Cache] Failed to clear caches:", err),
    );
  }).catch((err) =>
    console.error("[HeadWatcher] Failed to register head-changed listener:", err),
  );

  // Listen for .git/ directory changes (index, refs, etc.) to refresh panels
  listen<{ repo_path: string }>("repo-changed", (event) => {
    const { repo_path } = event.payload;
    // Invalidate caches so panels fetch fresh data
    invoke("clear_caches").catch((err) =>
      console.debug("[Cache] Failed to clear caches:", err),
    );
    // Bump revision counter — panels tracking this signal will re-fetch
    repositoriesStore.bumpRevision(repo_path);
    // Trigger immediate PR refresh (debounced 2s to coalesce rapid git events)
    githubStore.pollRepo(repo_path);
  }).catch((err) =>
    console.error("[RepoWatcher] Failed to register repo-changed listener:", err),
  );

  // Listen for sessions created/closed by remote clients (browser UI)
  if (isTauri()) {
    listen<{ session_id: string; cwd: string | null }>("session-created", (event) => {
      const { session_id, cwd } = event.payload;
      // Skip if this session is already tracked (created locally)
      const existing = terminalsStore.getIds().find(
        (id) => terminalsStore.get(id)?.sessionId === session_id,
      );
      if (existing) return;

      console.log(`[Remote] Session created: ${session_id}`);
      const id = terminalsStore.add({
        sessionId: session_id,
        fontSize: deps.getDefaultFontSize(),
        name: `Remote ${terminalsStore.getCount() + 1}`,
        cwd: cwd ?? null,
        awaitingInput: null,
      });

      // Match to repo/branch by cwd
      if (cwd) {
        const matchedRepo = repositoriesStore.getPaths().find((repoPath) => {
          if (cwd === repoPath) return true;
          const repoState = repositoriesStore.get(repoPath);
          if (!repoState) return false;
          return Object.values(repoState.branches).some(
            (b) => b.worktreePath && cwd === b.worktreePath,
          );
        });

        if (matchedRepo) {
          const repoState = repositoriesStore.get(matchedRepo);
          const branchName =
            Object.values(repoState?.branches || {}).find(
              (b) => b.worktreePath && cwd === b.worktreePath,
            )?.name || repoState?.activeBranch;

          if (branchName) {
            repositoriesStore.addTerminalToBranch(matchedRepo, branchName, id);
          }
        }
      }
    }).catch((err) =>
      console.error("[Remote] Failed to register session-created listener:", err),
    );

    listen<{ session_id: string }>("session-closed", (event) => {
      const { session_id } = event.payload;
      const termId = terminalsStore.getIds().find(
        (id) => terminalsStore.get(id)?.sessionId === session_id,
      );
      if (termId) {
        console.log(`[Remote] Session closed: ${session_id}`);
        terminalsStore.remove(termId);
      }
    }).catch((err) =>
      console.error("[Remote] Failed to register session-closed listener:", err),
    );
  }

  // Check for surviving PTY sessions (persists across Vite HMR reloads)
  let survivingSessions: Awaited<ReturnType<typeof deps.pty.listActiveSessions>> = [];
  try {
    survivingSessions = await deps.pty.listActiveSessions();
  } catch (err) {
    console.warn("[PTY] Failed to list active sessions (server unreachable or auth failure):", err);
  }

  // Clear stale terminal IDs from previous session
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }

  // Re-adopt surviving PTY sessions or start fresh
  if (survivingSessions.length > 0) {
    console.log(`[PTY Reconnect] Found ${survivingSessions.length} surviving session(s)`);
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

  // Refresh git stats for persisted repos
  deps.refreshAllBranchStats();

  // Start batch PR/CI polling for all repos
  deps.stores.startPolling();

  // Start PR notification focus timer (auto-dismiss after 5 min focused)
  deps.stores.startPrNotificationTimer();

  // Load font preference from Rust config (single source of truth)
  deps.stores.loadFontFromConfig();

  // Load dictation config from disk
  deps.stores.refreshDictationConfig();

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
          terminalsStore.setActive(validTerminals[0]);
        } else {
          await deps.handleBranchSelect(firstPath, firstRepo.activeBranch);
        }
      } else {
        // Lazy restore: don't create terminals on startup.
        // savedTerminals stay in the branch store and will be restored
        // when the user clicks the branch in the sidebar.
      }
      return;
    }
  }

  // Lazy restore: don't create terminals on startup.
  // Terminals are restored when user clicks a branch in the sidebar.
}
