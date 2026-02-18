import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";

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
  activeRepoPath: string | null;
}

/** Check if branch is a main branch */
export function isMainBranch(branchName: string): boolean {
  const mainBranches = ["main", "master", "develop", "development", "dev"];
  return mainBranches.includes(branchName.toLowerCase());
}

const SAVE_DEBOUNCE_MS = 500;

/** Persist repos to Rust backend (fire-and-forget, terminals excluded) */
function saveReposImmediate(repositories: Record<string, RepositoryState>): void {
  const serializable: Record<string, RepositoryState> = {};
  for (const [path, repo] of Object.entries(repositories)) {
    const branches: Record<string, BranchState> = {};
    for (const [name, branch] of Object.entries(repo.branches)) {
      branches[name] = { ...branch, terminals: [] };
    }
    serializable[path] = { ...repo, branches };
  }
  invoke("save_repositories", {
    config: { repos: serializable },
  }).catch((err) => console.debug("Failed to save repos:", err));
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced save — coalesces rapid mutations into a single IPC call */
function saveRepos(repositories: Record<string, RepositoryState>): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    saveReposImmediate(repositories);
  }, SAVE_DEBOUNCE_MS);
}

/** Create the repositories store */
function createRepositoriesStore() {
  const [state, setState] = createStore<RepositoriesStoreState>({
    repositories: {},
    activeRepoPath: null,
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

        const loaded = await invoke<{ repos?: Record<string, RepositoryState> }>("load_repositories");
        if (loaded?.repos) {
          // Migration: add collapsed/expanded fields, clear stale terminal IDs
          Object.values(loaded.repos).forEach((repo) => {
            if (repo.collapsed === undefined) {
              repo.collapsed = false;
            }
            if (repo.expanded === undefined) {
              repo.expanded = true;
            }
            for (const branch of Object.values(repo.branches)) {
              branch.terminals = [];
              if (branch.hadTerminals === undefined) {
                branch.hadTerminals = false;
              }
            }
          });
          setState("repositories", loaded.repos);
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
      saveRepos(state.repositories);
    },

    /** Remove a repository */
    remove(path: string): void {
      setState(
        produce((s) => {
          delete s.repositories[path];
          if (s.activeRepoPath === path) {
            s.activeRepoPath = null;
          }
        })
      );
      saveRepos(state.repositories);
    },

    /** Set active repository */
    setActive(path: string | null): void {
      setState("activeRepoPath", path);
    },

    /** Toggle repository expanded state */
    toggleExpanded(path: string): void {
      setState("repositories", path, "expanded", (e) => !e);
      saveRepos(state.repositories);
    },

    /** Toggle repository collapsed state */
    toggleCollapsed(path: string): void {
      setState("repositories", path, "collapsed", (c) => !c);
      saveRepos(state.repositories);
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
      saveRepos(state.repositories);
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
        saveRepos(state.repositories);
      }
    },

    /** Remove terminal from branch */
    removeTerminalFromBranch(repoPath: string, branchName: string, terminalId: string): void {
      setState("repositories", repoPath, "branches", branchName, "terminals", (t) =>
        t.filter((id) => id !== terminalId)
      );
      saveRepos(state.repositories);
    },

    /** Set run command for a branch */
    setRunCommand(repoPath: string, branchName: string, command: string | undefined): void {
      const branch = state.repositories[repoPath]?.branches[branchName];
      if (branch) {
        setState("repositories", repoPath, "branches", branchName, "runCommand", command);
        saveRepos(state.repositories);
      }
    },

    /** Update branch stats (additions/deletions) */
    updateBranchStats(repoPath: string, branchName: string, additions: number, deletions: number): void {
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
      saveRepos(state.repositories);
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
      saveRepos(state.repositories);
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

    /** Reorder terminals within the active branch */
    reorderTerminals(repoPath: string, branchName: string, fromIndex: number, toIndex: number): void {
      setState("repositories", repoPath, "branches", branchName, "terminals", (terminals) => {
        const result = [...terminals];
        const [moved] = result.splice(fromIndex, 1);
        result.splice(toIndex, 0, moved);
        return result;
      });
      saveRepos(state.repositories);
    },

    /** Get terminals for current active branch */
    getActiveTerminals(): string[] {
      const repo = actions.getActive();
      if (!repo || !repo.activeBranch) return [];
      return repo.branches[repo.activeBranch]?.terminals || [];
    },

    /** Check if empty */
    isEmpty(): boolean {
      return Object.keys(state.repositories).length === 0;
    },
  };

  return { state, ...actions };
}

export const repositoriesStore = createRepositoriesStore();
