import { createSignal } from "solid-js";
import { allLeafIds, type PaneGroup, type PaneLayoutState, paneLayoutStore, removeLeaf } from "./paneLayout";
import { savedPaneLayouts } from "./savedPaneLayouts";
import { terminalsStore } from "./terminals";

/** Scope key for the hand-promoted workspace — the one this store has always
 *  had. Repo paths are the other keys (#e767). */
export const MANUAL_SCOPE = "__manual__";

let globalGroupCounter = 0;
function nextGlobalGroupId(): string {
	return `gw${++globalGroupCounter}`;
}

/** Add a terminal as a tab to the active group. Creates the first group if layout is empty. */
function addTerminalToLayout(current: PaneLayoutState | null, termId: string): PaneLayoutState {
	// First terminal — create single group
	if (!current?.root) {
		const groupId = nextGlobalGroupId();
		const newGroup: PaneGroup = {
			id: groupId,
			tabs: [{ id: termId, type: "terminal" }],
			activeTabId: termId,
		};
		return {
			root: { type: "leaf", id: groupId },
			groups: { [groupId]: newGroup },
			activeGroupId: groupId,
		};
	}

	// Add as tab to active group (or first group as fallback)
	const targetGroupId = current.activeGroupId ?? Object.keys(current.groups)[0];
	if (!targetGroupId || !current.groups[targetGroupId]) return current;

	const group = current.groups[targetGroupId];
	return {
		...current,
		groups: {
			...current.groups,
			[targetGroupId]: {
				...group,
				tabs: [...group.tabs, { id: termId, type: "terminal" }],
				activeTabId: termId,
			},
		},
	};
}

/** Remove a terminal tab from its group. Returns null if the layout is now empty. */
function removeTerminalFromLayout(current: PaneLayoutState, termId: string): PaneLayoutState | null {
	// Find the group containing this terminal
	let targetGroupId: string | null = null;
	for (const [gid, group] of Object.entries(current.groups)) {
		if (group.tabs.some((t) => t.id === termId)) {
			targetGroupId = gid;
			break;
		}
	}
	if (!targetGroupId || !current.root) return current;

	const group = current.groups[targetGroupId];
	const remainingTabs = group.tabs.filter((t) => t.id !== termId);

	// Group still has tabs — just remove the tab
	if (remainingTabs.length > 0) {
		const newActiveTabId = group.activeTabId === termId ? remainingTabs.at(-1)!.id : group.activeTabId;
		return {
			...current,
			groups: {
				...current.groups,
				[targetGroupId]: {
					...group,
					tabs: remainingTabs,
					activeTabId: newActiveTabId,
				},
			},
		};
	}

	// Group is empty — remove the leaf from the tree
	const newRoot = removeLeaf(current.root, targetGroupId);
	if (!newRoot) return null;

	const newGroups = { ...current.groups };
	delete newGroups[targetGroupId];

	return {
		root: newRoot,
		groups: newGroups,
		activeGroupId:
			current.activeGroupId === targetGroupId
				? newRoot.type === "leaf"
					? newRoot.id
					: (allLeafIds(newRoot)[0] ?? null)
				: current.activeGroupId,
	};
}

function createGlobalWorkspaceStore() {
	const [isActive, setIsActive] = createSignal(false);
	const [promotedVersion, setPromotedVersion] = createSignal(0);

	/** One workspace per scope. `__manual__` is the hand-promoted workspace this
	 *  store has always been; every other key is a repo path whose worktrees are
	 *  auto-consolidated (#e767).
	 *
	 *  Keyed rather than exclusive because Boss picked "one layout per repo":
	 *  turning consolidation on for repo B must not cost repo A its arrangement,
	 *  and neither may disturb the manual workspace, which is a separate feature
	 *  that only shares this machinery. */
	interface Workspace {
		layout: PaneLayoutState | null;
		// Plain JS set — not in SolidJS store to avoid proxy issues
		promoted: Set<string>;
	}
	const workspaces = new Map<string, Workspace>();
	let scope: string = MANUAL_SCOPE;

	function workspace(key: string = scope): Workspace {
		let ws = workspaces.get(key);
		if (!ws) {
			ws = { layout: null, promoted: new Set() };
			workspaces.set(key, ws);
		}
		return ws;
	}

	// Saved repo layout key for restore on auto-deactivation
	let savedRepoLayoutKey: string | null = null;

	function bumpPromoted(): void {
		setPromotedVersion((v) => v + 1);
	}

	/** Sync the background layout to paneLayoutStore if global workspace is active */
	function syncToPaneStore(): void {
		const current = workspace().layout;
		if (isActive() && current) {
			paneLayoutStore.restore(current);
		}
	}

	/** Restore the repo's pane layout and clear the saved key */
	function restoreRepoLayout(): void {
		const key = savedRepoLayoutKey;
		savedRepoLayoutKey = null;
		if (key) {
			const saved = savedPaneLayouts.get(key);
			if (saved) {
				paneLayoutStore.restore(saved);
			} else {
				paneLayoutStore.reset();
			}
		} else {
			paneLayoutStore.reset();
		}
	}

	/** Auto-deactivate when no promoted terminals remain */
	function autoDeactivate(): void {
		workspace().layout = null;
		setIsActive(false);
		restoreRepoLayout();
	}

	return {
		/** Whether global workspace is currently the active view */
		isActive,

		/**
		 * Switch to global workspace view.
		 * @param repoLayoutKey — savedPaneLayouts key for the current repo+branch (optional)
		 */
		activate(repoLayoutKey?: string): void {
			if (isActive()) return;

			// Save current repo layout (single-pane and split both count)
			if (repoLayoutKey) {
				const current = paneLayoutStore.serialize();
				if (current.root) savedPaneLayouts.set(repoLayoutKey, current);
				savedRepoLayoutKey = repoLayoutKey;
			}

			setIsActive(true);

			// Restore global layout (or reset if none)
			const current = workspace().layout;
			if (current) {
				paneLayoutStore.restore(current);
				// Restore focus to a promoted terminal too. TerminalArea gates pane
				// visibility on terminalsStore.activeId, so without this the workspace
				// shows no terminal until the user clicks a tab — e.g. after a repo
				// round-trip, which reset activeId to a repo terminal. Mirrors
				// handleBranchSelect, which setActive()s after restoring a layout.
				const activeGroup = current.activeGroupId ? current.groups[current.activeGroupId] : undefined;
				const activeTab = activeGroup?.activeTabId;
				const members = workspace().promoted;
				const target = activeTab && members.has(activeTab) ? activeTab : [...members][0];
				if (target) terminalsStore.setActive(target);
			} else {
				paneLayoutStore.reset();
			}
		},

		/**
		 * Switch back to repo view.
		 * @param repoLayoutKey — savedPaneLayouts key for the repo+branch to restore (optional)
		 */
		deactivate(repoLayoutKey?: string): void {
			if (!isActive()) return;

			// Save global layout (serialize any non-empty layout, not just split)
			const serialized = paneLayoutStore.serialize();
			workspace().layout = serialized.root ? serialized : null;

			setIsActive(false);

			// Use explicit key if given, otherwise fall back to the key saved on activate
			if (repoLayoutKey) {
				savedRepoLayoutKey = repoLayoutKey;
			}
			restoreRepoLayout();
		},

		/**
		 * Promote a terminal to the global workspace.
		 * Adds it as a tab to the active group (no auto-split).
		 *
		 * `scopeKey` defaults to whatever scope is current, which is right for a
		 * user clicking promote — they mean the workspace in front of them. A
		 * caller promoting on the user's behalf must name the scope instead:
		 * `useWorktreeConsolidation` swaps the current scope to a repo path when
		 * consolidation is on, so an unnamed promote would land a tab in a
		 * different bucket depending on which repo was active — the exact
		 * ambient-state dependency that parking an unowned tab here exists to
		 * remove. `assignSessionToRepoBranch` passes MANUAL_SCOPE for that reason.
		 */
		promote(termId: string, scopeKey: string = scope): boolean {
			const ws = workspace(scopeKey);
			if (ws.promoted.has(termId)) return true;

			const updated = addTerminalToLayout(ws.layout, termId);

			ws.promoted.add(termId);
			ws.layout = updated;
			bumpPromoted();
			syncToPaneStore();
			return true;
		},

		/** Remove a terminal from the global workspace. Auto-deactivates when empty. */
		unpromote(termId: string): void {
			const ws = workspace();
			if (!ws.promoted.has(termId)) return;
			ws.promoted.delete(termId);

			if (ws.layout) {
				ws.layout = removeTerminalFromLayout(ws.layout, termId);
			}

			bumpPromoted();

			if (isActive() && ws.promoted.size === 0) {
				autoDeactivate();
			} else {
				syncToPaneStore();
			}
		},

		/** Check if a terminal is promoted */
		isPromoted(termId: string): boolean {
			promotedVersion(); // subscribe
			return workspace().promoted.has(termId);
		},

		/** Toggle promote/unpromote a terminal */
		togglePromote(termId: string): void {
			if (workspace().promoted.has(termId)) {
				this.unpromote(termId);
			} else {
				this.promote(termId);
			}
		},

		/** Get all promoted terminal IDs */
		getPromotedIds(): string[] {
			promotedVersion(); // subscribe
			return [...workspace().promoted];
		},

		/** Whether any terminals are promoted */
		hasPromoted(): boolean {
			promotedVersion(); // subscribe
			return workspace().promoted.size > 0;
		},

		/** Handle terminal removal — auto-unpromote and auto-deactivate if needed */
		onTerminalRemoved(termId: string): void {
			// A closed terminal has to leave EVERY workspace that holds it: an
			// auto-consolidated repo scope is usually not the visible one, and a
			// pane pointing at a dead terminal survives the next scope switch.
			let touched = false;
			for (const ws of workspaces.values()) {
				if (!ws.promoted.delete(termId)) continue;
				touched = true;
				if (ws.layout) {
					ws.layout = removeTerminalFromLayout(ws.layout, termId);
				}
			}
			if (!touched) return;

			bumpPromoted();
			if (isActive()) {
				if (workspace().promoted.size === 0) {
					autoDeactivate();
				} else {
					syncToPaneStore();
				}
			}
		},

		/** Get the global workspace layout */
		getLayout(): PaneLayoutState | null {
			return workspace().layout;
		},

		/** Set the global workspace layout */
		setLayout(newLayout: PaneLayoutState | null): void {
			workspace().layout = newLayout ? structuredClone(newLayout) : null;
		},

		/** Which workspace the promote/layout methods currently address. */
		getScope(): string {
			promotedVersion(); // subscribe
			return scope;
		},

		/** Point the store at another workspace, swapping the visible layout when
		 *  one is on screen. */
		setScope(key: string): void {
			if (key === scope) return;
			if (isActive()) {
				const serialized = paneLayoutStore.serialize();
				workspace().layout = serialized.root ? serialized : null;
			}
			scope = key;
			bumpPromoted();
			syncToPaneStore();
		},

		/** Members of a scope without switching to it. */
		getScopeMembers(key: string): string[] {
			promotedVersion(); // subscribe
			return [...workspace(key).promoted];
		},

		/** Layout of a scope without switching to it. */
		getScopeLayout(key: string): PaneLayoutState | null {
			return workspace(key).layout;
		},

		/**
		 * Make `key`'s workspace hold exactly `termIds` — the auto-consolidation
		 * entry point (#e767).
		 *
		 * Declarative on purpose: the caller recomputes the repo's worktree
		 * terminals and hands over the whole set, so a worktree created, removed
		 * or archived between calls needs no separate event. Idempotent, so it is
		 * safe to drive from an effect that re-runs on unrelated store writes.
		 */
		syncScopeMembers(key: string, termIds: string[]): void {
			const ws = workspace(key);
			const wanted = new Set(termIds);
			let changed = false;

			for (const existing of [...ws.promoted]) {
				if (wanted.has(existing)) continue;
				ws.promoted.delete(existing);
				if (ws.layout) ws.layout = removeTerminalFromLayout(ws.layout, existing);
				changed = true;
			}
			for (const wantedId of termIds) {
				if (ws.promoted.has(wantedId)) continue;
				ws.promoted.add(wantedId);
				ws.layout = addTerminalToLayout(ws.layout, wantedId);
				changed = true;
			}

			if (!changed) return;
			bumpPromoted();
			if (key === scope) syncToPaneStore();
		},
	};
}

export const globalWorkspaceStore = createGlobalWorkspaceStore();

// Debug registry — expose global workspace state for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("globalWorkspace", () => {
	return {
		isActive: globalWorkspaceStore.isActive(),
		promotedTerminals: globalWorkspaceStore.getPromotedIds(),
		layout: globalWorkspaceStore.getLayout(),
	};
});

// Wire up terminal removal via callback — avoids direct coupling from terminals→globalWorkspace
terminalsStore.onRemove((id) => globalWorkspaceStore.onTerminalRemoved(id));
