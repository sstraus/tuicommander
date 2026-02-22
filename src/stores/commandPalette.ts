import { createStore } from "solid-js/store";

const RECENT_ACTIONS_KEY = "tui-commander-recent-actions";
const MAX_RECENT = 10;

interface CommandPaletteState {
  isOpen: boolean;
  query: string;
  recentActions: string[];
}

function loadRecentActions(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_ACTIONS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.slice(0, MAX_RECENT) : [];
  } catch {
    return [];
  }
}

function createCommandPaletteStore() {
  const [state, setState] = createStore<CommandPaletteState>({
    isOpen: false,
    query: "",
    recentActions: loadRecentActions(),
  });

  return {
    state,

    open(): void {
      setState("isOpen", true);
      setState("query", "");
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

    recordUsage(actionId: string): void {
      const updated = [actionId, ...state.recentActions.filter((id) => id !== actionId)].slice(0, MAX_RECENT);
      setState("recentActions", updated);
      try {
        localStorage.setItem(RECENT_ACTIONS_KEY, JSON.stringify(updated));
      } catch {
        // localStorage full — ignore
      }
    },
  };
}

export const commandPaletteStore = createCommandPaletteStore();
