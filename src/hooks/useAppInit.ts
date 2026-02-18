import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { invoke, listen } from "../invoke";
import { AGENTS } from "../agents";
import type { SavedTerminal } from "../types";

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
  createNewTerminal: () => Promise<string | undefined>;
  getDefaultFontSize: () => number;
  stores: {
    hydrate: () => Promise<void>;
    startPolling: () => void;
    stopPolling: () => void;
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
        snapshots.get(repoPath)!.set(branchName, saved);
      }
    }
  }

  return snapshots;
}

/** Restore terminal sessions from persisted savedTerminals in each branch */
async function restoreTerminalSessions(deps: AppInitDeps): Promise<boolean> {
  let restoredAny = false;
  let firstTerminalId: string | null = null;
  let firstRepoPath: string | null = null;
  let firstBranchName: string | null = null;

  for (const repoPath of repositoriesStore.getPaths()) {
    const repo = repositoriesStore.get(repoPath);
    if (!repo) continue;

    for (const [branchName, branch] of Object.entries(repo.branches)) {
      const saved = branch.savedTerminals;
      if (!saved || saved.length === 0) continue;

      for (const terminal of saved) {
        const id = terminalsStore.add({
          sessionId: null,
          fontSize: terminal.fontSize,
          name: terminal.name,
          cwd: terminal.cwd,
          awaitingInput: null,
        });

        repositoriesStore.addTerminalToBranch(repoPath, branchName, id);

        // Set pending resume command if this was an agent terminal
        if (terminal.agentType) {
          const agentConfig = AGENTS[terminal.agentType];
          if (agentConfig?.resumeCommand) {
            terminalsStore.update(id, { pendingResumeCommand: agentConfig.resumeCommand });
          }
        }

        if (!firstTerminalId) {
          firstTerminalId = id;
          firstRepoPath = repoPath;
          firstBranchName = branchName;
        }
        restoredAny = true;
      }
    }
  }

  // Consume-once: clear savedTerminals so they don't re-restore on crash
  if (restoredAny) {
    repositoriesStore.clearSavedTerminals();
  }

  // Activate the first restored terminal and its repo/branch
  if (firstTerminalId && firstRepoPath && firstBranchName) {
    terminalsStore.setActive(firstTerminalId);
    repositoriesStore.setActive(firstRepoPath);
    deps.setCurrentRepoPath(firstRepoPath);
    deps.setCurrentBranch(firstBranchName);
    repositoriesStore.setActiveBranch(firstRepoPath, firstBranchName);
  }

  return restoredAny;
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

    // 2. Close all PTY sessions
    for (const id of terminalsStore.getIds()) {
      const t = terminalsStore.get(id);
      if (t?.sessionId) {
        deps.pty.close(t.sessionId).catch(() => {});
      }
    }
  });

  // Hydrate all stores from Rust backend
  try {
    await deps.stores.hydrate();
  } catch {
    deps.setStatusInfo("Warning: 1 store(s) failed to load");
  }

  // Start HEAD file watchers for all known repos and listen for branch changes
  for (const repoPath of repositoriesStore.getPaths()) {
    invoke("start_head_watcher", { repoPath }).catch((err) =>
      console.warn(`[HeadWatcher] Failed to start for ${repoPath}:`, err),
    );
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
    invoke("clear_caches").catch(() => {});
  }).catch(() => {});

  // Check for surviving PTY sessions (persists across Vite HMR reloads)
  const survivingSessions = await deps.pty.listActiveSessions();

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
        // Try to restore terminals from saved state before creating new ones
        const restored = await restoreTerminalSessions(deps);
        if (!restored) {
          await deps.handleBranchSelect(firstPath, firstRepo.activeBranch);
        }
      }
      return;
    }
  }

  // Create first terminal only if repos exist but no surviving sessions and no restored sessions
  if (repoPaths.length > 0 && survivingSessions.length === 0) {
    const restored = await restoreTerminalSessions(deps);
    if (!restored) {
      await deps.createNewTerminal();
    }
  }
}
