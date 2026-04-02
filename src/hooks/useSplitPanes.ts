import { createSignal } from "solid-js";
import { terminalsStore, type TabLayout } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";

/** Re-fit panes after CSS flex layout settles (~150ms).
 *  Without this delay, fit() measures pre-layout dimensions → wrong column counts. */
function refitPanes(paneIds: string[] = terminalsStore.state.layout.panes) {
  setTimeout(() => {
    for (const paneId of paneIds) {
      terminalsStore.get(paneId)?.ref?.fit();
    }
  }, 150);
}

/** Split pane management */
export function useSplitPanes() {
  const [zoomed, setZoomed] = createSignal(false);
  const [savedLayout, setSavedLayout] = createSignal<TabLayout | null>(null);

  const handleSplit = (direction: "vertical" | "horizontal") => {
    const activeId = terminalsStore.state.activeId;
    if (!activeId) return;

    // If layout has no panes, initialize it with the current active terminal
    if (terminalsStore.state.layout.panes.length === 0) {
      terminalsStore.setLayout({
        direction: "none",
        panes: [activeId],
        ratios: [],
        activePaneIndex: 0,
      });
    }

    const newId = terminalsStore.splitPane(direction);
    if (!newId) return;

    // Track the new terminal in the branch (skip in unified mode to hide tab)
    if (settingsStore.state.splitTabMode !== "unified") {
      const activeRepo = repositoriesStore.getActive();
      if (activeRepo?.activeBranch) {
        repositoriesStore.addTerminalToBranch(activeRepo.path, activeRepo.activeBranch, newId);
      }
    }

    terminalsStore.setActive(newId);
    refitPanes();
  };

  const resetLayout = () => {
    terminalsStore.setLayout({
      direction: "none",
      panes: terminalsStore.state.layout.panes.slice(0, 1),
      ratios: [],
      activePaneIndex: 0,
    });
  };

  const toggleZoomPane = () => {
    if (zoomed()) {
      // Restore saved layout, filtering out panes that were closed while zoomed
      const layout = savedLayout();
      if (layout) {
        const validPanes = layout.panes.filter(id => terminalsStore.get(id) !== undefined);
        if (validPanes.length === 0) {
          // All saved panes were closed — stay single-pane
          setSavedLayout(null);
          setZoomed(false);
          return;
        }
        const restoredLayout = validPanes.length < layout.panes.length
          ? { ...layout, panes: validPanes, ratios: layout.ratios.slice(0, validPanes.length - 1), activePaneIndex: 0 }
          : layout;
        terminalsStore.setLayout(restoredLayout);
        setSavedLayout(null);
        refitPanes(restoredLayout.panes);
      }
      setZoomed(false);
    } else {
      const layout = terminalsStore.state.layout;
      if (layout.direction === "none" || layout.panes.length <= 1) return;
      // Save current layout and zoom to active pane
      setSavedLayout({ ...layout, panes: [...layout.panes], ratios: [...layout.ratios] });
      const activePane = layout.panes[layout.activePaneIndex] ?? layout.panes[0];
      terminalsStore.setLayout({
        direction: "none",
        panes: [activePane],
        ratios: [],
        activePaneIndex: 0,
      });
      setZoomed(true);
      refitPanes([activePane]);
    }
  };

  return {
    handleSplit,
    resetLayout,
    toggleZoomPane,
    zoomed,
  };
}
