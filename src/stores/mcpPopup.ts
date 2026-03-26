import { createStore } from "solid-js/store";

interface McpPopupState {
  isOpen: boolean;
}

function createMcpPopupStore() {
  const [state, setState] = createStore<McpPopupState>({
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
      setState("isOpen", !state.isOpen);
    },
  };
}

export const mcpPopupStore = createMcpPopupStore();
