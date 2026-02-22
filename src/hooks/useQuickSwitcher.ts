import { repositoriesStore } from "../stores/repositories";

/** Dependencies injected into useQuickSwitcher */
export interface QuickSwitcherDeps {
  handleBranchSelect: (repoPath: string, branchName: string) => void;
}

/** Quick switcher: resolve shortcut index to repo+branch.
 * Must use the same repo ordering as Sidebar.repoShortcutStarts:
 * grouped repos (in groupOrder) first, then ungrouped repos. */
export function useQuickSwitcher(deps: QuickSwitcherDeps) {
  const switchToBranchByIndex = (index: number) => {
    let counter = 1;
    const layout = repositoriesStore.getGroupedLayout();
    const orderedRepos = [
      ...layout.groups.flatMap((g) => g.repos),
      ...layout.ungrouped,
    ];
    for (const repo of orderedRepos) {
      const branches = Object.values(repo.branches).sort((a, b) => {
        if (a.isMain && !b.isMain) return -1;
        if (!a.isMain && b.isMain) return 1;
        return a.name.localeCompare(b.name);
      });
      for (const branch of branches) {
        if (counter === index) {
          deps.handleBranchSelect(repo.path, branch.name);
          return;
        }
        counter++;
      }
    }
  };

  return {
    switchToBranchByIndex,
  };
}
