import { createStore, produce } from "solid-js/store";

/** Editor tab data */
export interface EditorTabData {
  id: string;
  repoPath: string;
  filePath: string;
  fileName: string; // Display name (basename of filePath)
  isDirty: boolean;
}

/** Editor tabs store state */
interface EditorTabsStoreState {
  tabs: Record<string, EditorTabData>;
  activeId: string | null;
  counter: number;
}

/** Create the editor tabs store */
function createEditorTabsStore() {
  const [state, setState] = createStore<EditorTabsStoreState>({
    tabs: {},
    activeId: null,
    counter: 0,
  });

  const actions = {
    /** Add a new editor tab (or activate existing if same file already open) */
    add(repoPath: string, filePath: string): string {
      // Check if tab for this file already exists
      const existing = Object.values(state.tabs).find(
        (tab) => tab.repoPath === repoPath && tab.filePath === filePath,
      );
      if (existing) {
        setState("activeId", existing.id);
        return existing.id;
      }

      // Create new tab
      const id = `edit-${state.counter + 1}`;
      const fileName = filePath.split("/").pop() || filePath;
      setState("counter", (c) => c + 1);
      setState("tabs", id, { id, repoPath, filePath, fileName, isDirty: false });
      setState("activeId", id);
      return id;
    },

    /** Remove an editor tab */
    remove(id: string): void {
      setState(
        produce((s) => {
          delete s.tabs[id];
          if (s.activeId === id) {
            const remaining = Object.keys(s.tabs);
            s.activeId = remaining.length > 0 ? remaining[remaining.length - 1] : null;
          }
        }),
      );
    },

    /** Set the active editor tab */
    setActive(id: string | null): void {
      setState("activeId", id);
    },

    /** Mark a tab as dirty or clean */
    setDirty(id: string, isDirty: boolean): void {
      if (state.tabs[id]) {
        setState("tabs", id, "isDirty", isDirty);
      }
    },

    /** Clear all editor tabs for a repository */
    clearForRepo(repoPath: string): void {
      setState(
        produce((s) => {
          const idsToRemove = Object.values(s.tabs)
            .filter((tab) => tab.repoPath === repoPath)
            .map((tab) => tab.id);

          for (const id of idsToRemove) {
            delete s.tabs[id];
          }

          if (s.activeId && idsToRemove.includes(s.activeId)) {
            s.activeId = null;
          }
        }),
      );
    },

    /** Clear all editor tabs */
    clearAll(): void {
      setState({ tabs: {}, activeId: null, counter: state.counter });
    },

    /** Get a tab by ID */
    get(id: string): EditorTabData | undefined {
      return state.tabs[id];
    },

    /** Get all tab IDs */
    getIds(): string[] {
      return Object.keys(state.tabs);
    },

    /** Get active tab */
    getActive(): EditorTabData | undefined {
      return state.activeId ? state.tabs[state.activeId] : undefined;
    },

    /** Get count of tabs */
    getCount(): number {
      return Object.keys(state.tabs).length;
    },
  };

  return { state, ...actions };
}

export const editorTabsStore = createEditorTabsStore();
