import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";

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

  // Close all PTY sessions on app exit
  window.addEventListener("beforeunload", () => {
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
    const firstPath = repoPaths[0];
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
        await deps.handleBranchSelect(firstPath, firstRepo.activeBranch);
      }
      return;
    }
  }

  // Create first terminal only if repos exist but no surviving sessions
  if (repoPaths.length > 0 && survivingSessions.length === 0) {
    await deps.createNewTerminal();
  }
}
