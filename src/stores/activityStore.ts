import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { ActivityItem, ActivitySection, Disposable } from "../plugins/types";
import { createConfigDeltaWriter } from "../utils/configDeltaWriter";
import { appLogger } from "./appLogger";

/** Serializable subset of ActivityItem (no onClick function) */
type PersistedActivityItem = Omit<ActivityItem, "onClick">;
const activityWriter = createConfigDeltaWriter<PersistedActivityItem[]>("save_activity", "items");

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
	activityWriter.save(toPersistedItems(items)).catch((err) => appLogger.error("store", "Failed to save activity", err));
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
			const loaded = await invoke<PersistedActivityItem[] | { items?: PersistedActivityItem[] } | null>(
				"load_activity",
			);
			activityWriter.loaded(Array.isArray(loaded) ? loaded : (loaded?.items ?? []));
			const items = Array.isArray(loaded) ? loaded : loaded?.items;
			if (Array.isArray(items)) {
				const migrated = items.map((item) => ({
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
		return state.sections.toSorted((a, b) => a.priority - b.priority);
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
				// Splice the overflow off the front rather than replacing the array,
				// for the same reason as removeItem.
				if (s.items.length > 500) {
					s.items.splice(0, s.items.length - 500);
				}
			}),
		);
		saveActivity(state.items);
	}

	function removeItem(id: string): void {
		// Splice, not filter-into-a-new-array: handing the store a fresh array
		// invalidates every consumer of the list, including the rows that were not
		// touched. A splice only notifies what actually moved.
		setState(
			produce((s) => {
				const idx = s.items.findIndex((i) => i.id === id);
				if (idx >= 0) s.items.splice(idx, 1);
			}),
		);
		saveActivity(state.items);
	}

	function updateItem(id: string, updates: Partial<Omit<ActivityItem, "id" | "pluginId" | "createdAt">>): void {
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

	/** Is this item one the popover would show for that section and repo? */
	function isVisibleIn(item: ActivityItem, sectionId: string, repoPath?: string): boolean {
		if (item.sectionId !== sectionId || item.dismissed) return false;
		// When filtering by repo: include items that match OR have no repoPath set
		if (repoPath !== undefined && item.repoPath) return item.repoPath === repoPath;
		return true;
	}

	/** Newest first — the bell reads top-down, so the latest message must lead. */
	function getForSection(sectionId: string, repoPath?: string): ActivityItem[] {
		return state.items.filter((i) => isVisibleIn(i, sectionId, repoPath)).sort((a, b) => b.createdAt - a.createdAt);
	}

	/**
	 * How many items the popover would show across these sections. The badge wants
	 * one number, so it walks the items once and never sorts — asking each section
	 * for its list instead cost a scan and a sort per section, both discarded for
	 * a `.length`.
	 */
	function countActiveInSections(sectionIds: string[], repoPath?: string): number {
		if (sectionIds.length === 0) return 0;
		const wanted = new Set(sectionIds);
		let count = 0;
		for (const item of state.items) {
			if (wanted.has(item.sectionId) && isVisibleIn(item, item.sectionId, repoPath)) count++;
		}
		return count;
	}

	function getLastItem(repoPath?: string): ActivityItem | null {
		let candidates = getActive();
		if (repoPath !== undefined) {
			candidates = candidates.filter((i) => !i.repoPath || i.repoPath === repoPath);
		}
		// Exclude non-dismissible items — these are static plugin launchers
		// (e.g. "Open Kanban board", "View repository overview") not real notifications.
		candidates = candidates.filter((i) => i.dismissible);
		if (candidates.length === 0) return null;
		return candidates.reduce((latest, item) => (item.createdAt >= latest.createdAt ? item : latest));
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
		countActiveInSections,
		getLastItem,
		flushSave,
		clearAll,
		_testCancelPendingSave(): void {
			if (saveActivityTimer) {
				clearTimeout(saveActivityTimer);
				saveActivityTimer = null;
			}
		},
	};
}

export const activityStore = createActivityStore();
