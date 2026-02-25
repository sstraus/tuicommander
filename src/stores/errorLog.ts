import { createStore } from "solid-js/store";

interface ErrorLogState {
  isOpen: boolean;
}

function createErrorLogStore() {
  const [state, setState] = createStore<ErrorLogState>({
    isOpen: false,
  });

  return {
    state,

    open(): void {
      setState("isOpen", true);
    },

    close(): void {
      setState("isOpen", false);
    },

    toggle(): void {
      setState("isOpen", (v) => !v);
    },
  };
}

export const errorLogStore = createErrorLogStore();
