import { createSignal } from "solid-js";
import {
  paneLayoutStore,
  splitLeaf,
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

/** Add a terminal to a layout, auto-splitting as needed. Returns updated layout or null if denied. */
function addTerminalToLayout(
  current: PaneLayoutState | null,
  termId: string,
): PaneLayoutState | null {
  const groupId = nextGlobalGroupId();
  const newGroup: PaneGroup = {
    id: groupId,
    tabs: [{ id: termId, type: "terminal" }],
    activeTabId: termId,
  };

  // First terminal — single leaf
  if (!current || !current.root) {
    return {
      root: { type: "leaf", id: groupId },
      groups: { [groupId]: newGroup },
      activeGroupId: groupId,
    };
  }

  const root = current.root;

  // Single leaf — split horizontally
  if (root.type === "leaf") {
    return {
      root: {
        type: "branch",
        direction: "horizontal",
        children: [root, { type: "leaf", id: groupId }],
        ratios: [0.5, 0.5],
      },
      groups: { ...current.groups, [groupId]: newGroup },
      activeGroupId: groupId,
    };
  }

  // Branch — find last leaf and split it
  const leafIds = allLeafIds(root);
  const lastLeafId = leafIds[leafIds.length - 1];
  const newRoot = splitLeaf(root, lastLeafId, "horizontal", groupId);

  if (!newRoot) {
    // MAX_SPLIT_DEPTH exceeded
    return null;
  }

  return {
    root: newRoot,
    groups: { ...current.groups, [groupId]: newGroup },
    activeGroupId: groupId,
  };
}

/** Remove a terminal from a layout. Returns updated layout or null if empty. */
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

  function bumpPromoted(): void {
    setPromotedVersion(v => v + 1);
  }

  /** Sync the background layout to paneLayoutStore if global workspace is active */
  function syncToPaneStore(): void {
    if (isActive() && layout) {
      paneLayoutStore.restore(layout);
    }
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

      // Restore repo layout (or reset if none saved)
      if (repoLayoutKey) {
        const saved = savedPaneLayouts.get(repoLayoutKey);
        if (saved) {
          paneLayoutStore.restore(saved);
        } else {
          paneLayoutStore.reset();
        }
      } else {
        paneLayoutStore.reset();
      }
    },

    /**
     * Promote a terminal to the global workspace.
     * Returns true if promoted, false if denied (layout full).
     */
    promote(termId: string): boolean {
      if (promoted.has(termId)) return true;

      const updated = addTerminalToLayout(layout, termId);
      if (!updated) return false; // MAX_SPLIT_DEPTH exceeded

      promoted.add(termId);
      layout = updated;
      bumpPromoted();
      syncToPaneStore();
      return true;
    },

    /** Remove a terminal from the global workspace */
    unpromote(termId: string): void {
      if (!promoted.has(termId)) return;
      promoted.delete(termId);

      if (layout) {
        layout = removeTerminalFromLayout(layout, termId);
      }

      bumpPromoted();
      syncToPaneStore();
      if (isActive() && !layout) {
        paneLayoutStore.reset();
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
          layout = null;
          paneLayoutStore.reset();
          setIsActive(false);
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
