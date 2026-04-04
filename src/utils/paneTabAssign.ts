import { paneLayoutStore, type PaneTabType } from "../stores/paneLayout";
import { setOnTabAdded } from "../stores/tabManager";

/** Map store names to PaneTabType */
const STORE_TO_TYPE: Record<string, PaneTabType> = {
  markdown: "markdown",
  diff: "diff",
  editor: "editor",
};

/** Assign a tab to the active pane group when split mode is on.
 *  No-op when not in split mode. Safe to call unconditionally after tab creation. */
export function assignTabToActiveGroup(tabId: string, type: PaneTabType): void {
  if (!paneLayoutStore.isSplit()) return;
  const activeGroupId = paneLayoutStore.state.activeGroupId;
  if (!activeGroupId) return;
  paneLayoutStore.addTab(activeGroupId, { id: tabId, type });
}

/** Register the global hook so all tab stores auto-assign to active pane group.
 *  Call once during app initialization. */
export function initPaneTabAssignment(): void {
  setOnTabAdded((tabId, storeName) => {
    const type = STORE_TO_TYPE[storeName];
    if (type) {
      assignTabToActiveGroup(tabId, type);
    }
  });
}
