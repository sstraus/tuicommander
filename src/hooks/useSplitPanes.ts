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
        ratio: 0.5,
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
  };

  return {
    handleSplit,
  };
}
