import { createTabManager, type BaseTab } from "./tabManager";

/** Diff tab data */
export interface DiffTabData extends BaseTab {
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  status: string; // "M" | "A" | "D" | "R"
  scope?: string; // "working" (default) or "committed" (HEAD~1)
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
    getActive: base.getActive,
    getCount: base.getCount,

    /** Add a new diff tab (or return existing if same file+scope already open) */
    add(repoPath: string, filePath: string, status: string, scope?: string): string {
      const existing = Object.values(base.state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath && tab.scope === scope,
      );
      if (existing) {
        base.setActive(existing.id);
        return existing.id;
      }

      const id = base._nextId("diff");
      const fileName = filePath.split("/").pop() || filePath;
      return base._addTab({ id, repoPath, filePath, fileName, status, scope });
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
