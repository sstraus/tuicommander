import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";

/** Split pane management */
export function useSplitPanes() {
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

    // Force re-fit ALL panes after CSS flex layout settles.
    // Without this, the new pane's fit() runs before flex ratios take effect,
    // resulting in wrong column counts (terminal shows ~5 cols instead of ~40).
    setTimeout(() => {
      for (const paneId of terminalsStore.state.layout.panes) {
        terminalsStore.get(paneId)?.ref?.fit();
      }
    }, 150);
  };

  const resetLayout = () => {
    terminalsStore.setLayout({
      direction: "none",
      panes: terminalsStore.state.layout.panes.slice(0, 1),
      ratios: [],
      activePaneIndex: 0,
    });
  };

  return {
    handleSplit,
    resetLayout,
  };
}
