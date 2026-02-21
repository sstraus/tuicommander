import { createStore, produce } from "solid-js/store";
import type { ActivityItem, ActivitySection, Disposable } from "../plugins/types";

interface ActivityStoreState {
  items: ActivityItem[];
  sections: ActivitySection[];
}

function createActivityStore() {
  const [state, setState] = createStore<ActivityStoreState>({
    items: [],
    sections: [],
  });

  // -------------------------------------------------------------------------
  // Section registration
  // -------------------------------------------------------------------------

  function registerSection(section: ActivitySection): Disposable {
    setState(
      produce((s) => {
        // Replace if same id exists, otherwise append
        const idx = s.sections.findIndex((x) => x.id === section.id);
        if (idx >= 0) {
          s.sections[idx] = section;
        } else {
          s.sections.push(section);
        }
      }),
    );
    return {
      dispose() {
        setState("sections", (prev) => prev.filter((s) => s.id !== section.id));
      },
    };
  }

  function getSections(): ActivitySection[] {
    return [...state.sections].sort((a, b) => a.priority - b.priority);
  }

  // -------------------------------------------------------------------------
  // Item CRUD
  // -------------------------------------------------------------------------

  function addItem(item: Omit<ActivityItem, "createdAt">): void {
    const full: ActivityItem = { ...item, createdAt: Date.now() };
    setState(
      produce((s) => {
        const idx = s.items.findIndex((i) => i.id === full.id);
        if (idx >= 0) {
          // Replace existing (same id), preserve original createdAt so
          // ordering is stable — but caller may legitimately want a refresh.
          // We use the new timestamp to reflect the update.
          s.items[idx] = full;
        } else {
          s.items.push(full);
        }
      }),
    );
  }

  function removeItem(id: string): void {
    setState("items", (prev) => prev.filter((i) => i.id !== id));
  }

  function updateItem(
    id: string,
    updates: Partial<Omit<ActivityItem, "id" | "pluginId" | "createdAt">>,
  ): void {
    setState(
      produce((s) => {
        const item = s.items.find((i) => i.id === id);
        if (item) Object.assign(item, updates);
      }),
    );
  }

  // -------------------------------------------------------------------------
  // Dismiss
  // -------------------------------------------------------------------------

  function dismissItem(id: string): void {
    setState(
      produce((s) => {
        const item = s.items.find((i) => i.id === id);
        if (item) item.dismissed = true;
      }),
    );
  }

  function dismissSection(sectionId: string): void {
    setState(
      produce((s) => {
        for (const item of s.items) {
          if (item.sectionId === sectionId && !item.dismissed) {
            item.dismissed = true;
          }
        }
      }),
    );
  }

  // -------------------------------------------------------------------------
  // Queries
  // -------------------------------------------------------------------------

  function getActive(): ActivityItem[] {
    return state.items.filter((i) => !i.dismissed);
  }

  function getForSection(sectionId: string): ActivityItem[] {
    return state.items.filter((i) => i.sectionId === sectionId && !i.dismissed);
  }

  function getLastItem(): ActivityItem | null {
    const active = getActive();
    if (active.length === 0) return null;
    return active.reduce((latest, item) =>
      item.createdAt >= latest.createdAt ? item : latest,
    );
  }

  // -------------------------------------------------------------------------
  // Reset (for testing)
  // -------------------------------------------------------------------------

  function clearAll(): void {
    setState({ items: [], sections: [] });
  }

  return {
    state,
    registerSection,
    getSections,
    addItem,
    removeItem,
    updateItem,
    dismissItem,
    dismissSection,
    getActive,
    getForSection,
    getLastItem,
    clearAll,
  };
}

export const activityStore = createActivityStore();
