import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { ActivityItem, ActivitySection, Disposable } from "../plugins/types";

/** Serializable subset of ActivityItem (no onClick function) */
type PersistedActivityItem = Omit<ActivityItem, "onClick">;

interface ActivityStoreState {
  items: ActivityItem[];
  sections: ActivitySection[];
}

/** Strip non-serializable fields before saving */
function toPersistedItems(items: ActivityItem[]): PersistedActivityItem[] {
  return items.map(({ onClick: _, ...rest }) => rest);
}

/** Persist activity items to Rust backend (fire-and-forget) */
function saveActivity(items: ActivityItem[]): void {
  invoke("save_activity", { items: toPersistedItems(items) }).catch((err) =>
    console.error("Failed to save activity:", err),
  );
}

function createActivityStore() {
  const [state, setState] = createStore<ActivityStoreState>({
    items: [],
    sections: [],
  });

  // -------------------------------------------------------------------------
  // Persistence
  // -------------------------------------------------------------------------

  async function hydrate(): Promise<void> {
    try {
      const loaded = await invoke<{ items?: PersistedActivityItem[] }>("load_activity");
      if (loaded?.items && Array.isArray(loaded.items)) {
        const migrated = loaded.items.map((item) => ({
          ...item,
          dismissed: item.dismissed ?? false,
        }));
        setState(
          produce((s) => {
            // Merge: existing (live) items take precedence over saved ones
            const liveIds = new Set(s.items.map((i) => i.id));
            for (const saved of migrated) {
              if (!liveIds.has(saved.id)) {
                s.items.push(saved);
              }
            }
          }),
        );
      }
    } catch (err) {
      console.debug("Failed to hydrate activity:", err);
    }
  }

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
          s.items[idx] = full;
        } else {
          s.items.push(full);
        }
      }),
    );
    saveActivity(state.items);
  }

  function removeItem(id: string): void {
    setState("items", (prev) => prev.filter((i) => i.id !== id));
    saveActivity(state.items);
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
    saveActivity(state.items);
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
    saveActivity(state.items);
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
    saveActivity(state.items);
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
    hydrate,
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
