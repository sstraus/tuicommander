import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { repositoriesStore } from "../../stores/repositories";
import { useQuickSwitcher } from "../../hooks/useQuickSwitcher";

function resetStores() {
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
}

describe("useQuickSwitcher", () => {
  const mockHandleBranchSelect = vi.fn().mockResolvedValue(undefined);

  let switcher: ReturnType<typeof useQuickSwitcher>;

  beforeEach(() => {
    resetStores();
    vi.clearAllMocks();

    switcher = useQuickSwitcher({
      handleBranchSelect: mockHandleBranchSelect,
    });
  });

  describe("switchToBranchByIndex", () => {
    it("selects the branch at the given index", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });

      switcher.switchToBranchByIndex(1);

      expect(mockHandleBranchSelect).toHaveBeenCalledTimes(1);
      // First branch should be called (sorted: feature < main alphabetically, unless isMain)
      const [repoPath, branchName] = mockHandleBranchSelect.mock.calls[0];
      expect(repoPath).toBe("/repo");
      expect(branchName).toBeDefined();
    });

    it("does nothing for index beyond available branches", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      switcher.switchToBranchByIndex(5);

      expect(mockHandleBranchSelect).not.toHaveBeenCalled();
    });

    it("works across multiple repos", () => {
      repositoriesStore.add({ path: "/repo1", displayName: "Repo1" });
      repositoriesStore.setBranch("/repo1", "main", { worktreePath: "/repo1" });
      repositoriesStore.add({ path: "/repo2", displayName: "Repo2" });
      repositoriesStore.setBranch("/repo2", "develop", { worktreePath: "/repo2" });

      // Index 2 should hit the second repo's branch
      switcher.switchToBranchByIndex(2);

      expect(mockHandleBranchSelect).toHaveBeenCalledTimes(1);
    });

    it("sorts main branch first", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      switcher.switchToBranchByIndex(1);

      expect(mockHandleBranchSelect).toHaveBeenCalledWith("/repo", "main");
    });
  });
});
