import { paneLayoutStore, type PaneTabType } from "../stores/paneLayout";
import { setOnTabAdded } from "../stores/tabManager";

/** Assign a tab to the active pane group when split mode is on.
 *  No-op when not in split mode. Safe to call unconditionally after tab creation. */
export function assignTabToActiveGroup(tabId: string, type: PaneTabType): void {
  if (!paneLayoutStore.isSplit()) return;
  const activeGroupId = paneLayoutStore.state.activeGroupId;
  if (!activeGroupId) return;
  paneLayoutStore.addTab(activeGroupId, { id: tabId, type });
}

/** Register the global hook so terminal tabs auto-assign to active pane group.
 *  Non-terminal tabs (panels, diff, editor) start unassigned — drag into a
 *  split pane to dock them. Call once during app initialization. */
export function initPaneTabAssignment(): void {
  setOnTabAdded(() => {
    // Only terminals are auto-assigned (handled by pty spawn logic).
    // Non-terminal tabs start as orphans so they render full-page.
  });
}
