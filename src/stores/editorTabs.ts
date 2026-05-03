import { createTabManager, type BaseTab } from "./tabManager";
import { currentBranchKey } from "./repositories";
import { pathBasename } from "../utils/pathUtils";

/** Editor tab data */
export interface EditorTabData extends BaseTab {
  /** Canonical repo path. Used for branch-scope filtering and repo-store ops
   *  (revisions, diffs). Matches the key used in repositoriesStore. */
  repoPath: string;
  /** Filesystem root for actual file I/O. Equals the active worktree path when
   *  one is selected, otherwise repoPath. Stored separately so the branch
   *  filter (which keys on canonical repoPath) keeps working on worktree
   *  branches. */
  fsRoot: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  isDirty: boolean;
  initialLine?: number; // Line to scroll to on first mount
  externalEditable?: boolean; // Allow editing external (absolute-path) files
}

function createEditorTabsStore() {
  const base = createTabManager<EditorTabData>("editor");

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

    /** Add a new editor tab (or activate existing if same file already open).
     *  `fsRoot` defaults to `repoPath` when omitted — pass it explicitly when
     *  the file lives inside a git worktree whose path differs from the
     *  canonical repo path. */
    add(
      repoPath: string,
      filePath: string,
      initialLine?: number,
      opts?: { fsRoot?: string; externalEditable?: boolean },
    ): string {
      const fsRoot = opts?.fsRoot ?? repoPath;
      const existing = Object.values(base.state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.fsRoot === fsRoot && tab.filePath === filePath,
      );
      if (existing) {
        base.setActive(existing.id);
        return existing.id;
      }

      const id = base._nextId("edit");
      const fileName = pathBasename(filePath) || filePath;
      return base._addTab({ id, repoPath, fsRoot, filePath, fileName, isDirty: false, branchKey: currentBranchKey(), initialLine, externalEditable: opts?.externalEditable });
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
