import { createSignal } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { paneLayoutStore, type PaneLayoutState } from "../stores/paneLayout";

/** Re-fit terminals in all pane groups after CSS flex layout settles (~150ms). */
function refitPaneTerminals() {
  setTimeout(() => {
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

/** Split pane management using recursive pane tree */
export function useSplitPanes() {
  const [zoomed, setZoomed] = createSignal(false);
  const [savedLayout, setSavedLayout] = createSignal<PaneLayoutState | null>(null);

  const handleSplit = (direction: "vertical" | "horizontal") => {
    if (!paneLayoutStore.isSplit()) {
      // First split: bootstrap tree with current active terminal
      const activeId = terminalsStore.state.activeId;
      if (!activeId) return;

      const groupId = paneLayoutStore.createGroup();
      paneLayoutStore.addTab(groupId, { id: activeId, type: "terminal" });
      paneLayoutStore.setRoot({ type: "leaf", id: groupId });
      paneLayoutStore.setActiveGroup(groupId);

      // Split creates empty second group (no dummy terminal)
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
    setZoomed(false);
    setSavedLayout(null);
  };

  /** Close the active pane group. Terminals in the group are destroyed. */
  const closeActivePane = () => {
    const activeGroupId = paneLayoutStore.state.activeGroupId;
    if (!activeGroupId) return;

    // Close terminal sessions in this group
    const group = paneLayoutStore.state.groups[activeGroupId];
    if (group) {
      for (const tab of group.tabs) {
        if (tab.type === "terminal") {
          terminalsStore.remove(tab.id);
        }
      }
    }

    paneLayoutStore.closePane(activeGroupId);

    // Focus the terminal in the new active group (if any)
    const newActiveGroup = paneLayoutStore.getActiveGroup();
    if (newActiveGroup) {
      const termTab = newActiveGroup.tabs.find(t => t.type === "terminal");
      if (termTab) {
        terminalsStore.setActive(termTab.id);
        requestAnimationFrame(() => terminalsStore.get(termTab.id)?.ref?.focus());
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
