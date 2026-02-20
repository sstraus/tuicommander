import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { SavedTerminal } from "../types";

const LEGACY_STORAGE_KEY = "tui-commander-repos";

/** Branch with its terminals */
export interface BranchState {
  name: string;
  isMain: boolean; // true for main/master/develop
  worktreePath: string | null; // Path to worktree directory (null for main branch)
  terminals: string[]; // terminal IDs belonging to this branch
  hadTerminals: boolean; // true once a terminal has been created — suppresses auto-spawn after close-all
  additions: number;
  deletions: number;
  runCommand?: string; // Saved run command for this branch
  savedTerminals?: SavedTerminal[]; // Persisted terminal metadata for session restore
}

/** Repository with branches */
export interface RepositoryState {
  path: string;
  displayName: string;
  initials: string;
  expanded: boolean; // Whether branches are expanded/collapsed
  collapsed: boolean; // Whether entire repo is collapsed to icon only
  branches: Record<string, BranchState>;
  activeBranch: string | null;
}

/** Repositories store state */
interface RepositoriesStoreState {
  repositories: Record<string, RepositoryState>;
  repoOrder: string[];
  activeRepoPath: string | null;
  /** Per-repo monotonic revision counter, bumped by repo-changed events */
  revisions: Record<string, number>;
}

/** Check if branch is a main branch */
export function isMainBranch(branchName: string): boolean {
  const mainBranches = ["main", "master", "develop", "development", "dev"];
  return mainBranches.includes(branchName.toLowerCase());
}

const SAVE_DEBOUNCE_MS = 500;

/** Persist repos to Rust backend (fire-and-forget, terminals excluded) */
function saveReposImmediate(repositories: Record<string, RepositoryState>, repoOrder: string[], activeRepoPath?: string | null): void {
  const serializable: Record<string, RepositoryState> = {};
  for (const [path, repo] of Object.entries(repositories)) {
    const branches: Record<string, BranchState> = {};
    for (const [name, branch] of Object.entries(repo.branches)) {
      branches[name] = { ...branch, terminals: [] };
    }
    serializable[path] = { ...repo, branches };
  }
  invoke("save_repositories", {
    config: { repos: serializable, repoOrder, activeRepoPath: activeRepoPath ?? null },
  }).catch((err) => console.debug("Failed to save repos:", err));
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced save — coalesces rapid mutations into a single IPC call */
function saveRepos(repositories: Record<string, RepositoryState>, repoOrder: string[], activeRepoPath?: string | null): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    saveReposImmediate(repositories, repoOrder, activeRepoPath);
  }, SAVE_DEBOUNCE_MS);
}

/** Create the repositories store */
function createRepositoriesStore() {
  const [state, setState] = createStore<RepositoriesStoreState>({
    repositories: {},
    repoOrder: [],
    activeRepoPath: null,
    revisions: {},
  });

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

        const loaded = await invoke<{ repos?: Record<string, RepositoryState>; repoOrder?: string[]; activeRepoPath?: string | null }>("load_repositories");
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
            for (const branch of Object.values(repo.branches)) {
              branch.terminals = [];
              // Reset hadTerminals on startup: the flag only suppresses auto-spawn
              // within a session (after user closes all terminals). Across restarts,
              // auto-spawn should work unless savedTerminals will restore them.
              branch.hadTerminals = branch.savedTerminals?.length ? true : false;
              if (branch.savedTerminals === undefined) {
                branch.savedTerminals = [];
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

          // Restore active repo
          if (loaded.activeRepoPath && loaded.activeRepoPath in repos) {
            setState("activeRepoPath", loaded.activeRepoPath);
          }
        }
      } catch (err) {
        console.debug("Failed to hydrate repositories:", err);
      }
    },

    /** Add a repository */
    add(repo: { path: string; displayName: string; initials?: string }): void {
      setState("repositories", repo.path, {
        path: repo.path,
        displayName: repo.displayName,
        initials: repo.initials ?? "",
        expanded: true,
        collapsed: false,
        branches: {},
        activeBranch: null,
      });
      if (!state.repoOrder.includes(repo.path)) {
        setState("repoOrder", [...state.repoOrder, repo.path]);
      }
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Remove a repository */
    remove(path: string): void {
      setState(
        produce((s) => {
          delete s.repositories[path];
          s.repoOrder = s.repoOrder.filter((p) => p !== path);
          if (s.activeRepoPath === path) {
            s.activeRepoPath = null;
          }
        })
      );
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Set active repository */
    setActive(path: string | null): void {
      setState("activeRepoPath", path);
      saveRepos(state.repositories, state.repoOrder, path);
    },

    /** Update the display name of a repository */
    setDisplayName(path: string, displayName: string): void {
      if (!state.repositories[path]) return;
      setState("repositories", path, "displayName", displayName);
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Toggle repository expanded state */
    toggleExpanded(path: string): void {
      setState("repositories", path, "expanded", (e) => !e);
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Toggle repository collapsed state */
    toggleCollapsed(path: string): void {
      setState("repositories", path, "collapsed", (c) => !c);
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
          additions: 0,
          deletions: 0,
          ...data,
        });
      }
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Set active branch for a repo */
    setActiveBranch(repoPath: string, branchName: string | null): void {
      setState("repositories", repoPath, "activeBranch", branchName);
    },

    /** Add terminal to branch */
    addTerminalToBranch(repoPath: string, branchName: string, terminalId: string): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      if (branch && !branch.terminals.includes(terminalId)) {
        setState("repositories", repoPath, "branches", branchName, "terminals", (t) => [...t, terminalId]);
        if (!branch.hadTerminals) {
          setState("repositories", repoPath, "branches", branchName, "hadTerminals", true);
        }
        saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
      }
    },

    /** Remove terminal from branch */
    removeTerminalFromBranch(repoPath: string, branchName: string, terminalId: string): void {
      setState("repositories", repoPath, "branches", branchName, "terminals", (t) =>
        t.filter((id) => id !== terminalId)
      );
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Set run command for a branch */
    setRunCommand(repoPath: string, branchName: string, command: string | undefined): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      if (branch) {
        setState("repositories", repoPath, "branches", branchName, "runCommand", command);
        saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Get ordered repo paths */
    getOrderedRepos(): RepositoryState[] {
      return state.repoOrder
        .map((path) => state.repositories[path])
        .filter(Boolean);
    },

    /** Reorder terminals within the active branch */
    reorderTerminals(repoPath: string, branchName: string, fromIndex: number, toIndex: number): void {
      setState("repositories", repoPath, "branches", branchName, "terminals", (terminals) => {
        const result = [...terminals];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
      saveReposImmediate(state.repositories, state.repoOrder, state.activeRepoPath);
    },

    /** Update savedTerminals from live terminal state (called on terminal add/remove) */
    updateSavedTerminals(getTerminal: (id: string) => { name: string; cwd: string | null; fontSize: number; agentType: string | null } | undefined): void {
      setState(
        produce((s) => {
          for (const repo of Object.values(s.repositories)) {
            for (const branch of Object.values(repo.branches)) {
              if (branch.terminals.length === 0) {
                branch.savedTerminals = [];
                continue;
              }
              const saved: SavedTerminal[] = [];
              for (const termId of branch.terminals) {
                const t = getTerminal(termId);
                if (!t) continue;
                saved.push({
                  name: t.name,
                  cwd: t.cwd,
                  fontSize: t.fontSize,
                  agentType: t.agentType as SavedTerminal["agentType"],
                });
              }
              branch.savedTerminals = saved;
            }
          }
        })
      );
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
      saveRepos(state.repositories, state.repoOrder, state.activeRepoPath);
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
  };

  return { state, ...actions };
}

export const repositoriesStore = createRepositoriesStore();
