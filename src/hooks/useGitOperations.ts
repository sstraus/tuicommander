import { createSignal, batch } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore, type RepositoryState } from "../stores/repositories";
import { appLogger } from "../stores/appLogger";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauri } from "../transport";
import { invoke } from "../invoke";
import { findOrphanTerminals } from "../utils/terminalOrphans";
import { filterValidTerminals } from "../utils/terminalFilter";
import { verifyAndBuildResumeCommand } from "../utils/agentSession";
import { repoSettingsStore } from "../stores/repoSettings";
import { githubStore } from "../stores/github";
import { paneLayoutStore, type PaneLayoutState } from "../stores/paneLayout";

/** In-memory pane layout cache per branch — survives branch switches within a session */
const savedPaneLayouts = new Map<string, PaneLayoutState>();

function paneLayoutKey(repoPath: string, branchName: string): string {
  return `${repoPath}\0${branchName}`;
}
import { effectiveMergeMethod, isMergeMethodNotAllowed } from "../utils/prMerge";
import type { WorktreeCreateOptions } from "../components/CreateWorktreeDialog";

/** Dependencies injected into useGitOperations */
export interface GitOperationsDeps {
  repo: {
    getInfo: (path: string) => Promise<{ path: string; name: string; initials: string; branch: string; status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown"; is_git_repo: boolean }>;
    getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
    getWorktreePaths: (repoPath: string) => Promise<Record<string, string>>;
    getRepoSummary: (repoPath: string) => Promise<{
      worktree_paths: Record<string, string>;
      merged_branches: string[];
      diff_stats: Record<string, { additions: number; deletions: number }>;
      last_commit_ts: Record<string, number | null>;
    }>;
    getRepoStructure: (repoPath: string) => Promise<{
      worktree_paths: Record<string, string>;
      merged_branches: string[];
    }>;
    getRepoDiffStats: (repoPath: string) => Promise<{
      diff_stats: Record<string, { additions: number; deletions: number }>;
      last_commit_ts: Record<string, number | null>;
    }>;
    removeWorktree: (repoPath: string, branchName: string, deleteBranch: boolean) => Promise<void>;
    createWorktree: (baseRepo: string, branchName: string, createBranch?: boolean, baseRef?: string) => Promise<{ name: string; path: string; branch: string; base_repo: string }>;
    renameBranch: (repoPath: string, oldName: string, newName: string) => Promise<void>;
    generateWorktreeName: (existingNames: string[]) => Promise<string>;
    generateCloneBranchName: (sourceBranch: string, existingNames: string[]) => Promise<string>;
    listBaseRefOptions: (repoPath: string) => Promise<import("./useRepository").BaseRefOption[]>;
    mergeAndArchiveWorktree: (repoPath: string, branchName: string, targetBranch: string, afterMerge: string) => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
    finalizeMergedWorktree: (repoPath: string, branchName: string, action: "archive" | "delete") => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
    listLocalBranches: (repoPath: string) => Promise<string[]>;
    getMergedBranches: (repoPath: string) => Promise<string[]>;
    checkoutRemoteBranch: (repoPath: string, branchName: string) => Promise<void>;
    detectOrphanWorktrees: (repoPath: string) => Promise<string[]>;
    removeOrphanWorktree: (repoPath: string, worktreePath: string) => Promise<void>;
    mergePrViaGithub: (repoPath: string, prNumber: number, mergeMethod: string) => Promise<string>;
    switchBranch: (repoPath: string, branchName: string, opts?: { force?: boolean; stash?: boolean }) => Promise<{ success: boolean; stashed: boolean; previous_branch: string; new_branch: string }>;
    runSetupScript: (script: string, cwd: string) => Promise<{ exit_code: number; stdout: string; stderr: string }>;
  };
  pty: {
    canSpawn: () => Promise<boolean>;
    write: (sessionId: string, data: string) => Promise<void>;
    getWorktreesDir: (repoPath?: string) => Promise<string>;
  };
  dialogs: {
    confirmRemoveRepo: (repoName: string) => Promise<boolean>;
    confirmRemoveWorktree: (branchName: string) => Promise<boolean>;
    confirmStashAndSwitch?: (branchName: string) => Promise<boolean>;
    confirmOrphanCleanup?: (paths: string[]) => Promise<boolean>;
    /** Browser mode only: show an in-app text-input dialog to enter a repo path */
    promptRepoPath?: () => Promise<string | null>;
  };
  closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
  createNewTerminal: () => Promise<string | undefined>;
  setStatusInfo: (msg: string) => void;
  getDefaultFontSize: () => number;
  getMaxTabNameLength: () => number;
  /** Returns the effective promptOnCreate setting for the given repo.
   *  When false, handleAddWorktree skips the dialog and creates instantly.
   *  Defaults to true (show dialog) when not provided. */
  getPromptOnCreate?: (repoPath: string) => boolean;
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
    baseRefs: import("./useRepository").BaseRefOption[];
  } | null>(null);

  /** Pending merge context — set when afterMerge=ask; cleared once the user picks or skips cleanup */
  const [mergePendingCtx, setMergePendingCtx] = createSignal<{
    repoPath: string;
    branchName: string;
    baseBranch: string;
    hasDirtyFiles: boolean;
  } | null>(null);

  /** Reentrancy guard: prevents concurrent handleBranchSelect calls from
   *  duplicating terminals (e.g. rapid sidebar clicks, quick-switcher). */
  let branchSelectInFlight: Promise<void> | null = null;

  /** Transition a repo from git to shell mode (e.g. .git was removed) */
  const transitionToShell = (repoPath: string, currentRepo: RepositoryState) => {
    batch(() => {
      // Migrate all terminals to a shell branch
      const allTerminals: string[] = [];
      for (const branch of Object.values(currentRepo.branches)) {
        allTerminals.push(...branch.terminals);
        repositoriesStore.removeBranch(repoPath, branch.name);
      }
      repositoriesStore.setIsGitRepo(repoPath, false);
      const shellBranch = "shell";
      repositoriesStore.setBranch(repoPath, shellBranch, {
        worktreePath: repoPath,
        isMain: true,
        isShell: true,
      });
      for (const termId of allTerminals) {
        repositoriesStore.addTerminalToBranch(repoPath, shellBranch, termId);
      }
      repositoriesStore.setActiveBranch(repoPath, shellBranch);
    });
  };

  // Per-repo refresh generation counter — discards stale Phase 2 writes
  // when a newer refresh has started for the same repo.
  const refreshGeneration = new Map<string, number>();

  const refreshAllBranchStats = async () => {
    await Promise.all(repositoriesStore.getPaths().map(async (repoPath) => {
      const gen = (refreshGeneration.get(repoPath) ?? 0) + 1;
      refreshGeneration.set(repoPath, gen);

      const repo = repositoriesStore.get(repoPath);
      if (!repo) return;
      // Non-git directories: check if they became a git repo
      if (repo.isGitRepo === false) {
        try {
          const info = await deps.repo.getInfo(repoPath);
          if (info.is_git_repo && info.branch) {
            // Directory gained .git — transition to git mode
            batch(() => {
              for (const branch of Object.values(repo.branches)) {
                repositoriesStore.removeBranch(repoPath, branch.name);
              }
              repositoriesStore.setIsGitRepo(repoPath, true);
              repositoriesStore.setBranch(repoPath, info.branch, { worktreePath: repoPath });
              repositoriesStore.setActiveBranch(repoPath, info.branch);
            });
          }
        } catch {
          // Probe failed — stay in shell mode
        }
        return;
      }

      // === PHASE 1: Structure (fast) ===
      // Returns worktree_paths + merged_branches only — no expensive diff stats.
      const structure = await deps.repo.getRepoStructure(repoPath);
      if (refreshGeneration.get(repoPath) !== gen) return; // stale

      const worktreePaths = structure.worktree_paths;
      const mergedSet = new Set(structure.merged_branches);

      const currentRepo = repositoriesStore.get(repoPath);
      if (!currentRepo) return;

      if (Object.keys(worktreePaths).length === 0) {
        // Worktrees came back empty — either a transient backend error or the
        // repo is no longer a git repo. Probe to find out.
        try {
          const info = await deps.repo.getInfo(repoPath);
          if (!info.is_git_repo) {
            transitionToShell(repoPath, currentRepo);
            return;
          }
        } catch {
          // getInfo failed — don't destroy UI state
        }
        // Still a git repo but no worktrees returned — skip to avoid
        // destroying existing branch state on a transient error.
        if (Object.keys(currentRepo.branches).length > 0) return;
      }

      // Compute the target set of branches to keep, then apply all
      // mutations in a single batch to prevent intermediate renders
      // (which caused the sidebar to flash/jump during refresh).
      const storeIds = new Set(terminalsStore.getIds());
      const toRemove: string[] = [];
      const terminalsToClose: string[] = [];

      // If activeBranch is no longer a worktree, find its replacement:
      // the worktree branch that occupies the same path (HEAD moved).
      let activeBranchReplacement: string | null = null;
      const active = currentRepo.activeBranch;
      if (active && !(active in worktreePaths)) {
        const activePath = currentRepo.branches[active]?.worktreePath;
        if (activePath) {
          for (const [wtBranch, wtPath] of Object.entries(worktreePaths)) {
            if (wtPath === activePath) {
              activeBranchReplacement = wtBranch;
              break;
            }
          }
        }
      }

      for (const branchName of Object.keys(currentRepo.branches)) {
        if (!(branchName in worktreePaths)) {
          // If this is the stale activeBranch and we found a replacement, allow removal
          if (branchName === active && activeBranchReplacement) {
            appLogger.info("terminal", `refreshAllBranchStats: activeBranch "${branchName}" replaced by "${activeBranchReplacement}"`);
            toRemove.push(branchName);
            continue;
          }
          // Branch has live terminals — only keep it if the worktree path
          // is the main repo checkout (HEAD switched away). If the worktree
          // directory was deleted externally, close the orphaned terminals
          // so the stale branch can be cleaned up.
          const branchState = currentRepo.branches[branchName];
          const hasLiveTerminals = branchState?.terminals.some(id => storeIds.has(id));
          if (hasLiveTerminals) {
            const isLinkedWorktree = branchState.worktreePath && branchState.worktreePath !== repoPath;
            if (isLinkedWorktree) {
              // Linked worktree was removed externally — close its terminals
              appLogger.info("terminal", `refreshAllBranchStats: closing terminals for deleted worktree "${branchName}"`, {
                terminals: branchState.terminals,
                worktreePath: branchState.worktreePath,
              });
              terminalsToClose.push(...branchState.terminals.filter(id => storeIds.has(id)));
              toRemove.push(branchName);
            } else {
              appLogger.info("terminal", `refreshAllBranchStats: keeping "${branchName}" — has live terminals`, {
                terminals: branchState.terminals,
              });
            }
            continue;
          }
          toRemove.push(branchName);
        }
      }

      if (toRemove.length > 0) {
        appLogger.info("terminal", `refreshAllBranchStats removing branches from ${repoPath}`, {
          toRemove,
          worktreePathKeys: Object.keys(worktreePaths),
          existingBranches: Object.keys(currentRepo.branches),
        });
      }

      // Close terminals for deleted worktrees before mutating store state.
      // Best-effort: a PTY may already be dead; log and continue so the
      // branch removal in the batch below is not blocked.
      for (const termId of terminalsToClose) {
        try {
          await deps.closeTerminal(termId, true);
        } catch (err) {
          appLogger.warn("terminal", `refreshAllBranchStats: failed to close terminal ${termId}`, err);
        }
      }

      batch(() => {
        // Create new worktree branches first so mergeBranchState has a target
        for (const [branchName, wtPath] of Object.entries(worktreePaths)) {
          repositoriesStore.setBranch(repoPath, branchName, { worktreePath: wtPath, isMerged: mergedSet.has(branchName) });
        }
        // Migrate terminal state from stale activeBranch to its replacement
        if (active && activeBranchReplacement && toRemove.includes(active)) {
          repositoriesStore.mergeBranchState(repoPath, active, activeBranchReplacement);
          repositoriesStore.setActiveBranch(repoPath, activeBranchReplacement);
        }
        for (const branchName of toRemove) {
          repositoriesStore.removeBranch(repoPath, branchName);
        }
      });

      const updatedRepo = repositoriesStore.get(repoPath);
      if (!updatedRepo) return;

      // Side effects that only need structure data — run before Phase 2
      await handleAutoArchiveMerged(repoPath, updatedRepo.branches);
      await handleOrphanCleanup(repoPath);

      // === PHASE 2: Stats (slow) ===
      // Per-worktree diff stats + last-commit timestamps.
      // Non-fatal: if this fails, UI shows rows from Phase 1 with stale/zero stats.
      if (refreshGeneration.get(repoPath) !== gen) return; // stale check before Phase 2

      try {
        const stats = await deps.repo.getRepoDiffStats(repoPath);
        if (refreshGeneration.get(repoPath) !== gen) return; // stale after await

        const currentRepoForStats = repositoriesStore.get(repoPath);
        if (!currentRepoForStats) return;

        batch(() => {
          for (const branch of Object.values(currentRepoForStats.branches)) {
            if (!branch.worktreePath) continue;
            const ds = stats.diff_stats[branch.worktreePath];
            if (ds) {
              repositoriesStore.updateBranchStats(repoPath, branch.name, ds.additions, ds.deletions);
            }
            const ts = stats.last_commit_ts?.[branch.name];
            if (ts !== undefined) {
              // Rust emits Unix seconds (%ct); JS Date.now() uses milliseconds
              repositoriesStore.setBranch(repoPath, branch.name, { lastCommitTs: ts !== null ? ts * 1000 : null });
            }
          }
        });
      } catch (err) {
        appLogger.warn("git", `Phase 2 diff stats failed for ${repoPath}`, err);
      }
    }));
  };

  /** Detect orphaned linked worktrees and act based on the orphanCleanup setting. */
  let orphanDialogOpen = false;
  const handleOrphanCleanup = async (repoPath: string) => {
    const orphanCleanup = repoSettingsStore.getEffective(repoPath)?.orphanCleanup ?? "ask";
    if (orphanCleanup === "off") return;

    let orphanPaths: string[];
    try {
      orphanPaths = await deps.repo.detectOrphanWorktrees(repoPath);
    } catch {
      return; // Detection failure is non-fatal
    }
    if (orphanPaths.length === 0) return;

    if (orphanCleanup === "on") {
      // Auto-remove silently
      for (const wtPath of orphanPaths) {
        try {
          await deps.repo.removeOrphanWorktree(repoPath, wtPath);
        } catch (err) {
          appLogger.warn("git", `Failed to auto-remove orphan worktree ${wtPath}`, err);
        }
      }
      deps.setStatusInfo(`Removed ${orphanPaths.length} orphaned worktree(s)`);
      return;
    }

    // orphanCleanup === "ask"
    if (orphanDialogOpen) return; // Prevent duplicate dialogs from concurrent refreshes
    orphanDialogOpen = true;
    let confirmed: boolean;
    try {
      confirmed = (await deps.dialogs.confirmOrphanCleanup?.(orphanPaths)) ?? false;
    } finally {
      orphanDialogOpen = false;
    }
    if (!confirmed) return;

    for (const wtPath of orphanPaths) {
      try {
        await deps.repo.removeOrphanWorktree(repoPath, wtPath);
      } catch (err) {
        appLogger.warn("git", `Failed to remove orphan worktree ${wtPath}`, err);
      }
    }
    deps.setStatusInfo(`Removed ${orphanPaths.length} orphaned worktree(s)`);
  };

  /** Archive all merged linked worktrees when the autoArchiveMerged setting is enabled. */
  const handleAutoArchiveMerged = async (repoPath: string, branches: RepositoryState["branches"]) => {
    if (!repoSettingsStore.getEffective(repoPath)?.autoArchiveMerged) return;

    const mergedLinkedBranches = Object.values(branches).filter(
      (b) => b.isMerged && b.worktreePath !== null && b.worktreePath !== repoPath,
    );
    if (mergedLinkedBranches.length === 0) return;

    let archived = 0;
    for (const branch of mergedLinkedBranches) {
      try {
        await deps.repo.finalizeMergedWorktree(repoPath, branch.name, "archive");
        archived++;
      } catch (err) {
        appLogger.warn("git", `Failed to auto-archive merged worktree for "${branch.name}"`, err);
      }
    }
    if (archived > 0) {
      deps.setStatusInfo(`Auto-archived ${archived} merged worktree(s)`);
    }
  };

  const handleAddTerminalToBranch = async (repoPath: string, branchName: string) => {
    const canSpawn = await deps.pty.canSpawn();
    if (!canSpawn) {
      deps.setStatusInfo("Max sessions reached (50)");
      return;
    }

    // Ensure this repo+branch is active without going through handleBranchSelect
    // (which has auto-spawn logic that would create a duplicate terminal).
    const activeRepo = repositoriesStore.getActive();
    if (activeRepo?.path !== repoPath || activeRepo?.activeBranch !== branchName) {
      repositoriesStore.setActive(repoPath);
      repositoriesStore.setActiveBranch(repoPath, branchName);
      setCurrentRepoPath(repoPath);
      setCurrentBranch(branchName);
    }

    const branch = repositoriesStore.get(repoPath)?.branches[branchName];
    const termCount = branch?.terminals.length || 0;

    const id = terminalsStore.add({
      sessionId: null,
      fontSize: deps.getDefaultFontSize(),
      name: `${branchName.split("/").pop()} ${termCount + 1}`,
      cwd: branch?.worktreePath || null,
      awaitingInput: null,
      tuicSession: crypto.randomUUID(),
    });

    repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
    terminalsStore.setActive(id);
    // Focus the new terminal after SolidJS renders and mounts the component
    // (onMount sets ref, which happens in the next frame).
    requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
    return id;
  };

  const handleBranchSelect = async (repoPath: string, branchName: string) => {
    // Serialize: wait for any in-flight branch select to finish before starting ours.
    // Without this, rapid sidebar clicks or quick-switcher can run two selects
    // concurrently, causing duplicate terminal creation from savedTerminals.
    if (branchSelectInFlight) {
      await branchSelectInFlight;
    }
    let resolve!: () => void;
    branchSelectInFlight = new Promise<void>((r) => { resolve = r; });
    try {
      await handleBranchSelectInner(repoPath, branchName);
    } finally {
      branchSelectInFlight = null;
      resolve();
    }
  };

  const handleBranchSelectInner = async (repoPath: string, branchName: string) => {
    // Log the state we're LEAVING — critical for diagnosing terminal disappearance
    const prevRepo = repositoriesStore.getActive();
    const prevBranchName = prevRepo?.activeBranch;
    const prevBranch = prevBranchName ? prevRepo?.branches[prevBranchName] : null;
    appLogger.info("terminal", `BranchSelect LEAVING ${prevRepo?.path ?? "(none)"}/${prevBranchName ?? "(none)"}`, {
      prevTerminals: prevBranch?.terminals ?? [],
      prevHadTerminals: prevBranch?.hadTerminals,
      activeTerminalId: terminalsStore.state.activeId,
      allStoreIds: terminalsStore.getIds(),
    });

    // Save state for the branch we're leaving
    if (prevRepo?.activeBranch) {
      const currentActiveId = terminalsStore.state.activeId;
      if (currentActiveId && prevBranch?.terminals.includes(currentActiveId)) {
        repositoriesStore.setBranch(prevRepo.path, prevRepo.activeBranch, { lastActiveTerminal: currentActiveId });
        appLogger.info("terminal", `BranchSelect SAVE lastActiveTerminal=${currentActiveId} for ${prevRepo.activeBranch}`);
      } else {
        appLogger.info("terminal", `BranchSelect SKIP save lastActiveTerminal — activeId=${currentActiveId} not in branch terminals ${JSON.stringify(prevBranch?.terminals)}`);
      }
      // Save pane layout for the branch we're leaving
      if (paneLayoutStore.isSplit()) {
        const key = paneLayoutKey(prevRepo.path, prevRepo.activeBranch);
        savedPaneLayouts.set(key, paneLayoutStore.serialize());
        appLogger.info("terminal", `BranchSelect SAVE paneLayout for ${prevRepo.activeBranch}`);
      } else {
        // Clear any stale layout if user unsplit while on this branch
        savedPaneLayouts.delete(paneLayoutKey(prevRepo.path, prevRepo.activeBranch));
      }
    }

    // Batch all reactive updates so downstream effects (file browser, etc.)
    // see a consistent snapshot — prevents stale intermediate states where
    // repoPath updated but fsRoot still points to the old worktree.
    batch(() => {
      setCurrentRepoPath(repoPath);
      repositoriesStore.setActive(repoPath);
      repositoriesStore.setActiveBranch(repoPath, branchName);
      setCurrentBranch(branchName);
    });

    // Fire-and-forget: diff stats are cosmetic, don't block branch switch
    const selectedBranch = repositoriesStore.get(repoPath)?.branches[branchName];
    if (selectedBranch?.worktreePath) {
      const wtPath = selectedBranch.worktreePath;
      deps.repo.getDiffStats(wtPath).then((stats) => {
        repositoriesStore.updateBranchStats(repoPath, branchName, stats.additions, stats.deletions);
      }).catch((err) => appLogger.debug("git", `getDiffStats failed for ${branchName}`, err));
    }
    let branch = repositoriesStore.get(repoPath)?.branches[branchName];

    // Adopt orphaned terminals whose cwd matches this branch's worktree path.
    // Pre-compute claimed set O(B×T) once, then check in O(1) per terminal.
    if (branch?.worktreePath) {
      const branchTermSet = new Set(branch.terminals);
      const claimedIds = new Set<string>();
      for (const b of Object.values(repositoriesStore.get(repoPath)?.branches ?? {})) {
        if (b.name !== branchName) {
          for (const tid of b.terminals) claimedIds.add(tid);
        }
      }
      for (const id of terminalsStore.getIds()) {
        if (branchTermSet.has(id)) continue;
        if (claimedIds.has(id)) continue;
        const term = terminalsStore.get(id);
        if (term?.cwd === branch.worktreePath) {
          appLogger.info("terminal", `BranchSelect: adopting orphan ${id} into ${branchName} (cwd matches worktreePath)`);
          repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
        }
      }
      // Re-read branch state after potential adoptions
      branch = repositoriesStore.get(repoPath)?.branches[branchName];
    }
    const validTerminals = filterValidTerminals(branch?.terminals, terminalsStore.getIds());
    appLogger.info("terminal", `BranchSelect → ${branchName}`, { branchTerminals: branch?.terminals, storeIds: terminalsStore.getIds(), valid: validTerminals, hadTerminals: branch?.hadTerminals, savedTerminals: branch?.savedTerminals?.length ?? 0 });
    if (validTerminals.length === 0 && (branch?.terminals?.length ?? 0) > 0) {
      appLogger.warn("terminal", `BranchSelect MISMATCH: branch has terminals ${JSON.stringify(branch?.terminals)} but none found in store ${JSON.stringify(terminalsStore.getIds())}. Will create fresh terminal.`);
    }

    if (validTerminals.length > 0) {
      // Restore saved pane layout if available and all its terminals are still valid
      const layoutKey = paneLayoutKey(repoPath, branchName);
      const savedLayout = savedPaneLayouts.get(layoutKey);
      if (savedLayout) {
        const validSet = new Set(validTerminals);
        const layoutTerminals = Object.values(savedLayout.groups).flatMap(g => g.tabs.filter(t => t.type === "terminal").map(t => t.id));
        const allValid = layoutTerminals.length > 0 && layoutTerminals.every(id => validSet.has(id));
        if (allValid) {
          paneLayoutStore.restore(savedLayout);
          appLogger.info("terminal", `BranchSelect RESTORE paneLayout for ${branchName}`);
        } else {
          appLogger.info("terminal", `BranchSelect DISCARD stale paneLayout for ${branchName} — terminal IDs changed`);
          savedPaneLayouts.delete(layoutKey);
          paneLayoutStore.reset();
        }
      } else {
        paneLayoutStore.reset();
      }
      // Restore the last active terminal for this branch, or fall back to first
      const remembered = branch?.lastActiveTerminal;
      if (remembered && validTerminals.includes(remembered)) {
        appLogger.info("terminal", `BranchSelect RESTORE lastActiveTerminal=${remembered} for ${branchName}`);
        terminalsStore.setActive(remembered);
      } else {
        appLogger.info("terminal", `BranchSelect FALLBACK to first terminal=${validTerminals[0]} for ${branchName} (remembered=${remembered}, valid=${JSON.stringify(validTerminals)})`);
        terminalsStore.setActive(validTerminals[0]);
      }
    } else if (branch?.savedTerminals && branch.savedTerminals.length > 0) {
      // Lazy restore: create terminals from persisted session state
      // First pass: create all terminals synchronously (instant UI)
      const restoredIds: { id: string; terminal: (typeof branch.savedTerminals)[number] }[] = [];
      for (const terminal of branch.savedTerminals) {
        const id = terminalsStore.add({
          sessionId: null,
          fontSize: terminal.fontSize,
          name: terminal.name,
          cwd: terminal.cwd,
          awaitingInput: null,
          tuicSession: terminal.tuicSession ?? crypto.randomUUID(),
        });
        repositoriesStore.addTerminalToBranch(repoPath, branchName, id);
        restoredIds.push({ id, terminal });
      }
      // Clear savedTerminals for this branch (consume-once)
      repositoriesStore.setBranch(repoPath, branchName, { savedTerminals: [] });
      if (restoredIds.length > 0) terminalsStore.setActive(restoredIds[0].id);

      paneLayoutStore.reset();

      // Second pass: verify resume commands in parallel (non-blocking)
      const agentTerminals = restoredIds.filter((r) => r.terminal.agentType);
      if (agentTerminals.length > 0) {
        Promise.all(agentTerminals.map(async ({ id, terminal }) => {
          const resumeCmd = await verifyAndBuildResumeCommand(
            terminal.agentType!,
            terminal.cwd,
            terminal.tuicSession,
            terminal.agentSessionId,
          );
          if (resumeCmd) {
            terminalsStore.update(id, { pendingResumeCommand: resumeCmd, agentSessionId: terminal.agentSessionId ?? null });
          }
        })).catch((e) => appLogger.warn("terminal", "Resume command verification failed", { error: String(e) }));
      }
    } else if (!branch?.hadTerminals) {
      // First time selecting this branch — auto-spawn a terminal
      paneLayoutStore.reset();
      await handleAddTerminalToBranch(repoPath, branchName);
    } else {
      // hadTerminals && no valid terminals → user closed them all, show empty state.
      // Clear layout and activeId so the previous branch's split doesn't bleed through.
      paneLayoutStore.reset();
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

    const effective = repoSettingsStore.getEffective(repoPath);
    const deleteBranch = effective?.deleteBranchOnRemove ?? true;
    try {
      await deps.repo.removeWorktree(repoPath, branchName, deleteBranch);
      deps.setStatusInfo(`Removed ${branchName}`);
    } catch (err) {
      const reason = err instanceof Error ? err.message : String(err);
      appLogger.error("git", `Failed to remove worktree for ${branchName}: ${reason}`);
      deps.setStatusInfo(`Removed ${branchName} from UI (worktree removal failed)`);
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
        defaultPath: repositoriesStore.getActive()?.path ?? "/",
      });
      if (!selected) return;
      path = typeof selected === "string" ? selected : selected[0];
    } else {
      // Browser mode: no native file picker — use in-app text input dialog
      const input = await deps.dialogs.promptRepoPath?.();
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

      // Start unified repo watcher (covers HEAD, git state, working tree)
      if (info.is_git_repo) {
        invoke("start_repo_watcher", { repoPath: info.path }).catch((err) =>
          appLogger.warn("app", `RepoWatcher failed to start for ${info.path}`, err),
        );
      }

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
      deps.pty.getWorktreesDir(repoPath),
      deps.repo.listBaseRefOptions(repoPath),
    ]);

    const promptOnCreate = deps.getPromptOnCreate?.(repoPath) ?? true;

    if (!promptOnCreate) {
      // Skip dialog: create worktree instantly with auto-generated name
      setWorktreeDialogState({
        repoPath,
        suggestedName,
        existingBranches: localBranches,
        worktreeBranches,
        worktreesDir,
        baseRefs,
      });
      await confirmCreateWorktree({
        branchName: suggestedName,
        createBranch: true,
        baseRef: baseRefs[0]?.name ?? "HEAD",
      });
      return;
    }

    setWorktreeDialogState({
      repoPath,
      suggestedName,
      existingBranches: localBranches,
      worktreeBranches,
      worktreesDir,
      baseRefs,
    });
  };

  /** Shared post-creation setup: run scripts, open terminal, fetch stats */
  const setupNewWorktree = async (
    repoPath: string,
    result: { name: string; path: string; branch: string; base_repo: string },
    displayName: string,
  ) => {
    repositoriesStore.setBranch(repoPath, result.branch, { worktreePath: result.path });
    repositoriesStore.setActiveBranch(repoPath, result.branch);

    const effective = repoSettingsStore.getEffective(repoPath);
    if (effective?.setupScript) {
      try {
        deps.setStatusInfo(`Running setup script in ${displayName}...`);
        const scriptResult = await deps.repo.runSetupScript(effective.setupScript, result.path);
        if (scriptResult.exit_code !== 0) {
          appLogger.warn("git", `Setup script failed (exit ${scriptResult.exit_code})`, scriptResult.stderr);
          deps.setStatusInfo(`Setup script failed (exit ${scriptResult.exit_code})`);
        }
      } catch (err) {
        appLogger.warn("git", "Setup script execution error", err);
        deps.setStatusInfo(`Setup script failed: ${err}`);
      }
    }

    const termId = await handleAddTerminalToBranch(repoPath, result.branch);

    if (termId && effective?.runScript) {
      terminalsStore.update(termId, { pendingInitCommand: effective.runScript });
    }

    try {
      const stats = await deps.repo.getDiffStats(result.path);
      repositoriesStore.updateBranchStats(repoPath, result.branch, stats.additions, stats.deletions);
    } catch (err) { appLogger.debug("git", `getDiffStats failed for ${result.branch}`, err); }

    deps.setStatusInfo(`Created worktree ${displayName}`);
  };

  const confirmCreateWorktree = async (options: WorktreeCreateOptions) => {
    const dialogState = worktreeDialogState();
    if (!dialogState) return;

    const { repoPath } = dialogState;

    if (creatingWorktreeRepos().has(repoPath)) return;
    setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

    try {
      deps.setStatusInfo(`Creating worktree ${options.branchName}...`);
      const result = await deps.repo.createWorktree(repoPath, options.branchName, options.createBranch, options.baseRef);

      // Close dialog only on success
      setWorktreeDialogState(null);
      await setupNewWorktree(repoPath, result, options.branchName);
    } catch (err) {
      appLogger.error("git", "Failed to create worktree", err);
      deps.setStatusInfo(`Failed to create worktree: ${err}`);
      // Re-throw so the dialog can show the error and stay open
      throw err;
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

      await setupNewWorktree(repoPath, result, cloneName);
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

  /** Merge a worktree branch into target, then archive/delete based on setting.
   *  When the branch has an open PR, uses GitHub API with the configured merge strategy.
   *  Falls back to local git merge if no PR is found or GitHub API fails. */
  /** Close all terminals belonging to a branch. */
  const closeTerminalsForBranch = async (repoPath: string, branchName: string) => {
    const repoState = repositoriesStore.get(repoPath);
    const branch = repoState?.branches[branchName];
    if (branch) {
      for (const termId of branch.terminals) {
        await deps.closeTerminal(termId, true);
      }
    }
  };

  const handleMergeAndArchive = async (
    repoPath: string,
    branchName: string,
    targetBranch: string,
    afterMerge: string,
  ) => {
    try {
      deps.setStatusInfo(`Merging ${branchName} into ${targetBranch}...`);

      // Use GitHub API when a PR exists for this branch
      const pr = githubStore.getPrStatus(repoPath, branchName);
      if (pr && pr.state === "OPEN") {
        const preferred = repoSettingsStore.getEffective(repoPath)?.prMergeStrategy ?? "merge";
        const method = effectiveMergeMethod(pr, preferred);
        try {
          await deps.repo.mergePrViaGithub(repoPath, pr.number, method);
        } catch (githubErr) {
          if (isMergeMethodNotAllowed(githubErr)) {
            // Surface 405 to the caller — branch protection rules disallow this merge method
            throw githubErr;
          }
          // Other GitHub API failures — fall back to local git merge
          appLogger.warn("git", `GitHub API merge failed, falling back to local git merge: ${githubErr}`);
          await mergeLocalAndFinalize(repoPath, branchName, targetBranch, afterMerge);
          return;
        }
        // GitHub merge succeeded — close terminals and finalize the worktree locally
        await closeTerminalsForBranch(repoPath, branchName);
        if (afterMerge === "ask") {
          deps.setStatusInfo(`Merged ${branchName} via GitHub — choose what to do with the worktree`);
          let hasDirtyFiles = false;
          try {
            const status = await invoke<{ stdout: string }>("run_git_command", { path: repoPath, args: ["status", "--porcelain"] });
            hasDirtyFiles = status.stdout.trim().length > 0;
          } catch (err) { appLogger.warn("git", `Could not check dirty status for ${repoPath}, assuming clean`, err); }
          setMergePendingCtx({ repoPath, branchName, baseBranch: targetBranch, hasDirtyFiles });
          return;
        }
        const action = afterMerge as "archive" | "delete";
        await deps.repo.finalizeMergedWorktree(repoPath, branchName, action);
        deps.setStatusInfo(`Merged ${branchName} via GitHub (${action === "archive" ? "archived" : "deleted"})`);
        repositoriesStore.removeBranch(repoPath, branchName);
        refreshAllBranchStats();
        return;
      }

      // No open PR — local git merge
      await mergeLocalAndFinalize(repoPath, branchName, targetBranch, afterMerge);
    } catch (err) {
      if (isMergeMethodNotAllowed(err)) {
        throw err;
      }
      appLogger.error("git", "Failed to merge and archive worktree", err);
      deps.setStatusInfo(`Failed to merge: ${err}`);
    }
  };

  /** Local git merge path: checkout target, merge, then archive/delete/ask. */
  const mergeLocalAndFinalize = async (
    repoPath: string,
    branchName: string,
    targetBranch: string,
    afterMerge: string,
  ) => {
    const result = await deps.repo.mergeAndArchiveWorktree(repoPath, branchName, targetBranch, afterMerge);

    // Merge succeeded — close terminals now (not before, to avoid orphaning the branch on failure)
    await closeTerminalsForBranch(repoPath, branchName);

    if (result.action === "pending") {
      // "ask" mode — merge succeeded, user must choose what to do with the worktree
      deps.setStatusInfo(`Merged ${branchName} into ${targetBranch} — choose what to do with the worktree`);
      let hasDirtyFiles = false;
      try {
        const status = await invoke<{ stdout: string }>("run_git_command", { path: repoPath, args: ["status", "--porcelain"] });
        hasDirtyFiles = status.stdout.trim().length > 0;
      } catch (err) { appLogger.warn("git", `Could not check dirty status for ${repoPath}, assuming clean`, err); }
      setMergePendingCtx({ repoPath, branchName, baseBranch: targetBranch, hasDirtyFiles });
      // Branch stays in sidebar until the user decides via cleanup dialog
      return;
    }

    deps.setStatusInfo(`Merged ${branchName} into ${targetBranch} (${result.action})`);

    // Remove branch from sidebar
    repositoriesStore.removeBranch(repoPath, branchName);

    // Refresh to pick up updated branch stats
    refreshAllBranchStats();
  };

  /** Dismiss the post-merge cleanup dialog. */
  const dismissMergePending = () => {
    setMergePendingCtx(null);
  };

  const handleNewTab = async () => {
    // Prefer the active terminal's branch registration and CWD as source of truth —
    // the store's activeBranch may be stale if HEAD changed externally and head-changed
    // hasn't been fully processed yet (race between refreshAllBranchStats and setActiveBranch).
    const activeTerminalId = terminalsStore.state.activeId;
    const activeTerminal = activeTerminalId ? terminalsStore.get(activeTerminalId) : null;
    const activeCwd = activeTerminal?.cwd ?? null;

    if (activeCwd) {
      for (const repoPath of repositoriesStore.getPaths()) {
        const repo = repositoriesStore.get(repoPath);
        if (!repo) continue;

        // When multiple branches share the same worktreePath (main checkout after HEAD move),
        // prefer the branch that owns the active terminal — it reflects the partially-processed
        // head-changed state more accurately than insertion-order iteration.
        if (activeTerminalId) {
          const ownerEntry = Object.entries(repo.branches).find(
            ([, b]) => b.worktreePath === activeCwd && b.terminals.includes(activeTerminalId),
          );
          if (ownerEntry) {
            await handleAddTerminalToBranch(repoPath, ownerEntry[0]);
            return;
          }
        }

        // Linked worktree: unique worktreePath per branch, unambiguous match
        const match = Object.values(repo.branches).find(
          (b) => b.worktreePath && b.worktreePath === activeCwd,
        );
        if (match) {
          await handleAddTerminalToBranch(repoPath, match.name);
          return;
        }
      }
    }

    // Fall back to store's active branch (no active terminal or no CWD match)
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

    let waitAttempts = 0;
    const waitForSession = setInterval(async () => {
      waitAttempts++;
      const terminal = terminalsStore.get(id);
      if (terminal?.sessionId) {
        clearInterval(waitForSession);
        try {
          await deps.pty.write(terminal.sessionId, command + "\n");
        } catch (err) {
          appLogger.error("terminal", "Failed to send run command", err);
        }
      } else if (waitAttempts >= 20) {
        clearInterval(waitForSession);
        appLogger.warn("terminal", "Timed out waiting for session on run command");
      }
    }, 500);
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

  // --- Branch switching ---

  const [switchBranchLists, setSwitchBranchLists] = createSignal<Record<string, string[]>>({});
  const [currentBranches, setCurrentBranches] = createSignal<Record<string, string>>({});

  /** Refresh the local branch list and current branch for all git repos (for context menu). */
  const refreshBranchLists = async () => {
    const repos = repositoriesStore.getOrderedRepos().filter(r => r.isGitRepo !== false);
    const results: Record<string, string[]> = {};
    const heads: Record<string, string> = {};
    await Promise.all(repos.map(async (r) => {
      try {
        const [branches, info] = await Promise.all([
          deps.repo.listLocalBranches(r.path),
          deps.repo.getInfo(r.path),
        ]);
        results[r.path] = branches;
        heads[r.path] = info.branch;
      } catch {
        // skip repos that error
      }
    }));
    setSwitchBranchLists(results);
    setCurrentBranches(heads);
  };

  // Compose branch stats + branch list refresh into a single function
  const refreshAllBranchStatsAndLists = async () => {
    await refreshAllBranchStats();
    await refreshBranchLists();
  };

  /** After a branch switch on the main worktree, migrate all stale branch entries
   *  (same worktreePath as repoPath) into the new branch and remove them. */
  const migrateMainWorktreeBranches = (repoPath: string, newBranch: string) => {
    const repo = repositoriesStore.get(repoPath);
    if (!repo) return;

    // Ensure the target branch entry exists
    repositoriesStore.setBranch(repoPath, newBranch, { worktreePath: repoPath });

    // Find all branches on the main worktree that aren't the new branch
    const stale = Object.values(repo.branches).filter(
      (b) => b.worktreePath === repoPath && b.name !== newBranch,
    );

    batch(() => {
      for (const branch of stale) {
        repositoriesStore.mergeBranchState(repoPath, branch.name, newBranch);
        repositoriesStore.removeBranch(repoPath, branch.name);
      }
      repositoriesStore.setActiveBranch(repoPath, newBranch);
    });
  };

  /** Handle branch switch request from sidebar. Checks terminal safety, then calls Rust. */
  const handleSwitchBranch = async (repoPath: string, branchName: string) => {
    const repo = repositoriesStore.get(repoPath);
    if (!repo) return;

    // Pre-flight: check for busy terminals on the main worktree
    const mainWorktreeBranches = Object.values(repo.branches).filter(b => b.worktreePath === repoPath);
    for (const branch of mainWorktreeBranches) {
      for (const termId of branch.terminals) {
        const term = terminalsStore.get(termId);
        if (term?.shellState === "busy") {
          deps.setStatusInfo(`Cannot switch branch: terminal "${term.name || termId}" has a running process`);
          return;
        }
      }
    }

    try {
      const result = await deps.repo.switchBranch(repoPath, branchName);
      if (result.stashed) {
        deps.setStatusInfo(`Switched to ${result.new_branch} (changes stashed)`);
      } else {
        deps.setStatusInfo(`Switched to ${result.new_branch}`);
      }
      // Migrate all main-worktree branches into the new branch entry, then remove stale ones
      migrateMainWorktreeBranches(repoPath, result.new_branch);
      // Refresh branch stats to pick up the new HEAD
      await refreshAllBranchStatsAndLists();
    } catch (err) {
      const errMsg = String(err);
      if (errMsg === "dirty" || errMsg.includes("dirty")) {
        // Dirty working tree — ask user to stash
        const confirmed = await deps.dialogs.confirmStashAndSwitch?.(branchName);
        if (confirmed) {
          try {
            const result = await deps.repo.switchBranch(repoPath, branchName, { stash: true });
            deps.setStatusInfo(`Switched to ${result.new_branch} (changes stashed)`);
            migrateMainWorktreeBranches(repoPath, result.new_branch);
            await refreshAllBranchStatsAndLists();
          } catch (stashErr) {
            deps.setStatusInfo(`Stash & switch failed: ${stashErr}`);
          }
        }
      } else {
        deps.setStatusInfo(`Branch switch failed: ${errMsg}`);
      }
    }
  };

  const handleCheckoutRemoteBranch = async (repoPath: string, branchName: string) => {
    try {
      await deps.repo.checkoutRemoteBranch(repoPath, branchName);
      deps.setStatusInfo(`Checked out ${branchName}`);
      migrateMainWorktreeBranches(repoPath, branchName);
      await refreshAllBranchStatsAndLists();
      await handleBranchSelectInner(repoPath, branchName);
    } catch (err) {
      appLogger.error("git", "Failed to checkout remote branch", { error: String(err) });
      deps.setStatusInfo(`Checkout failed: ${err}`);
    }
  };

  // --- OSC 7 CWD tracking: reassign terminal to matching worktree branch ---

  const cwdDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

  /** Find the repo + branch whose worktreePath best matches the given cwd (longest prefix). */
  const findBranchForCwd = (cwd: string): { repoPath: string; branchName: string } | null => {
    let best: { repoPath: string; branchName: string } | null = null;
    let bestLen = 0;
    for (const repoPath of repositoriesStore.getPaths()) {
      const repo = repositoriesStore.get(repoPath);
      if (!repo) continue;
      for (const [branchName, branch] of Object.entries(repo.branches)) {
        // For main branches without a worktreePath, the repo path itself is the match
        const wt = branch.worktreePath ?? (branch.isMain ? repoPath : null);
        if (!wt) continue;
        if ((cwd === wt || cwd.startsWith(wt + "/")) && wt.length > bestLen) {
          best = { repoPath, branchName };
          bestLen = wt.length;
        }
      }
    }
    return best;
  };

  /** Async body for debounced CWD change handling — extracted for proper error catching. */
  const performCwdReassignment = async (terminalId: string, newCwd: string) => {
    // Guard: terminal may have been closed during the debounce window
    if (!terminalsStore.get(terminalId)) return;

    const currentRepoPathForTerm = repositoriesStore.getRepoPathForTerminal(terminalId);
    // Find which branch the terminal currently belongs to
    let currentBranchName: string | null = null;
    if (currentRepoPathForTerm) {
      const repo = repositoriesStore.get(currentRepoPathForTerm);
      if (repo) {
        for (const [bName, branch] of Object.entries(repo.branches)) {
          if (branch.terminals.includes(terminalId)) {
            currentBranchName = bName;
            break;
          }
        }
      }
    }

    let target = findBranchForCwd(newCwd);

    // If no match and cwd is inside a known repo, the worktree may have just been created
    if (!target && currentRepoPathForTerm) {
      const isInsideKnownRepo = repositoriesStore.getPaths().some(
        (rp) => newCwd === rp || newCwd.startsWith(rp + "/"),
      );
      if (isInsideKnownRepo) {
        await refreshAllBranchStats();
        target = findBranchForCwd(newCwd);
      }
    }

    if (!target) return; // Unknown path — no reassignment
    // Same branch? Nothing to do
    if (target.repoPath === currentRepoPathForTerm && target.branchName === currentBranchName) return;

    const { repoPath: targetRepoPath, branchName: targetBranchName } = target;

    appLogger.info("terminal", `[CwdChange] ${terminalId} → ${targetRepoPath}:${targetBranchName} (cwd=${newCwd})`);

    batch(() => {
      // Remove from old branch
      if (currentRepoPathForTerm && currentBranchName) {
        repositoriesStore.removeTerminalFromBranch(currentRepoPathForTerm, currentBranchName, terminalId);
      }
      // Add to new branch
      repositoriesStore.addTerminalToBranch(targetRepoPath, targetBranchName, terminalId);

      // If this is the active terminal, switch the active branch
      if (terminalsStore.state.activeId === terminalId) {
        repositoriesStore.setActiveBranch(targetRepoPath, targetBranchName);
        setCurrentBranch(targetBranchName);
        if (targetRepoPath !== currentRepoPathForTerm) {
          repositoriesStore.setActive(targetRepoPath);
          setCurrentRepoPath(targetRepoPath);
        }
      }
    });
  };

  /** Called from Terminal.tsx OSC 7 handler when a terminal's cwd changes. */
  const handleTerminalCwdChange = (terminalId: string, newCwd: string) => {
    // Debounce reassignment per terminal (300ms) — cwd store update is immediate in Terminal.tsx
    clearTimeout(cwdDebounceTimers.get(terminalId));
    cwdDebounceTimers.set(
      terminalId,
      setTimeout(() => {
        cwdDebounceTimers.delete(terminalId);
        void performCwdReassignment(terminalId, newCwd).catch((err) =>
          appLogger.warn("terminal", `[CwdChange] reassignment error for ${terminalId}`, err),
        );
      }, 300),
    );
  };

  /** Cancel any pending CWD debounce timer for a terminal (call on terminal close). */
  const cancelCwdTracking = (terminalId: string) => {
    const timer = cwdDebounceTimers.get(terminalId);
    if (timer !== undefined) {
      clearTimeout(timer);
      cwdDebounceTimers.delete(terminalId);
    }
  };

  /** Get available worktree targets for moving a terminal (excludes current branch). */
  const getWorktreeTargets = (terminalId: string): Array<{ branchName: string; path: string }> => {
    const repoPath = repositoriesStore.getRepoPathForTerminal(terminalId);
    if (!repoPath) return [];
    const repo = repositoriesStore.get(repoPath);
    if (!repo) return [];

    // Find current branch for this terminal
    let currentBranchName: string | null = null;
    for (const [name, branch] of Object.entries(repo.branches)) {
      if (branch.terminals.includes(terminalId)) {
        currentBranchName = name;
        break;
      }
    }

    const targets: Array<{ branchName: string; path: string }> = [];
    for (const [name, branch] of Object.entries(repo.branches)) {
      if (name === currentBranchName) continue;
      const wtPath = branch.worktreePath ?? (branch.isMain ? repoPath : null);
      if (wtPath) {
        targets.push({ branchName: name, path: wtPath });
      }
    }
    return targets;
  };

  /** Move a terminal to a different worktree by sending cd to the PTY. */
  const moveTerminalToWorktree = async (terminalId: string, worktreePath: string): Promise<void> => {
    const terminal = terminalsStore.get(terminalId);
    if (!terminal?.sessionId) return;
    // Single-quote the path to handle spaces safely
    const escaped = "'" + worktreePath.replace(/'/g, "'\\''") + "'";
    await deps.pty.write(terminal.sessionId, `cd ${escaped}\n`);
    appLogger.info("terminal", `[MoveToWorktree] ${terminalId} → cd ${worktreePath}`);
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
    refreshAllBranchStats: refreshAllBranchStatsAndLists,
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
    handleMergeAndArchive,
    mergePendingCtx,
    dismissMergePending,
    closeTerminalsForBranch,
    worktreeDialogState,
    setWorktreeDialogState,
    creatingWorktreeRepos,
    handleNewTab,
    handleRunCommand,
    executeRunCommand,
    generateWorktreeName,
    handleRepoSettings,
    handleCheckoutRemoteBranch,
    handleSwitchBranch,
    handleTerminalCwdChange,
    cancelCwdTracking,
    switchBranchLists,
    currentBranches,
    refreshBranchLists,
    getWorktreeTargets,
    moveTerminalToWorktree,
    /** Create a new terminal for the branch and queue the review command */
    handleReviewPr: async (repoPath: string, branchName: string, command: string) => {
      const termId = await handleAddTerminalToBranch(repoPath, branchName);
      if (termId) {
        terminalsStore.update(termId, { pendingInitCommand: command });
      }
    },
  };
}
