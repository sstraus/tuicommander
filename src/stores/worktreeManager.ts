import { createStore } from "solid-js/store";

interface WorktreeManagerState {
  isOpen: boolean;
  selectedIds: Set<string>;
  repoFilter: string | null;
  textFilter: string;
}

function createWorktreeManagerStore() {
  const [state, setState] = createStore<WorktreeManagerState>({
    isOpen: false,
    selectedIds: new Set<string>(),
    repoFilter: null,
    textFilter: "",
  });

  return {
    state,

    open(): void {
      setState("isOpen", true);
    },

    close(): void {
      setState({
        isOpen: false,
        selectedIds: new Set<string>(),
        repoFilter: null,
        textFilter: "",
      });
    },

    toggle(): void {
      if (state.isOpen) {
        this.close();
      } else {
        this.open();
      }
    },

    toggleSelect(id: string): void {
      const next = new Set(state.selectedIds);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      setState("selectedIds", next);
    },

    selectAll(ids: string[]): void {
      setState("selectedIds", new Set(ids));
    },

    clearSelection(): void {
      setState("selectedIds", new Set<string>());
    },

    setRepoFilter(repoPath: string | null): void {
      setState("repoFilter", repoPath);
    },

    setTextFilter(text: string): void {
      setState("textFilter", text);
    },
  };
}

export const worktreeManagerStore = createWorktreeManagerStore();
