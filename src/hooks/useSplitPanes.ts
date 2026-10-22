import { createSignal } from "solid-js";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { type PaneLayoutState, paneLayoutStore } from "../stores/paneLayout";
import { terminalsStore } from "../stores/terminals";

let refitTimer: ReturnType<typeof setTimeout> | null = null;
let focusRaf = 0;

/** Re-fit terminals in all pane groups after CSS flex layout settles (~150ms). */
function refitPaneTerminals() {
	if (refitTimer) clearTimeout(refitTimer);
	refitTimer = setTimeout(() => {
		refitTimer = null;
		for (const groupId of paneLayoutStore.getAllGroupIds()) {
			const group = paneLayoutStore.state.groups[groupId];
			if (!group) continue;
			for (const tab of group.tabs) {
				if (tab.type === "terminal") {
					terminalsStore.get(tab.id)?.ref?.fit();
				}
			}
		}
	}, 150);
}

export function _testCancelPendingTimers(): void {
	if (refitTimer) {
		clearTimeout(refitTimer);
		refitTimer = null;
	}
	cancelAnimationFrame(focusRaf);
	focusRaf = 0;
}

/** Split pane management using recursive pane tree */
export function useSplitPanes() {
	const [zoomed, setZoomed] = createSignal(false);
	const [savedLayout, setSavedLayout] = createSignal<PaneLayoutState | null>(null);

	const handleSplit = (direction: "vertical" | "horizontal") => {
		if (!paneLayoutStore.isSplit()) {
			// First split: only the active terminal in group 1, group 2 empty
			const activeId = terminalsStore.state.activeId;
			if (!activeId) return;

			const groupId = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(groupId, { id: activeId, type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: groupId });
			paneLayoutStore.setActiveGroup(groupId);

			paneLayoutStore.split(groupId, direction);
		} else {
			// Already split: split the active group
			const activeGroupId = paneLayoutStore.state.activeGroupId;
			if (!activeGroupId) return;
			paneLayoutStore.split(activeGroupId, direction);
		}

		refitPaneTerminals();
	};

	const resetLayout = () => {
		paneLayoutStore.reset();
		// A plain reset doesn't stick while the global workspace is active — it
		// owns the live layout and reapplies its own cached arrangement on the
		// very next promote/unpromote (e.g. a new terminal getting consolidated),
		// silently reintroducing whatever split the user just reset. Collapse its
		// cache to a single flat pane too, so there's nothing stale left to reapply.
		if (globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.resetActiveLayout();
		}
		setZoomed(false);
		setSavedLayout(null);
	};

	/** Close the active pane group. Terminals in the group are destroyed.
	 *  If a split still has an empty pane (freshly split, no terminal placed
	 *  yet), closing that empty pane is preferred — it cancels the split
	 *  without destroying a terminal. We can't rely on activeGroupId to target
	 *  it: an empty pane can't hold DOM focus, so the active-group pointer
	 *  drifts back to the terminal's group on the next focus event. */
	const closeActivePane = () => {
		const emptyGroupId = paneLayoutStore
			.getAllGroupIds()
			.find((id) => paneLayoutStore.state.groups[id]?.tabs.length === 0);
		const targetGroupId = emptyGroupId ?? paneLayoutStore.state.activeGroupId;
		if (!targetGroupId) return;

		// Close terminal sessions in this group
		const group = paneLayoutStore.state.groups[targetGroupId];
		if (group) {
			for (const tab of group.tabs) {
				if (tab.type === "terminal") {
					terminalsStore.remove(tab.id);
				}
			}
		}

		paneLayoutStore.closePane(targetGroupId);

		// Focus the terminal in the new active group (if any)
		const newActiveGroup = paneLayoutStore.getActiveGroup();
		if (newActiveGroup) {
			const termTab = newActiveGroup.tabs.find((t) => t.type === "terminal");
			if (termTab) {
				terminalsStore.setActive(termTab.id);
				cancelAnimationFrame(focusRaf);
				focusRaf = requestAnimationFrame(() => terminalsStore.get(termTab.id)?.ref?.focus());
			}
		}

		refitPaneTerminals();
	};

	const toggleZoomPane = () => {
		if (zoomed()) {
			// Restore saved layout
			const saved = savedLayout();
			if (saved) {
				paneLayoutStore.restore(saved);
				setSavedLayout(null);
				refitPaneTerminals();
			}
			setZoomed(false);
		} else {
			if (!paneLayoutStore.isSplit()) return;
			const activeGroupId = paneLayoutStore.state.activeGroupId;
			if (!activeGroupId) return;

			// Save full state, then zoom to single leaf
			setSavedLayout(paneLayoutStore.serialize());
			paneLayoutStore.setRoot({ type: "leaf", id: activeGroupId });
			setZoomed(true);
			refitPaneTerminals();
		}
	};

	return { handleSplit, resetLayout, closeActivePane, toggleZoomPane, zoomed };
}
