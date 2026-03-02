import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { ActivityItem, ActivitySection, Disposable } from "../plugins/types";
import { appLogger } from "./appLogger";

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

/** Fire-and-forget persist to Rust backend */
function persistActivityNow(items: ActivityItem[]): void {
  invoke("save_activity", { items: toPersistedItems(items) }).catch((err) =>
    appLogger.error("store", "Failed to save activity", err),
  );
}

/** Debounced persist (coalesces rapid mutations) */
let saveActivityTimer: ReturnType<typeof setTimeout> | null = null;
function saveActivity(items: ActivityItem[]): void {
  if (saveActivityTimer) clearTimeout(saveActivityTimer);
  saveActivityTimer = setTimeout(() => {
    saveActivityTimer = null;
    persistActivityNow(items);
  }, 300);
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
      appLogger.debug("store", "Failed to hydrate activity", err);
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

  function getForSection(sectionId: string, repoPath?: string): ActivityItem[] {
    return state.items.filter((i) => {
      if (i.sectionId !== sectionId || i.dismissed) return false;
      if (repoPath !== undefined) return i.repoPath === repoPath;
      return true;
    });
  }

  function getLastItem(repoPath?: string): ActivityItem | null {
    let candidates = getActive();
    if (repoPath !== undefined) {
      candidates = candidates.filter((i) => i.repoPath === repoPath);
    }
    if (candidates.length === 0) return null;
    return candidates.reduce((latest, item) =>
      item.createdAt >= latest.createdAt ? item : latest,
    );
  }

  // -------------------------------------------------------------------------
  // Reset (for testing)
  // -------------------------------------------------------------------------

  /** Flush any pending debounced save immediately */
  function flushSave(): void {
    if (saveActivityTimer) {
      clearTimeout(saveActivityTimer);
      saveActivityTimer = null;
      persistActivityNow(state.items);
    }
  }

  function clearAll(): void {
    if (saveActivityTimer) {
      clearTimeout(saveActivityTimer);
      saveActivityTimer = null;
    }
    setState({ items: [], sections: [] });
    persistActivityNow([]);
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
    flushSave,
    clearAll,
  };
}

export const activityStore = createActivityStore();
