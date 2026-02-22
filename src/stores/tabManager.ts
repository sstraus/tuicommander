import { createStore, produce, type SetStoreFunction } from "solid-js/store";

/**
 * Base constraint for tab data — every tab must have an id.
 */
export interface BaseTab {
  id: string;
}

/**
 * Internal store state shape for all tab managers.
 */
export interface TabStoreState<T extends BaseTab> {
  tabs: Record<string, T>;
  activeId: string | null;
  counter: number;
}

/**
 * Factory that creates a tab manager with shared CRUD logic.
 *
 * Domain stores call this, then layer on their own add() / custom methods.
 */
export function createTabManager<T extends BaseTab>() {
  const [state, setState] = createStore<TabStoreState<T>>({
    tabs: {} as Record<string, T>,
    activeId: null,
    counter: 0,
  });

  return {
    state,

    /** Internal: expose setState for domain-specific mutations. */
    _setState: setState as SetStoreFunction<TabStoreState<T>>,

    /** Internal: add a tab to the store and set it active. Returns the tab id. */
    _addTab(tab: T): string {
      setState("tabs", tab.id, tab);
      setState("activeId", tab.id);
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
      setState({ tabs: {} as Record<string, T>, activeId: null, counter: state.counter });
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
