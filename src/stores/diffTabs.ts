import { createStore, produce } from "solid-js/store";

/** Diff tab data */
export interface DiffTabData {
  id: string;
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  status: string; // "M" | "A" | "D" | "R"
}

/** Diff tabs store state */
interface DiffTabsStoreState {
  tabs: Record<string, DiffTabData>;
  activeId: string | null;
  counter: number;
}

/** Create the diff tabs store */
function createDiffTabsStore() {
  const [state, setState] = createStore<DiffTabsStoreState>({
    tabs: {},
    activeId: null,
    counter: 0,
  });

  const actions = {
    /** Add a new diff tab (or return existing if same file already open) */
    add(repoPath: string, filePath: string, status: string): string {
      // Check if tab for this file already exists
      const existing = Object.values(state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath
      );
      if (existing) {
        setState("activeId", existing.id);
        return existing.id;
      }

      // Create new tab
      const id = `diff-${state.counter + 1}`;
      const fileName = filePath.split("/").pop() || filePath;
      setState("counter", (c) => c + 1);
      setState("tabs", id, { id, repoPath, filePath, fileName, status });
      setState("activeId", id);
      return id;
    },

    /** Remove a diff tab */
    remove(id: string): void {
      setState(
        produce((s) => {
          delete s.tabs[id];
          // If we removed the active tab, select another
          if (s.activeId === id) {
            const remaining = Object.keys(s.tabs);
            s.activeId = remaining.length > 0 ? remaining[remaining.length - 1] : null;
          }
        })
      );
    },

    /** Set the active diff tab */
    setActive(id: string | null): void {
      setState("activeId", id);
    },

    /** Clear all diff tabs for a repository */
    clearForRepo(repoPath: string): void {
      setState(
        produce((s) => {
          const idsToRemove = Object.values(s.tabs)
            .filter((tab) => tab.repoPath === repoPath)
            .map((tab) => tab.id);

          for (const id of idsToRemove) {
            delete s.tabs[id];
          }

          // If active tab was removed, clear active
          if (s.activeId && idsToRemove.includes(s.activeId)) {
            s.activeId = null;
          }
        })
      );
    },

    /** Clear all diff tabs */
    clearAll(): void {
      setState({ tabs: {}, activeId: null, counter: state.counter });
    },

    /** Get a tab by ID */
    get(id: string): DiffTabData | undefined {
      return state.tabs[id];
    },

    /** Get all tab IDs */
    getIds(): string[] {
      return Object.keys(state.tabs);
    },

    /** Get tabs for a specific repository */
    getForRepo(repoPath: string): DiffTabData[] {
      return Object.values(state.tabs).filter((tab) => tab.repoPath === repoPath);
    },

    /** Get active tab */
    getActive(): DiffTabData | undefined {
      return state.activeId ? state.tabs[state.activeId] : undefined;
    },

    /** Get count of tabs */
    getCount(): number {
      return Object.keys(state.tabs).length;
    },
  };

  return { state, ...actions };
}

export const diffTabsStore = createDiffTabsStore();
