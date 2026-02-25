import { createSignal, batch } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { appLogger } from "../stores/appLogger";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauri } from "../transport";
import { findOrphanTerminals } from "../utils/terminalOrphans";
import { filterValidTerminals } from "../utils/terminalFilter";
import { AGENTS } from "../agents";
import type { WorktreeCreateOptions } from "../components/CreateWorktreeDialog";

/** Dependencies injected into useGitOperations */
export interface GitOperationsDeps {
  repo: {
    getInfo: (path: string) => Promise<{ path: string; name: string; initials: string; branch: string; status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown"; is_git_repo: boolean }>;
    getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
    getWorktreePaths: (repoPath: string) => Promise<Record<string, string>>;
    removeWorktree: (repoPath: string, branchName: string) => Promise<void>;
    createWorktree: (baseRepo: string, branchName: string, createBranch?: boolean, baseRef?: string) => Promise<{ name: string; path: string; branch: string; base_repo: string }>;
    renameBranch: (repoPath: string, oldName: string, newName: string) => Promise<void>;
    generateWorktreeName: (existingNames: string[]) => Promise<string>;
    generateCloneBranchName: (sourceBranch: string, existingNames: string[]) => Promise<string>;
    listBaseRefOptions: (repoPath: string) => Promise<string[]>;
    listLocalBranches: (repoPath: string) => Promise<string[]>;
  };
  pty: {
    canSpawn: () => Promise<boolean>;
    write: (sessionId: string, data: string) => Promise<void>;
    getWorktreesDir: () => Promise<string>;
  };
  dialogs: {
    confirmRemoveRepo: (repoName: string) => Promise<boolean>;
    confirmRemoveWorktree: (branchName: string) => Promise<boolean>;
  };
  closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
  createNewTerminal: () => Promise<string | undefined>;
  setStatusInfo: (msg: string) => void;
  getDefaultFontSize: () => number;
  getMaxTabNameLength: () => number;
}

/** Git and repository operations extracted from App.tsx */
export function useGitOperations(deps: GitOperationsDeps) {
  const [currentRepoPath, setCurrentRepoPath] = createSignal<string | undefined>(undefined);
  const [currentBranch, setCurrentBranch] = createSignal<string | null>(null);
  const [repoStatus, setRepoStatus] = createSignal<"clean" | "dirty" | "conflict" | "merge" | "unknown">("unknown");
  const [branchToRename, setBranchToRename] = createSignal<{ repoPath: string; branchName: string } | null>(null);
  const [creatingWorktreeRepos, setCreatingWorktreeRepos] = createSignal<Set<string>>(new Set());
  const [worktreeDialogState, setWorktreeDialogState] = createSignal<{
    repoPath: string;
    suggestedName: string;
    existingBranches: string[];
    worktreeBranches: string[];
    worktreesDir: string;
    baseRefs: string[];
  } | null>(null);

  const refreshAllBranchStats = async () => {
    await Promise.all(repositoriesStore.getPaths().map(async (repoPath) => {
      const repo = repositoriesStore.get(repoPath);
      if (!repo) return;
      // Skip non-git directories — no worktrees or branch stats to refresh
      if (repo.isGitRepo === false) return;

      const showAllBranches = repositoriesStore.get(repoPath)?.showAllBranches ?? false;
      const [worktreePaths, localBranches] = await Promise.all([
        deps.repo.getWorktreePaths(repoPath),
        showAllBranches ? deps.repo.listLocalBranches(repoPath) : Promise.resolve([] as string[]),
      ]);

      const currentRepo = repositoriesStore.get(repoPath);
      if (!currentRepo) return;

      const branchCount = Object.keys(currentRepo.branches).length;
      // Guard: an empty worktree response when branches exist likely indicates a
      // backend error. Clearing branches in that case would destroy UI state.
      if (Object.keys(worktreePaths).length === 0 && branchCount > 0) {
        console.warn(`[refreshAllBranchStats] getWorktreePaths returned empty for ${repoPath} with ${branchCount} branch(es) — skipping clear`);
        return;
      }

      // Compute the target set of branches to keep, then apply all
      // mutations in a single batch to prevent intermediate renders
      // (which caused the sidebar to flash/jump during refresh).
      const localSet = new Set(localBranches);
      const toRemove: string[] = [];
      for (const branchName of Object.keys(currentRepo.branches)) {
        if (!(branchName in worktreePaths) && !localSet.has(branchName)) {
          toRemove.push(branchName);
        }
      }

      batch(() => {
        for (const branchName of toRemove) {
          repositoriesStore.removeBranch(repoPath, branchName);
        }
        for (const [branchName, wtPath] of Object.entries(worktreePaths)) {
          repositoriesStore.setBranch(repoPath, branchName, { worktreePath: wtPath });
        }
        if (showAllBranches) {
          const updatedRepo = repositoriesStore.get(repoPath);
          for (const branchName of localBranches) {
            if (!(branchName in worktreePaths) && !updatedRepo?.branches[branchName]) {
              repositoriesStore.setBranch(repoPath, branchName, { worktreePath: null });
            }
          }
        }
      });

      const updatedRepo = repositoriesStore.get(repoPath);
      if (!updatedRepo) return;

      await Promise.all(Object.values(updatedRepo.branches).map(async (branch) => {
        if (!branch.worktreePath) return;
        try {
          const stats = await deps.repo.getDiffStats(branch.worktreePath);
          repositoriesStore.updateBranchStats(repoPath, branch.name, stats.additions, stats.deletions);
        } catch {
          // Ignore stats errors for individual branches
        }
      }));
    }));
  };

  const handleAddTerminalToBranch = async (repoPath: string, branchName: string) => {
    const canSpawn = await deps.pty.canSpawn();
    if (!canSpawn) {
      deps.setStatusInfo("Max sessions reached (50)");
      return;
    }

    const branch = repositoriesStore.get(repoPath)?.branches[branchName];
    const termCount = branch?.terminals.length || 0;

    const id = terminalsStore.add({
      sessionId: null,
      fontSize: deps.getDefaultFontSize(),
      name: `${branchName.split("/").pop()} ${termCount + 1}`,
      cwd: branch?.worktreePath || null,
      awaitingInput: null,
    });

    repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
    terminalsStore.setActive(id);
    return id;
  };

  const handleBranchSelect = async (repoPath: string, branchName: string) => {
    console.log(`[BranchSelect] Switching to ${branchName}`);

    // Save the current active terminal for the branch we're leaving
    const prevRepo = repositoriesStore.getActive();
    if (prevRepo?.activeBranch) {
      const currentActiveId = terminalsStore.state.activeId;
      if (currentActiveId) {
        const prevBranch = prevRepo.branches[prevRepo.activeBranch];
        if (prevBranch?.terminals.includes(currentActiveId)) {
          repositoriesStore.setBranch(prevRepo.path, prevRepo.activeBranch, { lastActiveTerminal: currentActiveId });
        }
      }
    }

    setCurrentRepoPath(repoPath);
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setActiveBranch(repoPath, branchName);
    setCurrentBranch(branchName);

    const selectedBranch = repositoriesStore.get(repoPath)?.branches[branchName];
    if (selectedBranch?.worktreePath) {
      try {
        const stats = await deps.repo.getDiffStats(selectedBranch.worktreePath);
        repositoriesStore.updateBranchStats(repoPath, branchName, stats.additions, stats.deletions);
      } catch {
        // Ignore stats errors
      }
    }

    const branch = repositoriesStore.get(repoPath)?.branches[branchName];
    const validTerminals = filterValidTerminals(branch?.terminals, terminalsStore.getIds());
    appLogger.info("terminal", `BranchSelect → ${branchName}`, { branchTerminals: branch?.terminals, storeIds: terminalsStore.getIds(), valid: validTerminals, hadTerminals: branch?.hadTerminals, savedTerminals: branch?.savedTerminals?.length ?? 0 });
    if (validTerminals.length === 0 && (branch?.terminals?.length ?? 0) > 0) {
      appLogger.warn("terminal", `BranchSelect MISMATCH: branch has terminals ${JSON.stringify(branch?.terminals)} but none found in store ${JSON.stringify(terminalsStore.getIds())}. Will create fresh terminal.`);
    }

    if (validTerminals.length > 0) {
      // Restore the last active terminal for this branch, or fall back to first
      const remembered = branch?.lastActiveTerminal;
      if (remembered && validTerminals.includes(remembered)) {
        terminalsStore.setActive(remembered);
      } else {
        terminalsStore.setActive(validTerminals[0]);
      }
    } else if (branch?.savedTerminals && branch.savedTerminals.length > 0) {
      // Lazy restore: create terminals from persisted session state
      let firstId: string | null = null;
      for (const terminal of branch.savedTerminals) {
        const id = terminalsStore.add({
          sessionId: null,
          fontSize: terminal.fontSize,
          name: terminal.name,
          cwd: terminal.cwd,
          awaitingInput: null,
        });
        repositoriesStore.addTerminalToBranch(repoPath, branchName, id);

        if (terminal.agentType) {
          const agentConfig = AGENTS[terminal.agentType];
          if (agentConfig?.resumeCommand) {
            terminalsStore.update(id, { pendingResumeCommand: agentConfig.resumeCommand });
          }
        }

        if (!firstId) firstId = id;
      }
      // Clear savedTerminals for this branch (consume-once)
      repositoriesStore.setBranch(repoPath, branchName, { savedTerminals: [] });
      if (firstId) terminalsStore.setActive(firstId);
    } else if (!branch?.hadTerminals) {
      // First time selecting this branch — auto-spawn a terminal
      await handleAddTerminalToBranch(repoPath, branchName);
    } else {
      // hadTerminals && no valid terminals → user closed them all, show empty state.
      // Clear activeId so the previous branch's terminal doesn't bleed through.
      terminalsStore.setActive(null);
    }

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        terminalsStore.getActive()?.ref?.focus();
      });
    });
  };

  const handleRemoveRepo = async (repoPath: string) => {
    const repoState = repositoriesStore.get(repoPath);
    if (!repoState) return;

    const confirmed = await deps.dialogs.confirmRemoveRepo(repoState.displayName);
    if (!confirmed) return;

    for (const branch of Object.values(repoState.branches)) {
      for (const termId of branch.terminals) {
        await deps.closeTerminal(termId, true);
      }
    }

    repositoriesStore.remove(repoPath);

    if (currentRepoPath() === repoPath) {
      setCurrentRepoPath(undefined);
      setCurrentBranch(null);
    }

    deps.setStatusInfo(`Removed ${repoState.displayName}`);

    if (terminalsStore.getCount() === 0) {
      await deps.createNewTerminal();
    }
  };

  const handleRemoveBranch = async (repoPath: string, branchName: string) => {
    const repoState = repositoriesStore.get(repoPath);
    const branch = repoState?.branches[branchName];
    if (!branch?.worktreePath) {
      deps.setStatusInfo(`Cannot remove ${branchName}: not a worktree`);
      return;
    }

    const confirmed = await deps.dialogs.confirmRemoveWorktree(branchName);
    if (!confirmed) return;

    for (const termId of branch.terminals) {
      await deps.closeTerminal(termId, true);
    }

    try {
      await deps.repo.removeWorktree(repoPath, branchName);
      deps.setStatusInfo(`Removed ${branchName}`);
    } catch (err) {
      console.warn("Backend worktree removal failed (cleaning up UI anyway):", err);
      deps.setStatusInfo(`Removed ${branchName} (git cleanup may be needed)`);
    }

    repositoriesStore.removeBranch(repoPath, branchName);
  };

  const handleOpenRenameBranchDialog = (repoPath: string, branchName: string) => {
    setBranchToRename({ repoPath, branchName });
  };

  const handleRenameBranch = async (oldName: string, newName: string) => {
    const branch = branchToRename();
    if (!branch) return;

    try {
      await deps.repo.renameBranch(branch.repoPath, oldName, newName);
    } catch (err) {
      appLogger.error("git", "Failed to rename branch", err);
      deps.setStatusInfo(`Failed to rename branch: ${err}`);
      return;
    }

    repositoriesStore.renameBranch(branch.repoPath, oldName, newName);

    if (currentBranch() === oldName) {
      setCurrentBranch(newName);
    }

    deps.setStatusInfo(`Renamed branch ${oldName} to ${newName}`);
  };

  const activeWorktreePath = () => {
    const activeRepo = repositoriesStore.getActive();
    if (!activeRepo?.activeBranch) return undefined;
    return activeRepo.branches[activeRepo.activeBranch]?.worktreePath || activeRepo.path;
  };

  const activeRunCommand = () => {
    const activeRepo = repositoriesStore.getActive();
    if (!activeRepo?.activeBranch) return undefined;
    return activeRepo.branches[activeRepo.activeBranch]?.runCommand;
  };

  const handleAddRepo = async () => {
    let path: string | null = null;

    if (isTauri()) {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Repository Folder",
      });
      if (!selected) return;
      path = typeof selected === "string" ? selected : selected[0];
    } else {
      // Browser mode: no native file picker — prompt for path
      const input = window.prompt("Enter the absolute path to the repository:");
      if (!input?.trim()) return;
      path = input.trim();
    }

    if (!path) return;

    try {
      const info = await deps.repo.getInfo(path);

      // Close orphan terminals (not associated with any branch)
      const branchTerminalMap: Record<string, string[]> = {};
      for (const repoPath of repositoriesStore.getPaths()) {
        const repoState = repositoriesStore.get(repoPath);
        if (repoState) {
          for (const branch of Object.values(repoState.branches)) {
            branchTerminalMap[`${repoPath}:${branch.name}`] = branch.terminals;
          }
        }
      }
      const orphanTerminals = findOrphanTerminals(terminalsStore.getIds(), branchTerminalMap);

      for (const id of orphanTerminals) {
        await deps.closeTerminal(id, true);
      }

      repositoriesStore.add({
        path: info.path,
        displayName: info.name,
        initials: info.initials,
        showAllBranches: settingsStore.state.showAllBranches,
        isGitRepo: info.is_git_repo,
      });

      if (info.branch) {
        repositoriesStore.setBranch(info.path, info.branch, { worktreePath: info.path });
        repositoriesStore.setActiveBranch(info.path, info.branch);
        await handleAddTerminalToBranch(info.path, info.branch);
      } else if (!info.is_git_repo) {
        // Non-git directory: create a shell entry so the user can open terminals
        const shellBranch = "shell";
        repositoriesStore.setBranch(info.path, shellBranch, {
          worktreePath: info.path,
          isMain: true,
          isShell: true,
        });
        repositoriesStore.setActiveBranch(info.path, shellBranch);
        await handleAddTerminalToBranch(info.path, shellBranch);
      }

      repositoriesStore.setActive(info.path);
      setCurrentRepoPath(info.path);
      setCurrentBranch(info.branch || (!info.is_git_repo ? "shell" : ""));
      setRepoStatus(info.status === "not-git" ? "unknown" : info.status);

      refreshAllBranchStats();
    } catch (err) {
      appLogger.error("git", "Failed to add repository", err);
      deps.setStatusInfo(`Failed to add repo: ${err}`);
    }
  };

  const handleAddWorktree = async (repoPath: string) => {
    // Prevent concurrent creations for the same repo
    if (creatingWorktreeRepos().has(repoPath)) return;

    const repoState = repositoriesStore.get(repoPath);
    const worktreeBranches = repoState ? Object.keys(repoState.branches) : [];

    // Fetch data for the dialog in parallel
    const [suggestedName, localBranches, worktreesDir, baseRefs] = await Promise.all([
      deps.repo.generateWorktreeName(worktreeBranches),
      deps.repo.listLocalBranches(repoPath),
      deps.pty.getWorktreesDir(),
      deps.repo.listBaseRefOptions(repoPath),
    ]);

    setWorktreeDialogState({
      repoPath,
      suggestedName,
      existingBranches: localBranches,
      worktreeBranches,
      worktreesDir,
      baseRefs,
    });
  };

  const confirmCreateWorktree = async (options: WorktreeCreateOptions) => {
    const dialogState = worktreeDialogState();
    if (!dialogState) return;

    const { repoPath } = dialogState;
    setWorktreeDialogState(null);

    if (creatingWorktreeRepos().has(repoPath)) return;
    setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

    try {
      deps.setStatusInfo(`Creating worktree ${options.branchName}...`);
      const result = await deps.repo.createWorktree(repoPath, options.branchName, options.createBranch, options.baseRef);

      repositoriesStore.setBranch(repoPath, result.branch, { worktreePath: result.path });
      repositoriesStore.setActiveBranch(repoPath, result.branch);

      await handleAddTerminalToBranch(repoPath, result.branch);

      try {
        const stats = await deps.repo.getDiffStats(result.path);
        repositoriesStore.updateBranchStats(repoPath, result.branch, stats.additions, stats.deletions);
      } catch { /* ignore */ }

      deps.setStatusInfo(`Created worktree ${options.branchName}`);
    } catch (err) {
      appLogger.error("git", "Failed to create worktree", err);
      deps.setStatusInfo(`Failed to create worktree: ${err}`);
    } finally {
      setCreatingWorktreeRepos((prev) => {
        const next = new Set(prev);
        next.delete(repoPath);
        return next;
      });
    }
  };

  /** Quick-clone flow: right-click branch → instant worktree with hybrid name */
  const handleCreateWorktreeFromBranch = async (repoPath: string, branchName: string) => {
    if (creatingWorktreeRepos().has(repoPath)) return;
    setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

    try {
      const repoState = repositoriesStore.get(repoPath);
      const existingBranches = repoState ? Object.keys(repoState.branches) : [];
      const cloneName = await deps.repo.generateCloneBranchName(branchName, existingBranches);

      deps.setStatusInfo(`Creating worktree ${cloneName}...`);
      const result = await deps.repo.createWorktree(repoPath, cloneName, true, branchName);

      repositoriesStore.setBranch(repoPath, result.branch, { worktreePath: result.path });
      repositoriesStore.setActiveBranch(repoPath, result.branch);

      await handleAddTerminalToBranch(repoPath, result.branch);

      try {
        const stats = await deps.repo.getDiffStats(result.path);
        repositoriesStore.updateBranchStats(repoPath, result.branch, stats.additions, stats.deletions);
      } catch { /* ignore */ }

      deps.setStatusInfo(`Created worktree ${cloneName}`);
    } catch (err) {
      appLogger.error("git", "Failed to create worktree from branch", err);
      deps.setStatusInfo(`Failed to create worktree: ${err}`);
    } finally {
      setCreatingWorktreeRepos((prev) => {
        const next = new Set(prev);
        next.delete(repoPath);
        return next;
      });
    }
  };

  const handleNewTab = async () => {
    const activeRepo = repositoriesStore.getActive();
    if (activeRepo && activeRepo.activeBranch) {
      await handleAddTerminalToBranch(activeRepo.path, activeRepo.activeBranch);
    } else {
      await deps.createNewTerminal();
    }
  };

  const handleRunCommand = (forceDialog: boolean, openDialog: () => void) => {
    const savedCmd = activeRunCommand();
    if (savedCmd && !forceDialog) {
      executeRunCommand(savedCmd);
    } else {
      openDialog();
    }
  };

  const executeRunCommand = async (command: string) => {
    const activeRepo = repositoriesStore.getActive();
    if (!activeRepo?.activeBranch) return;

    repositoriesStore.setRunCommand(activeRepo.path, activeRepo.activeBranch, command);

    const canSpawn = await deps.pty.canSpawn();
    if (!canSpawn) {
      deps.setStatusInfo("Max sessions reached (50)");
      return;
    }

    const branch = activeRepo.branches[activeRepo.activeBranch];
    const cwd = branch?.worktreePath || activeRepo.path;
    const maxNameLen = deps.getMaxTabNameLength();
    const tabName = command.length > maxNameLen ? command.slice(0, maxNameLen) + "..." : command;

    const id = terminalsStore.add({
      sessionId: null,
      fontSize: deps.getDefaultFontSize(),
      name: tabName,
      cwd,
      awaitingInput: null,
    });

    terminalsStore.setActive(id);
    repositoriesStore.addTerminalToBranch(activeRepo.path, activeRepo.activeBranch, id);

    const waitForSession = setInterval(async () => {
      const terminal = terminalsStore.get(id);
      if (terminal?.sessionId) {
        clearInterval(waitForSession);
        try {
          await deps.pty.write(terminal.sessionId, command + "\n");
        } catch (err) {
          appLogger.error("terminal", "Failed to send run command", err);
        }
      }
    }, 100);

    setTimeout(() => clearInterval(waitForSession), 10000);
  };

  const generateWorktreeName = async (): Promise<string> => {
    const state = worktreeDialogState();
    const worktreeBranches = state?.worktreeBranches ?? [];
    return deps.repo.generateWorktreeName(worktreeBranches);
  };

  const handleRepoSettings = (
    repoPath: string,
    openSettingsPanel: (context: { kind: "repo"; repoPath: string }) => void,
  ) => {
    setCurrentRepoPath(repoPath);
    openSettingsPanel({ kind: "repo", repoPath });
  };

  return {
    currentRepoPath,
    setCurrentRepoPath,
    currentBranch,
    setCurrentBranch,
    repoStatus,
    setRepoStatus,
    branchToRename,
    setBranchToRename,
    refreshAllBranchStats,
    handleBranchSelect,
    handleAddTerminalToBranch,
    handleRemoveRepo,
    handleRemoveBranch,
    handleOpenRenameBranchDialog,
    handleRenameBranch,
    activeWorktreePath,
    activeRunCommand,
    handleAddRepo,
    handleAddWorktree,
    confirmCreateWorktree,
    handleCreateWorktreeFromBranch,
    worktreeDialogState,
    setWorktreeDialogState,
    creatingWorktreeRepos,
    handleNewTab,
    handleRunCommand,
    executeRunCommand,
    generateWorktreeName,
    handleRepoSettings,
  };
}
