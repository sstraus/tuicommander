import { repositoriesStore } from "../stores/repositories";

/** Dependencies injected into useQuickSwitcher */
export interface QuickSwitcherDeps {
  handleBranchSelect: (repoPath: string, branchName: string) => void;
}

/** Quick switcher: resolve shortcut index to repo+branch */
export function useQuickSwitcher(deps: QuickSwitcherDeps) {
  const switchToBranchByIndex = (index: number) => {
    let counter = 1;
    for (const repo of Object.values(repositoriesStore.state.repositories)) {
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
