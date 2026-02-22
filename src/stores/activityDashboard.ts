import { createStore } from "solid-js/store";

interface ActivityDashboardState {
  isOpen: boolean;
}

function createActivityDashboardStore() {
  const [state, setState] = createStore<ActivityDashboardState>({
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

export const activityDashboardStore = createActivityDashboardStore();
