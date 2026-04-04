import { createTabManager, type BaseTab } from "./tabManager";
import { currentBranchKey } from "./repositories";
import { terminalsStore } from "./terminals";
import { mdTabsStore } from "./mdTabs";
import { editorTabsStore } from "./editorTabs";

export type DiffStatus = "M" | "A" | "D" | "R" | "?";

const VALID_DIFF_STATUSES = new Set<string>(["M", "A", "D", "R", "?"]);

/** Type guard for DiffStatus values received from backend */
export function isDiffStatus(value: unknown): value is DiffStatus {
  return typeof value === "string" && VALID_DIFF_STATUSES.has(value);
}

/** Diff tab data */
export interface DiffTabData extends BaseTab {
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  status: DiffStatus;
  scope?: string; // "working" (default) or "committed" (HEAD~1)
  untracked?: boolean; // True for "?" status files — skips redundant ls-files probe
}

function createDiffTabsStore() {
  const base = createTabManager<DiffTabData>("diff");
  const handles = new Map<string, unknown>();

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

    /** Add a new diff tab (or return existing if same file+scope already open).
     *  Deactivates terminal/md/editor tabs so the diff pane becomes visible. */
    add(repoPath: string, filePath: string, status: DiffStatus, scope?: string, untracked?: boolean): string {
      const existing = Object.values(base.state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath && tab.scope === scope,
      );
      if (existing) {
        base.setActive(existing.id);
        terminalsStore.setActive(null);
        mdTabsStore.setActive(null);
        editorTabsStore.setActive(null);
        return existing.id;
      }

      const id = base._nextId("diff");
      const fileName = filePath ? (filePath.split("/").pop() || filePath) : "Diff Scroll";
      const tabId = base._addTab({ id, repoPath, filePath, fileName, status, scope, untracked, branchKey: currentBranchKey() });
      terminalsStore.setActive(null);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      return tabId;
    },

    /** Register an imperative handle for a tab (e.g. openSearch) */
    setHandle(tabId: string, handle: unknown): void {
      handles.set(tabId, handle);
    },

    /** Remove the imperative handle when a tab component unmounts */
    clearHandle(tabId: string): void {
      handles.delete(tabId);
    },

    /** Retrieve the imperative handle for a tab */
    getHandle<T = unknown>(tabId: string): T | undefined {
      return handles.get(tabId) as T | undefined;
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
