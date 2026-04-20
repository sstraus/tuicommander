import { createStore, produce, type SetStoreFunction } from "solid-js/store";

/**
 * Base constraint for tab data — every tab must have an id.
 */
export interface BaseTab {
  id: string;
  /** When true, tab is visible across all branches (within the same repo if repoPath is set) */
  pinned?: boolean;
  /** Scope key: "repoPath|branchName" — tab only visible in this branch unless pinned */
  branchKey?: string;
  /** Repo scope — when set, tab is only visible when the active branchKey belongs to this repo */
  repoPath?: string;
  /** Per-tab font size override in pixels (used by zoom actions) */
  fontSize?: number;
}

/** Build a branch scope key for tab filtering */
export function makeBranchKey(repoPath: string, branchName: string): string {
  return `${repoPath}|${branchName}`;
}

/**
 * Internal store state shape for all tab managers.
 */
export interface TabStoreState<T extends BaseTab> {
  tabs: Record<string, T>;
  activeId: string | null;
  counter: number;
  /** Explicit display order for drag-reorder. Populated by _addTab; reorderByIds splices it. */
  _order: string[];
}

/**
 * Factory that creates a tab manager with shared CRUD logic.
 *
 * Domain stores call this, then layer on their own add() / custom methods.
 */
/** Global hook called after any tab is added. Set by paneTabAssign to avoid circular deps. */
export let onTabAdded: ((tabId: string, storeName: string) => void) | null = null;

/** Register the global onTabAdded hook (called once during app init) */
export function setOnTabAdded(hook: typeof onTabAdded): void {
  onTabAdded = hook;
}

export function createTabManager<T extends BaseTab>(storeName: string = "unknown") {
  const [state, setState] = createStore<TabStoreState<T>>({
    tabs: {} as Record<string, T>,
    activeId: null,
    counter: 0,
    _order: [],
  });

  return {
    state,

    /** Internal: expose setState for domain-specific mutations. */
    _setState: setState as SetStoreFunction<TabStoreState<T>>,

    /** Internal: add a tab to the store and set it active. Returns the tab id. */
    _addTab(tab: T): string {
      setState("tabs", tab.id, tab);
      setState("activeId", tab.id);
      setState("_order", (o) => [...o, tab.id]);
      onTabAdded?.(tab.id, storeName);
      return tab.id;
    },

    /** Internal: get next auto-increment id with the given prefix (e.g. "diff-1"). */
    _nextId(prefix: string): string {
      setState("counter", (c) => c + 1);
      return `${prefix}-${state.counter}`;
    },

    remove(id: string): void {
      setState(
        produce((s) => {
          delete s.tabs[id];
          s._order = s._order.filter((oid) => oid !== id);
          if (s.activeId === id) {
            const remaining = Object.keys(s.tabs);
            s.activeId = remaining.length > 0 ? remaining[remaining.length - 1] : null;
          }
        }),
      );
    },

    setActive(id: string | null): void {
      setState("activeId", id);
    },

    clearAll(): void {
      setState({ tabs: {} as Record<string, T>, activeId: null, counter: state.counter, _order: [] });
    },

    get(id: string): T | undefined {
      return state.tabs[id];
    },

    getIds(): string[] {
      return Object.keys(state.tabs);
    },

    getActive(): T | undefined {
      return state.activeId ? state.tabs[state.activeId] : undefined;
    },

    getCount(): number {
      return Object.keys(state.tabs).length;
    },

    /** Toggle pinned state for a tab */
    setPinned(id: string, pinned: boolean): void {
      if (state.tabs[id]) {
        setState("tabs", produce((tabs: Record<string, T>) => {
          if (tabs[id]) tabs[id].pinned = pinned;
        }));
      }
    },

    /** Get tab IDs visible for the given branch key (pinned + matching branch + unscoped).
     *  Respects user-defined drag order (_order array) when present. */
    getVisibleIds(currentBranchKey: string | null): string[] {
      const isVisible = (id: string): boolean => {
        const tab = state.tabs[id];
        if (!tab) return false;
        if (tab.repoPath) {
          if (!currentBranchKey || !currentBranchKey.startsWith(tab.repoPath + "|")) return false;
        }
        if (tab.pinned) return true;
        if (!tab.branchKey) return true;
        return tab.branchKey === currentBranchKey;
      };
      // Ordered tabs first (those tracked in _order), then any remainder
      const inOrder = state._order.filter(isVisible);
      const inOrderSet = new Set(inOrder);
      const remainder = Object.keys(state.tabs).filter((id) => !inOrderSet.has(id) && isVisible(id));
      return [...inOrder, ...remainder];
    },

    /** Move sourceId immediately before or after targetId in the display order. No-op on bad IDs. */
    reorderByIds(sourceId: string, targetId: string, side: "before" | "after"): void {
      if (sourceId === targetId) return;
      setState(
        produce((s) => {
          const src = s._order.indexOf(sourceId);
          const tgt = s._order.indexOf(targetId);
          if (src === -1 || tgt === -1) return;
          s._order.splice(src, 1);
          const newTgt = s._order.indexOf(targetId);
          s._order.splice(side === "before" ? newTgt : newTgt + 1, 0, sourceId);
        }),
      );
    },

    /** Clear tabs matching a predicate */
    _clearWhere(predicate: (tab: T) => boolean): void {
      setState(
        produce((s) => {
          const idsToRemove = Object.values(s.tabs)
            .filter((tab) => predicate(tab as T))
            .map((tab) => (tab as T).id);

          for (const id of idsToRemove) {
            delete s.tabs[id];
          }

          if (s.activeId && idsToRemove.includes(s.activeId)) {
            s.activeId = null;
          }
        }),
      );
    },
  };
}
