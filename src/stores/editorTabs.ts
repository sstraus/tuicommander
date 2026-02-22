import { createTabManager, type BaseTab } from "./tabManager";

/** Editor tab data */
export interface EditorTabData extends BaseTab {
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  isDirty: boolean;
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
    getActive: base.getActive,
    getCount: base.getCount,

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
      return base._addTab({ id, repoPath, filePath, fileName, isDirty: false });
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
