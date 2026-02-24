import { createTabManager, makeBranchKey, type BaseTab } from "./tabManager";
import { repositoriesStore } from "./repositories";

export type DiffStatus = "M" | "A" | "D" | "R";

/** Diff tab data */
export interface DiffTabData extends BaseTab {
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  status: DiffStatus;
  scope?: string; // "working" (default) or "committed" (HEAD~1)
}

/** Get the branch key for the currently active repo+branch */
function currentBranchKey(): string | undefined {
  const repoPath = repositoriesStore.state.activeRepoPath;
  if (!repoPath) return undefined;
  const repo = repositoriesStore.state.repositories[repoPath];
  if (!repo?.activeBranch) return undefined;
  return makeBranchKey(repoPath, repo.activeBranch);
}

function createDiffTabsStore() {
  const base = createTabManager<DiffTabData>();

  return {
    state: base.state,
    remove: base.remove,
    setActive: base.setActive,
    clearAll: base.clearAll,
    get: base.get,
    getIds: base.getIds,
    getVisibleIds: base.getVisibleIds,
    getActive: base.getActive,
    getCount: base.getCount,
    setPinned: base.setPinned,

    /** Add a new diff tab (or return existing if same file+scope already open) */
    add(repoPath: string, filePath: string, status: DiffStatus, scope?: string): string {
      const existing = Object.values(base.state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath && tab.scope === scope,
      );
      if (existing) {
        base.setActive(existing.id);
        return existing.id;
      }

      const id = base._nextId("diff");
      const fileName = filePath.split("/").pop() || filePath;
      return base._addTab({ id, repoPath, filePath, fileName, status, scope, branchKey: currentBranchKey() });
    },

    /** Clear all diff tabs for a repository */
    clearForRepo(repoPath: string): void {
      base._clearWhere((tab) => tab.repoPath === repoPath);
    },

    /** Get tabs for a specific repository */
    getForRepo(repoPath: string): DiffTabData[] {
      return Object.values(base.state.tabs).filter((tab) => tab.repoPath === repoPath);
    },
  };
}

export const diffTabsStore = createDiffTabsStore();
