import { createStore, produce, type SetStoreFunction } from "solid-js/store";
import { readTabOrderForReload, readTabsForReload, trackTabOrderForReload, trackTabsForReload } from "./tabReload";

/**
 * Base constraint for tab data — every tab must have an id.
 */
export interface BaseTab {
	id: string;
	/** MCP ui action=tab identity; distinct ids must never deduplicate by file path. */
	mcpUiId?: string;
	/** When true, tab is visible across all branches (within the same repo if repoPath is set) */
	pinned?: boolean;
	/** MCP UI tabs may use pinning across repositories while retaining their opening repo. */
	pinAcrossRepos?: boolean;
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
 * Move sourceId immediately before or after targetId, in place. No-op on bad IDs.
 *
 * The single ordering primitive: every display order in the app (per-store `_order`,
 * the cross-kind free-mode order) splices through here, so a fix lands once.
 */
export function reorderIds(order: string[], sourceId: string, targetId: string, side: "before" | "after"): void {
	if (sourceId === targetId) return;
	const src = order.indexOf(sourceId);
	const tgt = order.indexOf(targetId);
	if (src === -1 || tgt === -1) return;
	order.splice(src, 1);
	const newTgt = order.indexOf(targetId);
	order.splice(side === "before" ? newTgt : newTgt + 1, 0, sourceId);
}

/**
 * Ids from `order` that are still visible, followed by visible `candidateIds` the
 * order has never seen — a tab opened after the last drag still shows up, at the end.
 */
export function orderedThenRemainder(
	order: readonly string[],
	candidateIds: Iterable<string>,
	isVisible: (id: string) => boolean,
): string[] {
	const inOrder = order.filter(isVisible);
	const seen = new Set(inOrder);
	const remainder = [...candidateIds].filter((id) => !seen.has(id) && isVisible(id));
	return [...inOrder, ...remainder];
}

/**
 * The one display order that spans every tab kind.
 *
 * A per-store `_order` cannot express it: a drag between a terminal and a diff moves
 * an id across store boundaries, and terminals keep their own order in
 * repositoriesStore (per repo, per branch). So the stores mirror their adds and
 * removes here, and the tab bar reads this list in the ordering modes that mix kinds.
 * Lives in tabManager because every tab store already imports it and it imports none.
 */
function createTabOrderingStore() {
	const [state, setState] = createStore<{ order: string[] }>({ order: readTabOrderForReload() });
	trackTabOrderForReload(() => state.order);

	return {
		state,

		/** Track a newly opened tab, at the end or right after `afterId`. */
		insert(id: string, afterId?: string): void {
			if (state.order.includes(id)) return;
			setState(
				produce((s) => {
					if (afterId) {
						const idx = s.order.indexOf(afterId);
						if (idx !== -1) {
							s.order.splice(idx + 1, 0, id);
							return;
						}
					}
					s.order.push(id);
				}),
			);
		},

		/** Append a restore batch without repeatedly scanning the growing order. */
		insertMany(ids: readonly string[]): void {
			const seen = new Set(state.order);
			const added = ids.filter((id) => {
				if (seen.has(id)) return false;
				seen.add(id);
				return true;
			});
			if (added.length) setState("order", (order) => [...order, ...added]);
		},

		/** Forget a closed tab. No-op for an unknown id. */
		remove(id: string): void {
			setState(
				produce((s) => {
					const idx = s.order.indexOf(id);
					if (idx !== -1) s.order.splice(idx, 1);
				}),
			);
		},

		/** Move sourceId immediately before or after targetId. No-op on bad IDs. */
		reorder(sourceId: string, targetId: string, side: "before" | "after"): void {
			if (sourceId === targetId) return;
			setState(
				produce((s) => {
					reorderIds(s.order, sourceId, targetId, side);
				}),
			);
		},

		/** The visible ids, in user order, with never-dragged ones appended. */
		getOrdered(visibleIds: Set<string>): string[] {
			return orderedThenRemainder(state.order, visibleIds, (id) => visibleIds.has(id));
		},

		clear(): void {
			setState("order", []);
		},
	};
}

export const tabOrderingStore = createTabOrderingStore();

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

/**
 * Deactivators for every pane-owning store, keyed by store name.
 *
 * TerminalArea renders terminals, diffs, markdown and editors as four independent
 * `For` lists, each marking its pane `active` from its OWN store's `activeId`. So
 * "only one pane is showing" is a cross-store invariant that no single store owned:
 * each call site had to remember to null the other three, and the ones that forgot
 * (MarkdownTab's Edit button) rendered the viewer and the editor on top of each
 * other. Registering here lets the *activating* store enforce it once.
 *
 * Lives in tabManager because all four stores already import it and it imports
 * none of them — putting the registry in any one store would make them cyclic.
 */
const paneDeactivators = new Map<string, () => void>();
// Restore a saved pane without importing desktop-only content stores into mobile.
const paneTabActivators = new Map<string, (id: string) => boolean>();

export function activateRegisteredPaneTab(storeName: string, id: string): boolean {
	return paneTabActivators.get(storeName)?.(id) ?? false;
}

/** Register a store that owns a pane, so activating another pane can deactivate it. */
export function registerPaneDeactivator(storeName: string, deactivate: () => void): void {
	paneDeactivators.set(storeName, deactivate);
}

/**
 * Make `storeName` the only store with an active tab.
 *
 * Call this from the paths that ACTIVATE a tab. Background/silent opens must not
 * call it — they deliberately leave the current pane showing.
 */
export function activatePaneExclusively(storeName: string): void {
	for (const [name, deactivate] of paneDeactivators) {
		if (name !== storeName) deactivate();
	}
}

export function createTabManager<T extends BaseTab>(storeName: string = "unknown") {
	const restored = readTabsForReload<T>(storeName);
	const [state, setState] = createStore<TabStoreState<T>>({
		tabs: restored?.tabs ?? ({} as Record<string, T>),
		activeId: null,
		counter: restored?.counter ?? 0,
		_order: restored?._order ?? [],
	});
	if (restored) tabOrderingStore.insertMany(restored._order);
	trackTabsForReload(storeName, state, restored?.activeId ?? null, (id) => {
		setState("activeId", id);
		activatePaneExclusively(storeName);
	});

	registerPaneDeactivator(storeName, () => setState("activeId", null));
	paneTabActivators.set(storeName, (id) => {
		if (!state.tabs[id]) return false;
		setState("activeId", id);
		activatePaneExclusively(storeName);
		return true;
	});

	return {
		state,

		/** Internal: expose setState for domain-specific mutations. */
		_setState: setState as SetStoreFunction<TabStoreState<T>>,

		/** Internal: add a tab to the store and set it active. Returns the tab id.
		 *  Activating makes this the ONLY pane showing — see activatePaneExclusively. */
		_addTab(tab: T): string {
			setState("tabs", tab.id, tab);
			setState("activeId", tab.id);
			setState("_order", (o) => [...o, tab.id]);
			tabOrderingStore.insert(tab.id);
			activatePaneExclusively(storeName);
			onTabAdded?.(tab.id, storeName);
			return tab.id;
		},

		/** Internal: add a tab without changing activeId (background/silent open). */
		_addTabBackground(tab: T): string {
			setState("tabs", tab.id, tab);
			setState("_order", (o) => [...o, tab.id]);
			tabOrderingStore.insert(tab.id);
			return tab.id;
		},

		/** Internal: append fresh restored tabs, preserving order and pane selection. */
		_addTabsBackground(tabs: readonly T[]): void {
			if (!tabs.length) return;
			for (const tab of tabs) setState("tabs", tab.id, tab);
			const ids = tabs.map((tab) => tab.id);
			setState("_order", (order) => [...order, ...ids]);
			tabOrderingStore.insertMany(ids);
		},

		/** Internal: get next auto-increment id with the given prefix (e.g. "diff-1"). */
		_nextId(prefix: string): string {
			setState("counter", (c) => c + 1);
			return `${prefix}-${state.counter}`;
		},

		/** Remove a tab. Sets activeId to null when removing the active tab — the caller
		 *  is responsible for selecting a visibility-aware replacement (same contract as
		 *  terminalsStore.remove). The store can't filter by branch/repo scope here, so
		 *  auto-promoting an arbitrary remaining tab could activate one hidden from the
		 *  tab bar — a ghost full-screen panel with no visible tab. */
		remove(id: string): void {
			tabOrderingStore.remove(id);
			setState(
				produce((s) => {
					delete s.tabs[id];
					s._order = s._order.filter((oid) => oid !== id);
					if (s.activeId === id) {
						s.activeId = null;
					}
				}),
			);
		},

		/** Activate a tab, or clear the active one with `null`.
		 *  Activating deactivates every other pane store: TerminalArea marks each pane
		 *  from its own store's activeId, so two stores holding an activeId render two
		 *  panes on top of each other. `null` only clears this store. */
		setActive(id: string | null): void {
			setState("activeId", id);
			if (id !== null) activatePaneExclusively(storeName);
		},

		clearAll(): void {
			for (const id of Object.keys(state.tabs)) tabOrderingStore.remove(id);
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
		setPinned(id: string, pinned: boolean, pinAcrossRepos = false): void {
			if (state.tabs[id]) {
				setState(
					"tabs",
					produce((tabs: Record<string, T>) => {
						if (tabs[id]) {
							tabs[id].pinned = pinned;
							if (pinAcrossRepos) tabs[id].pinAcrossRepos = true;
						}
					}),
				);
			}
		},

		/** Get tab IDs visible for the given branch key (pinned + matching branch + unscoped).
		 *  Respects user-defined drag order (_order array) when present. */
		getVisibleIds(currentBranchKey: string | null, includeActive = true): string[] {
			const isVisible = (id: string): boolean => {
				const tab = state.tabs[id];
				if (!tab) return false;
				// The pane draws whatever activeId points at, so hiding the active tab
				// leaves content with no tab to name it, switch from or close — the same
				// ghost panel `remove()` refuses to create by promotion. `_addTab`
				// activates unconditionally, which is how opening a path from ANOTHER
				// repo (an absolute path an agent printed) produced one: the tab was
				// correctly filed under the repo that owns the file, and the repo gate
				// below then hid it. Exempt only the active tab — exempting foreign
				// tabs generally would leak every repo's tabs into every tab bar.
				if (includeActive && id === state.activeId) return true;
				if (tab.pinned && tab.pinAcrossRepos) return true;
				if (tab.repoPath) {
					if (!currentBranchKey?.startsWith(tab.repoPath + "|")) return false;
				}
				if (tab.pinned) return true;
				if (!tab.branchKey) return true;
				return tab.branchKey === currentBranchKey;
			};
			// Ordered tabs first (those tracked in _order), then any remainder
			return orderedThenRemainder(state._order, Object.keys(state.tabs), isVisible);
		},

		/** Move sourceId immediately before or after targetId in the display order. No-op on bad IDs. */
		reorderByIds(sourceId: string, targetId: string, side: "before" | "after"): void {
			if (sourceId === targetId) return;
			setState(
				produce((s) => {
					reorderIds(s._order, sourceId, targetId, side);
				}),
			);
		},

		/** Clear tabs matching a predicate */
		_clearWhere(predicate: (tab: T) => boolean): void {
			const idsToRemove = Object.values(state.tabs)
				.filter((tab) => predicate(tab as T))
				.map((tab) => (tab as T).id);
			for (const id of idsToRemove) tabOrderingStore.remove(id);
			setState(
				produce((s) => {
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
