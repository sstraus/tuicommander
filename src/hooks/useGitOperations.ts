import { createSignal } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { open } from "@tauri-apps/plugin-dialog";
import { findOrphanTerminals } from "../utils/terminalOrphans";
import { filterValidTerminals } from "../utils/terminalFilter";

/** Dependencies injected into useGitOperations */
export interface GitOperationsDeps {
  repo: {
    getInfo: (path: string) => Promise<{ path: string; name: string; initials: string; branch: string; status: string }>;
    getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
    getWorktreePaths: (repoPath: string) => Promise<Record<string, string>>;
    removeWorktree: (repoPath: string, branchName: string) => Promise<void>;
    createWorktree: (baseRepo: string, branchName: string) => Promise<{ name: string; path: string; branch: string; base_repo: string }>;
    renameBranch: (repoPath: string, oldName: string, newName: string) => Promise<void>;
    generateWorktreeName: (existingNames: string[]) => Promise<string>;
  };
  pty: {
    canSpawn: () => Promise<boolean>;
    write: (sessionId: string, data: string) => Promise<void>;
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

  const refreshAllBranchStats = async () => {
    await Promise.all(repositoriesStore.getPaths().map(async (repoPath) => {
      if (!repositoriesStore.get(repoPath)) return;

      const worktreePaths = await deps.repo.getWorktreePaths(repoPath);

      const currentRepo = repositoriesStore.get(repoPath);
      if (currentRepo) {
        for (const branchName of Object.keys(currentRepo.branches)) {
          if (!(branchName in worktreePaths)) {
            repositoriesStore.removeBranch(repoPath, branchName);
          }
        }
      }

      for (const [branchName, wtPath] of Object.entries(worktreePaths)) {
        repositoriesStore.setBranch(repoPath, branchName, { worktreePath: wtPath });
      }

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

    if (validTerminals.length > 0) {
      terminalsStore.setActive(validTerminals[0]);
    } else if (!branch?.hadTerminals) {
      // First time selecting this branch — auto-spawn a terminal
      await handleAddTerminalToBranch(repoPath, branchName);
    }
    // If hadTerminals && no valid terminals → user closed them all, show empty state

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

    await deps.repo.renameBranch(branch.repoPath, oldName, newName);
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
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Repository Folder",
    });

    if (!selected) return;

    const path = typeof selected === "string" ? selected : selected[0];
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

      repositoriesStore.add({ path: info.path, displayName: info.name, initials: info.initials });

      if (info.branch) {
        repositoriesStore.setBranch(info.path, info.branch, { worktreePath: info.path });
        repositoriesStore.setActiveBranch(info.path, info.branch);
        await handleAddTerminalToBranch(info.path, info.branch);
      }

      repositoriesStore.setActive(info.path);
      setCurrentRepoPath(info.path);
      setCurrentBranch(info.branch);
      setRepoStatus(info.status as "clean" | "dirty" | "conflict" | "merge" | "unknown");

      refreshAllBranchStats();
    } catch (err) {
      console.error("Failed to add repository:", err);
      deps.setStatusInfo(`Failed to add repo: ${err}`);
    }
  };

  const handleAddWorktree = async (repoPath: string) => {
    // Prevent concurrent creations for the same repo
    if (creatingWorktreeRepos().has(repoPath)) return;

    setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

    const repoState = repositoriesStore.get(repoPath);
    const existingBranches = repoState ? Object.keys(repoState.branches) : [];

    const branchName = await deps.repo.generateWorktreeName(existingBranches);

    try {
      deps.setStatusInfo(`Creating worktree ${branchName}...`);
      const result = await deps.repo.createWorktree(repoPath, branchName);

      repositoriesStore.setBranch(repoPath, result.branch, { worktreePath: result.path });
      repositoriesStore.setActiveBranch(repoPath, result.branch);

      await handleAddTerminalToBranch(repoPath, result.branch);

      try {
        const stats = await deps.repo.getDiffStats(result.path);
        repositoriesStore.updateBranchStats(repoPath, result.branch, stats.additions, stats.deletions);
      } catch { /* ignore */ }

      deps.setStatusInfo(`Created worktree ${branchName}`);
    } catch (err) {
      console.error("Failed to create worktree:", err);
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
          console.error("Failed to send run command:", err);
        }
      }
    }, 100);

    setTimeout(() => clearInterval(waitForSession), 10000);
  };

  const handleRepoSettings = (
    repoPath: string,
    openSettingsPanel: (context: { kind: "repo"; repoPath: string; displayName: string }) => void,
  ) => {
    setCurrentRepoPath(repoPath);
    const repoState = repositoriesStore.get(repoPath);
    const displayName = repoState?.displayName ?? repoPath.split("/").pop() ?? repoPath;
    openSettingsPanel({ kind: "repo", repoPath, displayName });
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
    creatingWorktreeRepos,
    handleNewTab,
    handleRunCommand,
    executeRunCommand,
    handleRepoSettings,
  };
}
