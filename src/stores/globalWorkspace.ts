import { createSignal } from "solid-js";
import {
  paneLayoutStore,
  removeLeaf,
  allLeafIds,
  type PaneLayoutState,
  type PaneGroup,
} from "./paneLayout";
import { savedPaneLayouts } from "./savedPaneLayouts";

let globalGroupCounter = 0;
function nextGlobalGroupId(): string {
  return `gw${++globalGroupCounter}`;
}

/** Add a terminal as a tab to the active group. Creates the first group if layout is empty. */
function addTerminalToLayout(
  current: PaneLayoutState | null,
  termId: string,
): PaneLayoutState {
  // First terminal — create single group
  if (!current || !current.root) {
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
function removeTerminalFromLayout(
  current: PaneLayoutState,
  termId: string,
): PaneLayoutState | null {
  // Find the group containing this terminal
  let targetGroupId: string | null = null;
  for (const [gid, group] of Object.entries(current.groups)) {
    if (group.tabs.some(t => t.id === termId)) {
      targetGroupId = gid;
      break;
    }
  }
  if (!targetGroupId || !current.root) return current;

  const group = current.groups[targetGroupId];
  const remainingTabs = group.tabs.filter(t => t.id !== termId);

  // Group still has tabs — just remove the tab
  if (remainingTabs.length > 0) {
    const newActiveTabId = group.activeTabId === termId
      ? remainingTabs[remainingTabs.length - 1].id
      : group.activeTabId;
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
    activeGroupId: current.activeGroupId === targetGroupId
      ? (newRoot.type === "leaf" ? newRoot.id : allLeafIds(newRoot)[0] ?? null)
      : current.activeGroupId,
  };
}

function createGlobalWorkspaceStore() {
  const [isActive, setIsActive] = createSignal(false);
  const [promotedVersion, setPromotedVersion] = createSignal(0);

  // Plain JS set — not in SolidJS store to avoid proxy issues
  const promoted = new Set<string>();

  // Global workspace layout (separate from paneLayoutStore)
  let layout: PaneLayoutState | null = null;

  // Saved repo layout key for restore on auto-deactivation
  let savedRepoLayoutKey: string | null = null;

  function bumpPromoted(): void {
    setPromotedVersion(v => v + 1);
  }

  /** Sync the background layout to paneLayoutStore if global workspace is active */
  function syncToPaneStore(): void {
    if (isActive() && layout) {
      paneLayoutStore.restore(layout);
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
    layout = null;
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
      if (layout) {
        paneLayoutStore.restore(layout);
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
      layout = serialized.root ? serialized : null;

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
     */
    promote(termId: string): boolean {
      if (promoted.has(termId)) return true;

      const updated = addTerminalToLayout(layout, termId);

      promoted.add(termId);
      layout = updated;
      bumpPromoted();
      syncToPaneStore();
      return true;
    },

    /** Remove a terminal from the global workspace. Auto-deactivates when empty. */
    unpromote(termId: string): void {
      if (!promoted.has(termId)) return;
      promoted.delete(termId);

      if (layout) {
        layout = removeTerminalFromLayout(layout, termId);
      }

      bumpPromoted();

      if (isActive() && promoted.size === 0) {
        autoDeactivate();
      } else {
        syncToPaneStore();
      }
    },

    /** Check if a terminal is promoted */
    isPromoted(termId: string): boolean {
      promotedVersion(); // subscribe
      return promoted.has(termId);
    },

    /** Get all promoted terminal IDs */
    getPromotedIds(): string[] {
      promotedVersion(); // subscribe
      return [...promoted];
    },

    /** Whether any terminals are promoted */
    hasPromoted(): boolean {
      promotedVersion(); // subscribe
      return promoted.size > 0;
    },

    /** Handle terminal removal — auto-unpromote and auto-deactivate if needed */
    onTerminalRemoved(termId: string): void {
      if (!promoted.has(termId)) return;
      promoted.delete(termId);

      if (layout) {
        layout = removeTerminalFromLayout(layout, termId);
      }

      bumpPromoted();
      if (isActive()) {
        if (promoted.size === 0) {
          autoDeactivate();
        } else {
          syncToPaneStore();
        }
      }
    },

    /** Get the global workspace layout */
    getLayout(): PaneLayoutState | null {
      return layout;
    },

    /** Set the global workspace layout */
    setLayout(newLayout: PaneLayoutState | null): void {
      layout = newLayout ? JSON.parse(JSON.stringify(newLayout)) : null;
    },
  };
}

export const globalWorkspaceStore = createGlobalWorkspaceStore();
