import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { type PaneTab, paneLayoutStore } from "../stores/paneLayout";
import { terminalsStore } from "../stores/terminals";

/**
 * Whether `tab`'s underlying content still exists in its owning store. A tab
 * whose store entry is gone renders nothing (no tab strip entry, no close
 * button).
 * Used to decide whether a pane still has anything worth keeping open, rather
 * than trusting a raw `tabs.length` count that a dead entry can never let
 * reach zero.
 */
export function isPaneTabLive(tab: PaneTab): boolean {
	switch (tab.type) {
		case "terminal":
			return terminalsStore.get(tab.id) != null;
		case "diff":
			return diffTabsStore.get(tab.id) != null;
		case "markdown":
			return mdTabsStore.get(tab.id) != null;
		case "editor":
			return editorTabsStore.get(tab.id) != null;
	}
}

// Sweep a terminal's tab out of the layout the moment it's actually removed,
// collapsing the pane if nothing live is left. Closes a gap the explicit
// tab-close path (useTerminalLifecycle.ts's removeTabFromPane) can't reach on
// its own: a pane whose ONLY tab is a ghost (its session died via a path that
// never fired session-closed — e.g. a tmux-shim-materialized pane killed
// outside the normal close route) has no live sibling tab to ever trigger a
// close, so it stays wedged open until something unrelated happens to close a
// tab in that same group. This mirrors globalWorkspace.ts's own
// terminalsStore.onRemove wiring for the identical reason.
//
// Registered HERE (imported by the desktop-only useTerminalLifecycle.ts), not
// in paneLayout.ts: paneLayout.ts is statically reachable from the mobile
// entry (SessionCard -> activitySnapshot -> globalWorkspace -> paneLayout), and
// importing this module from it would pull diffTabs/mdTabs/editorTabs into
// mobile.html (~12 KB gzip — over its bundle budget). Mobile renders no panes.
terminalsStore.onRemove((id) => {
	const groupId = paneLayoutStore.getGroupForTab(id);
	if (!groupId) return;
	paneLayoutStore.removeTab(groupId, id);
	const updated = paneLayoutStore.state.groups[groupId];
	if (!updated?.tabs.some(isPaneTabLive)) {
		paneLayoutStore.closePane(groupId);
	}
});
