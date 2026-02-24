import { createTabManager, makeBranchKey, type BaseTab } from "./tabManager";
import { repositoriesStore } from "./repositories";

/** Editor tab data */
export interface EditorTabData extends BaseTab {
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  isDirty: boolean;
}

/** Get the branch key for the currently active repo+branch */
function currentBranchKey(): string | undefined {
  const repoPath = repositoriesStore.state.activeRepoPath;
  if (!repoPath) return undefined;
  const repo = repositoriesStore.state.repositories[repoPath];
  if (!repo?.activeBranch) return undefined;
  return makeBranchKey(repoPath, repo.activeBranch);
}

function createEditorTabsStore() {
  const base = createTabManager<EditorTabData>();

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

    /** Add a new editor tab (or activate existing if same file already open) */
    add(repoPath: string, filePath: string): string {
      const existing = Object.values(base.state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath,
      );
      if (existing) {
        base.setActive(existing.id);
        return existing.id;
      }

      const id = base._nextId("edit");
      const fileName = filePath.split("/").pop() || filePath;
      return base._addTab({ id, repoPath, filePath, fileName, isDirty: false, branchKey: currentBranchKey() });
    },

    /** Mark a tab as dirty or clean */
    setDirty(id: string, isDirty: boolean): void {
      if (base.state.tabs[id]) {
        base._setState("tabs", id, "isDirty", isDirty);
      }
    },

    /** Clear all editor tabs for a repository */
    clearForRepo(repoPath: string): void {
      base._clearWhere((tab) => tab.repoPath === repoPath);
    },
  };
}

export const editorTabsStore = createEditorTabsStore();
