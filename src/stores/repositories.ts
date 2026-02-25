import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { SavedTerminal } from "../types";
import { appLogger } from "./appLogger";

const LEGACY_STORAGE_KEY = "tui-commander-repos";

/** Branch with its terminals */
export interface BranchState {
  name: string;
  isMain: boolean; // true for main/master/develop
  isShell?: boolean; // true for non-git directory shell entries
  worktreePath: string | null; // Path to worktree directory (null for main branch)
  terminals: string[]; // terminal IDs belonging to this branch
  hadTerminals: boolean; // true once a terminal has been created — suppresses auto-spawn after close-all
  lastActiveTerminal: string | null; // last active terminal ID when leaving this branch
  additions: number;
  deletions: number;
  isMerged: boolean; // true when branch is fully merged into the repo's main branch
  runCommand?: string; // Saved run command for this branch
  savedTerminals?: SavedTerminal[]; // Persisted terminal metadata for session restore
}

/** Repository with branches */
export interface RepositoryState {
  path: string;
  displayName: string;
  initials: string;
  isGitRepo?: boolean; // false for plain directories (defaults to true for backward compat)
  expanded: boolean; // Whether branches are expanded/collapsed
  collapsed: boolean; // Whether entire repo is collapsed to icon only
  parked: boolean;    // Whether repo is hidden from sidebar (recallable via popover)
  showAllBranches: boolean; // Whether to show all local branches (not just worktrees + active)
  branches: Record<string, BranchState>;
  activeBranch: string | null;
}

/** A named, colored group of repositories */
export interface RepoGroup {
  id: string;
  name: string;
  color: string;        // hex color or "" for default
  collapsed: boolean;   // accordion state
  repoOrder: string[];  // ordered repo paths in this group
}

/** Repositories store state */
interface RepositoriesStoreState {
  repositories: Record<string, RepositoryState>;
  repoOrder: string[];         // ungrouped repo order
  activeRepoPath: string | null;
  /** Per-repo monotonic revision counter, bumped by repo-changed events */
  revisions: Record<string, number>;
  groups: Record<string, RepoGroup>;
  groupOrder: string[];        // display order of group IDs
}

/** Grouped layout returned by getGroupedLayout() */
export interface GroupedLayout {
  groups: Array<{ group: RepoGroup; repos: RepositoryState[] }>;
  ungrouped: RepositoryState[];
}

/** Fallback check when Rust-provided is_main is not available (e.g. local rename). */
function isMainBranch(branchName: string): boolean {
  const mainBranches = ["main", "master", "develop", "development", "dev"];
  return mainBranches.includes(branchName.toLowerCase());
}

const SAVE_DEBOUNCE_MS = 500;

/** Guard: prevent saves before hydrate completes to avoid nuking persisted data */
let hydrated = false;

/** Persist repos to Rust backend (fire-and-forget, terminals excluded) */
function saveReposImmediate(
  repositories: Record<string, RepositoryState>,
  repoOrder: string[],
  activeRepoPath: string | null | undefined,
  groups: Record<string, RepoGroup>,
  groupOrder: string[],
): void {
  if (!hydrated) {
    console.warn("[Repositories] Save blocked — hydrate not yet complete");
    return;
  }
  const serializable: Record<string, RepositoryState> = {};
  for (const [path, repo] of Object.entries(repositories)) {
    const branches: Record<string, BranchState> = {};
    for (const [name, branch] of Object.entries(repo.branches)) {
      branches[name] = { ...branch, terminals: [] };
    }
    serializable[path] = { ...repo, branches };
  }
  invoke("save_repositories", {
    config: {
      repos: serializable,
      repoOrder,
      activeRepoPath: activeRepoPath ?? null,
      groups,
      groupOrder,
    },
  }).catch((err) => console.error("Failed to save repos:", err));
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced save — coalesces rapid mutations into a single IPC call */
function saveRepos(
  repositories: Record<string, RepositoryState>,
  repoOrder: string[],
  activeRepoPath: string | null | undefined,
  groups: Record<string, RepoGroup>,
  groupOrder: string[],
): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    saveReposImmediate(repositories, repoOrder, activeRepoPath, groups, groupOrder);
  }, SAVE_DEBOUNCE_MS);
}

/** Generate a unique group ID */
function generateGroupId(): string {
  return `grp-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/** Create the repositories store */
function createRepositoriesStore() {
  const [state, setState] = createStore<RepositoriesStoreState>({
    repositories: {},
    repoOrder: [],
    activeRepoPath: null,
    revisions: {},
    groups: {},
    groupOrder: [],
  });

  /** Debounced save shorthand using current state */
  const save = () => saveRepos(state.repositories, state.repoOrder, state.activeRepoPath, state.groups, state.groupOrder);

  /** Immediate save shorthand using current state (for app exit) */
  const saveNow = () => saveReposImmediate(state.repositories, state.repoOrder, state.activeRepoPath, state.groups, state.groupOrder);

  const actions = {
    /** Load repos from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = JSON.parse(legacy);
            await invoke("save_repositories", { config: { repos: parsed } });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const loaded = await invoke<{
          repos?: Record<string, RepositoryState>;
          repoOrder?: string[];
          activeRepoPath?: string | null;
          groups?: Record<string, RepoGroup>;
          groupOrder?: string[];
        }>("load_repositories");
        const repos = loaded?.repos;
        if (repos) {
          // Migration: add collapsed/expanded fields, clear stale terminal IDs
          Object.values(repos).forEach((repo) => {
            if (repo.collapsed === undefined) {
              repo.collapsed = false;
            }
            if (repo.expanded === undefined) {
              repo.expanded = true;
            }
            if (repo.parked === undefined) {
              repo.parked = false;
            }
            if (repo.showAllBranches === undefined) {
              repo.showAllBranches = false;
            }
            for (const branch of Object.values(repo.branches)) {
              branch.terminals = [];
              // Reset hadTerminals on startup: the flag only suppresses auto-spawn
              // within a session (after user closes all terminals). Across restarts,
              // auto-spawn should work unless savedTerminals will restore them.
              branch.hadTerminals = branch.savedTerminals?.length ? true : false;
              if (branch.savedTerminals === undefined) {
                branch.savedTerminals = [];
              }
              if (branch.isMerged === undefined) {
                branch.isMerged = false;
              }
            }
          });
          setState("repositories", repos);

          // Hydrate repoOrder: use saved order, falling back to Object.keys for repos not yet in the order
          const repoPaths = Object.keys(repos);
          const savedOrder = loaded.repoOrder ?? [];
          const validOrder = savedOrder.filter((p) => p in repos);
          const missing = repoPaths.filter((p) => !validOrder.includes(p));
          setState("repoOrder", [...validOrder, ...missing]);

          // Hydrate groups — migration: missing groups field initializes empty
          setState("groups", loaded.groups ?? {});
          setState("groupOrder", loaded.groupOrder ?? []);

          // Restore active repo
          if (loaded.activeRepoPath && loaded.activeRepoPath in repos) {
            setState("activeRepoPath", loaded.activeRepoPath);
          }
        }
        hydrated = true;
      } catch (err) {
        console.error("Failed to hydrate repositories:", err);
        // hydrated stays false — saves are blocked to prevent data loss
      }
    },

    /** Add a repository */
    add(repo: { path: string; displayName: string; initials?: string; showAllBranches?: boolean; isGitRepo?: boolean }): void {
      setState("repositories", repo.path, {
        path: repo.path,
        displayName: repo.displayName,
        initials: repo.initials ?? "",
        isGitRepo: repo.isGitRepo ?? true,
        expanded: true,
        collapsed: false,
        parked: false,
        showAllBranches: repo.showAllBranches ?? false,
        branches: {},
        activeBranch: null,
      });
      if (!state.repoOrder.includes(repo.path)) {
        setState("repoOrder", [...state.repoOrder, repo.path]);
      }
      save();
    },

    /** Remove a repository */
    remove(path: string): void {
      setState(
        produce((s) => {
          delete s.repositories[path];
          s.repoOrder = s.repoOrder.filter((p) => p !== path);
          // Clean up group membership
          for (const group of Object.values(s.groups)) {
            group.repoOrder = group.repoOrder.filter((p) => p !== path);
          }
          if (s.activeRepoPath === path) {
            s.activeRepoPath = null;
          }
        })
      );
      save();
    },

    /** Set active repository */
    setActive(path: string | null): void {
      setState("activeRepoPath", path);
      save();
    },

    /** Update the display name of a repository */
    setDisplayName(path: string, displayName: string): void {
      if (!state.repositories[path]) return;
      setState("repositories", path, "displayName", displayName);
      save();
    },

    /** Toggle repository expanded state */
    toggleExpanded(path: string): void {
      setState("repositories", path, "expanded", (e) => !e);
      save();
    },

    /** Toggle repository collapsed state */
    toggleCollapsed(path: string): void {
      setState("repositories", path, "collapsed", (c) => !c);
      save();
    },

    /** Toggle show-all-branches state */
    toggleShowAllBranches(path: string): void {
      setState("repositories", path, "showAllBranches", (v) => !v);
      save();
    },

    /** Update git repo status (used when a directory gains or loses .git) */
    setIsGitRepo(path: string, isGitRepo: boolean): void {
      setState("repositories", path, "isGitRepo", isGitRepo);
      save();
    },

    /** Add or update a branch */
    setBranch(repoPath: string, branchName: string, data?: Partial<BranchState>): void {
      const existing = state.repositories[repoPath]?.branches[branchName];
      if (existing) {
        setState("repositories", repoPath, "branches", branchName, (prev) => ({
          ...prev,
          ...data,
        }));
      } else {
        setState("repositories", repoPath, "branches", branchName, {
          name: branchName,
          isMain: isMainBranch(branchName),
          worktreePath: null,
          terminals: [],
          hadTerminals: false,
          lastActiveTerminal: null,
          additions: 0,
          deletions: 0,
          isMerged: false,
          ...data,
        });
      }
      save();
    },

    /** Set active branch for a repo */
    setActiveBranch(repoPath: string, branchName: string | null): void {
      setState("repositories", repoPath, "activeBranch", branchName);
    },

    /** Add terminal to branch */
    addTerminalToBranch(repoPath: string, branchName: string, terminalId: string): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      if (branch && !branch.terminals.includes(terminalId)) {
        appLogger.info("terminal", `addTerminalToBranch ${branchName} += ${terminalId}`, { before: [...branch.terminals] });
        setState("repositories", repoPath, "branches", branchName, "terminals", (t) => [...t, terminalId]);
        if (!branch.hadTerminals) {
          setState("repositories", repoPath, "branches", branchName, "hadTerminals", true);
        }
        save();
      }
    },

    /** Remove terminal from branch */
    removeTerminalFromBranch(repoPath: string, branchName: string, terminalId: string): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      appLogger.info("terminal", `removeTerminalFromBranch ${branchName} -= ${terminalId}`, { before: branch?.terminals ? [...branch.terminals] : [] });
      setState("repositories", repoPath, "branches", branchName, "terminals", (t) =>
        t.filter((id) => id !== terminalId)
      );
      save();
    },

    /** Set run command for a branch */
    setRunCommand(repoPath: string, branchName: string, command: string | undefined): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      if (branch) {
        setState("repositories", repoPath, "branches", branchName, "runCommand", command);
        save();
      }
    },

    /** Update branch stats (additions/deletions) — only if branch already exists */
    updateBranchStats(repoPath: string, branchName: string, additions: number, deletions: number): void {
      if (!state.repositories[repoPath]?.branches[branchName]) return;
      setState("repositories", repoPath, "branches", branchName, { additions, deletions });
    },

    /** Remove a branch from a repository */
    removeBranch(repoPath: string, branchName: string): void {
      const repo = state.repositories[repoPath];
      if (!repo) return;

      const branch = repo.branches[branchName];
      if (branch) {
        appLogger.error("terminal", `removeBranch "${branchName}" from ${repoPath}`, {
          terminals: branch.terminals,
          hadTerminals: branch.hadTerminals,
          savedTerminals: branch.savedTerminals?.length ?? 0,
        });
      }

      setState(
        produce((s) => {
          const r = s.repositories[repoPath];
          if (!r) return;

          // Delete the branch
          delete r.branches[branchName];

          // Clear active branch if it was removed
          if (r.activeBranch === branchName) {
            const remainingBranches = Object.keys(r.branches);
            r.activeBranch = remainingBranches[0] || null;
          }
        })
      );
      save();
    },

    /** Rename a branch in a repository */
    renameBranch(repoPath: string, oldName: string, newName: string): void {
      const repo = state.repositories[repoPath];
      if (!repo || !repo.branches[oldName]) return;

      setState(
        produce((s) => {
          const r = s.repositories[repoPath];
          if (!r) return;

          // Get the old branch data
          const oldBranch = r.branches[oldName];
          if (!oldBranch) return;

          // Create new branch entry with updated name
          r.branches[newName] = {
            ...oldBranch,
            name: newName,
            isMain: isMainBranch(newName),
          };

          // Delete the old branch entry
          delete r.branches[oldName];

          // Update active branch if it was renamed
          if (r.activeBranch === oldName) {
            r.activeBranch = newName;
          }
        })
      );
      save();
    },

    /** Get repository by path */
    get(path: string): RepositoryState | undefined {
      return state.repositories[path];
    },

    /** Get active repository */
    getActive(): RepositoryState | undefined {
      return state.activeRepoPath ? state.repositories[state.activeRepoPath] : undefined;
    },

    /** Get all repository paths */
    getPaths(): string[] {
      return Object.keys(state.repositories);
    },

    /** Reorder repositories in the sidebar */
    reorderRepo(fromIndex: number, toIndex: number): void {
      setState("repoOrder", (order) => {
        const result = [...order];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      save();
    },

    /** Park or unpark a repository (hide from sidebar, recallable via popover) */
    setPark(path: string, parked: boolean): void {
      if (!state.repositories[path]) return;
      setState("repositories", path, "parked", parked);
      save();
    },

    /** Get all parked repositories */
    getParkedRepos(): RepositoryState[] {
      return Object.values(state.repositories).filter((r) => r.parked);
    },

    /** Get ordered repo paths (excludes parked repos) */
    getOrderedRepos(): RepositoryState[] {
      return state.repoOrder
        .map((path) => state.repositories[path])
        .filter((r) => r && !r.parked);
    },

    /** Reorder terminals within the active branch */
    reorderTerminals(repoPath: string, branchName: string, fromIndex: number, toIndex: number): void {
      setState("repositories", repoPath, "branches", branchName, "terminals", (terminals) => {
        const result = [...terminals];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      save();
    },

    /** Reverse-lookup: find which repo a terminal belongs to (O(repos*branches) scan, but
     *  only called per-terminal on render — not in a hot reactive loop). */
    getRepoPathForTerminal(termId: string): string | null {
      for (const [path, repo] of Object.entries(state.repositories)) {
        for (const branch of Object.values(repo.branches)) {
          if (branch.terminals.includes(termId)) return path;
        }
      }
      return null;
    },

    /** Get terminals for current active branch */
    getActiveTerminals(): string[] {
      const repo = actions.getActive();
      if (!repo || !repo.activeBranch) return [];
      return repo.branches[repo.activeBranch]?.terminals || [];
    },

    /** Snapshot terminal metadata into each branch for persistence (called at quit time) */
    snapshotTerminals(snapshots: Map<string, Map<string, SavedTerminal[]>>): void {
      setState(
        produce((s) => {
          for (const [repoPath, branches] of snapshots) {
            const repo = s.repositories[repoPath];
            if (!repo) continue;
            for (const [branchName, terminals] of branches) {
              const branch = repo.branches[branchName];
              if (!branch) continue;
              branch.savedTerminals = terminals;
            }
          }
        })
      );
      // Flush immediately (not debounced) — app is about to exit
      saveNow();
    },

    /** Clear savedTerminals from all branches (consume-once after restore) */
    clearSavedTerminals(): void {
      setState(
        produce((s) => {
          for (const repo of Object.values(s.repositories)) {
            for (const branch of Object.values(repo.branches)) {
              branch.savedTerminals = [];
            }
          }
        })
      );
      save();
    },

    /** Bump the revision counter for a repo (signals panels to re-fetch) */
    bumpRevision(repoPath: string): void {
      setState("revisions", repoPath, (n) => (n ?? 0) + 1);
    },

    /** Get the current revision counter for a repo (reactive — tracks in effects) */
    getRevision(repoPath: string): number {
      return state.revisions[repoPath] ?? 0;
    },

    /** Check if empty */
    isEmpty(): boolean {
      return Object.keys(state.repositories).length === 0;
    },

    // ── Group CRUD ──

    /** Create a new group. Returns ID or null if name is duplicate (case-insensitive). */
    createGroup(name: string): string | null {
      const nameLower = name.toLowerCase();
      const exists = Object.values(state.groups).some((g) => g.name.toLowerCase() === nameLower);
      if (exists) return null;

      const id = generateGroupId();
      setState("groups", id, { id, name, color: "", collapsed: false, repoOrder: [] });
      setState("groupOrder", [...state.groupOrder, id]);
      save();
      return id;
    },

    /** Delete a group — repos move to ungrouped */
    deleteGroup(id: string): void {
      const group = state.groups[id];
      if (!group) return;
      setState(
        produce((s) => {
          // Move repos to ungrouped order
          const repos = s.groups[id]?.repoOrder ?? [];
          s.repoOrder = [...s.repoOrder, ...repos];
          delete s.groups[id];
          s.groupOrder = s.groupOrder.filter((gid) => gid !== id);
        })
      );
      save();
    },

    /** Rename a group. Returns false if name is duplicate (case-insensitive). */
    renameGroup(id: string, newName: string): boolean {
      const nameLower = newName.toLowerCase();
      const exists = Object.values(state.groups).some(
        (g) => g.id !== id && g.name.toLowerCase() === nameLower,
      );
      if (exists) return false;
      setState("groups", id, "name", newName);
      save();
      return true;
    },

    /** Set group color */
    setGroupColor(id: string, color: string): void {
      if (!state.groups[id]) return;
      setState("groups", id, "color", color);
      save();
    },

    /** Toggle group collapsed/expanded */
    toggleGroupCollapsed(id: string): void {
      if (!state.groups[id]) return;
      setState("groups", id, "collapsed", (c) => !c);
      save();
    },

    // ── Group assignment ──

    /** Add repo to a group (removes from ungrouped or previous group) */
    addRepoToGroup(repoPath: string, groupId: string): void {
      if (!state.groups[groupId]) return;
      setState(
        produce((s) => {
          // Remove from ungrouped
          s.repoOrder = s.repoOrder.filter((p) => p !== repoPath);
          // Remove from any other group
          for (const group of Object.values(s.groups)) {
            group.repoOrder = group.repoOrder.filter((p) => p !== repoPath);
          }
          // Add to target group
          s.groups[groupId].repoOrder = [...s.groups[groupId].repoOrder, repoPath];
        })
      );
      save();
    },

    /** Remove repo from its group back to ungrouped */
    removeRepoFromGroup(repoPath: string): void {
      setState(
        produce((s) => {
          for (const group of Object.values(s.groups)) {
            group.repoOrder = group.repoOrder.filter((p) => p !== repoPath);
          }
          if (!s.repoOrder.includes(repoPath)) {
            s.repoOrder = [...s.repoOrder, repoPath];
          }
        })
      );
      save();
    },

    /** Find which group a repo belongs to (or undefined if ungrouped) */
    getGroupForRepo(repoPath: string): RepoGroup | undefined {
      // Direct lookup via group iteration with early return — O(groups) worst case
      // instead of O(groups * repos_per_group) with Array.includes
      for (const group of Object.values(state.groups)) {
        if (group.repoOrder.indexOf(repoPath) !== -1) return group;
      }
      return undefined;
    },

    // ── Group reordering ──

    /** Reorder a repo within its group */
    reorderRepoInGroup(groupId: string, fromIndex: number, toIndex: number): void {
      if (!state.groups[groupId]) return;
      setState("groups", groupId, "repoOrder", (order) => {
        const result = [...order];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      save();
    },

    /** Move repo from one group to another at a specific index */
    moveRepoBetweenGroups(repoPath: string, fromGroupId: string, toGroupId: string, toIndex: number): void {
      if (!state.groups[fromGroupId] || !state.groups[toGroupId]) return;
      setState(
        produce((s) => {
          s.groups[fromGroupId].repoOrder = s.groups[fromGroupId].repoOrder.filter((p) => p !== repoPath);
          const target = [...s.groups[toGroupId].repoOrder];
          target.splice(toIndex, 0, repoPath);
          s.groups[toGroupId].repoOrder = target;
        })
      );
      save();
    },

    /** Reorder groups in the display order */
    reorderGroups(fromIndex: number, toIndex: number): void {
      setState("groupOrder", (order) => {
        const result = [...order];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      save();
    },

    /** Get the grouped layout for rendering: ordered groups with their repos, plus ungrouped repos */
    getGroupedLayout(): GroupedLayout {
      const groups = state.groupOrder
        .map((gid) => state.groups[gid])
        .filter(Boolean)
        .map((group) => ({
          group,
          repos: group.repoOrder
            .map((path) => state.repositories[path])
            .filter((r) => r && !r.parked),
        }));

      // Collect all repo paths that belong to a group
      const groupedPaths = new Set(
        groups.flatMap((g) => g.group.repoOrder),
      );

      const ungrouped = state.repoOrder
        .filter((path) => !groupedPaths.has(path))
        .map((path) => state.repositories[path])
        .filter((r) => r && !r.parked);

      return { groups, ungrouped };
    },
  };

  return {
    state,
    ...actions,
    /** Test-only: set hydrated flag to enable saves in tests that skip hydrate */
    _testSetHydrated(value: boolean): void { hydrated = value; },
  };
}

export const repositoriesStore = createRepositoriesStore();
