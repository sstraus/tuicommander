import { createStore } from "solid-js/store";

interface SmartPromptsDropdownState {
  isOpen: boolean;
}

function createSmartPromptsDropdownStore() {
  const [state, setState] = createStore<SmartPromptsDropdownState>({
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

export const smartPromptsDropdownStore = createSmartPromptsDropdownStore();
