import { createStore } from "solid-js/store";

interface BranchSwitcherState {
  isOpen: boolean;
  query: string;
}

function createBranchSwitcherStore() {
  const [state, setState] = createStore<BranchSwitcherState>({
    isOpen: false,
    query: "",
  });

  return {
    state,

    open(): void {
      setState("query", "");
      setState("isOpen", true);
    },

    close(): void {
      setState("isOpen", false);
      setState("query", "");
    },

    toggle(): void {
      if (state.isOpen) {
        this.close();
      } else {
        this.open();
      }
    },

    setQuery(query: string): void {
      setState("query", query);
    },
  };
}

export const branchSwitcherStore = createBranchSwitcherStore();
