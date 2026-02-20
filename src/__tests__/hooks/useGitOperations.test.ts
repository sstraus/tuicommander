import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { useGitOperations } from "../../hooks/useGitOperations";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
}

describe("useGitOperations", () => {
  const mockRepo = {
    getInfo: vi.fn(),
    getDiffStats: vi.fn().mockResolvedValue({ additions: 0, deletions: 0 }),
    getWorktreePaths: vi.fn().mockResolvedValue({}),
    removeWorktree: vi.fn().mockResolvedValue(undefined),
    createWorktree: vi.fn(),
    renameBranch: vi.fn().mockResolvedValue(undefined),
    generateWorktreeName: vi.fn().mockResolvedValue("bold-nexus-042"),
  };

  const mockPty = {
    canSpawn: vi.fn().mockResolvedValue(true),
    write: vi.fn().mockResolvedValue(undefined),
  };

  const mockDialogs = {
    confirmRemoveRepo: vi.fn().mockResolvedValue(true),
    confirmRemoveWorktree: vi.fn().mockResolvedValue(true),
  };

  const mockCloseTerminal = vi.fn().mockResolvedValue(undefined);
  const mockCreateNewTerminal = vi.fn().mockResolvedValue("term-new");
  const mockSetStatusInfo = vi.fn();

  let gitOps: ReturnType<typeof useGitOperations>;

  beforeEach(() => {
    resetStores();
    vi.clearAllMocks();
    mockPty.canSpawn.mockResolvedValue(true);
    mockDialogs.confirmRemoveRepo.mockResolvedValue(true);
    mockDialogs.confirmRemoveWorktree.mockResolvedValue(true);

    gitOps = useGitOperations({
      repo: mockRepo,
      pty: mockPty,
      dialogs: mockDialogs,
      closeTerminal: mockCloseTerminal,
      createNewTerminal: mockCreateNewTerminal,
      setStatusInfo: mockSetStatusInfo,
      getDefaultFontSize: () => 14,
      getMaxTabNameLength: () => 25,
    });
  });

  describe("handleBranchSelect", () => {
    it("sets active repo and branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      await gitOps.handleBranchSelect("/repo", "main");

      expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
      expect(repositoriesStore.get("/repo")?.activeBranch).toBe("main");
      expect(gitOps.currentRepoPath()).toBe("/repo");
      expect(gitOps.currentBranch()).toBe("main");
    });

    it("auto-spawns terminal on first branch select", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });

      await gitOps.handleBranchSelect("/repo", "feature");

      // First time → should auto-create a terminal
      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      expect(branch?.terminals.length).toBeGreaterThan(0);
      expect(branch?.hadTerminals).toBe(true);
    });

    it("does not auto-spawn after user closed all terminals", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt", hadTerminals: true });

      await gitOps.handleBranchSelect("/repo", "feature");

      // hadTerminals is true but no live terminals → show empty state, no spawn
      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      expect(branch?.terminals.length).toBe(0);
    });

    it("restores terminals from savedTerminals on branch click", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", {
        worktreePath: "/repo/wt",
        hadTerminals: true,
        savedTerminals: [
          { name: "Terminal 1", cwd: "/repo/wt", fontSize: 14, agentType: null },
          { name: "Agent", cwd: "/repo/wt", fontSize: 12, agentType: null },
        ],
      });

      await gitOps.handleBranchSelect("/repo", "feature");

      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      expect(branch?.terminals.length).toBe(2);
      // savedTerminals should be consumed
      expect(branch?.savedTerminals?.length).toBe(0);
      // First restored terminal should be active
      expect(terminalsStore.state.activeId).toBe(branch?.terminals[0]);
    });

    it("preserves terminal metadata during lazy restore", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", {
        worktreePath: "/repo/wt",
        hadTerminals: true,
        savedTerminals: [
          { name: "My Terminal", cwd: "/custom/path", fontSize: 16, agentType: null },
        ],
      });

      await gitOps.handleBranchSelect("/repo", "feature");

      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      const termId = branch?.terminals[0];
      const terminal = termId ? terminalsStore.get(termId) : undefined;
      expect(terminal?.name).toBe("My Terminal");
      expect(terminal?.cwd).toBe("/custom/path");
      expect(terminal?.fontSize).toBe(16);
      expect(terminal?.sessionId).toBeNull();
    });

    it("does not restore savedTerminals when live terminals exist", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", {
        worktreePath: "/repo",
        savedTerminals: [
          { name: "Saved", cwd: "/repo", fontSize: 14, agentType: null },
        ],
      });

      // Add a live terminal
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Live",
        cwd: "/repo",
        awaitingInput: null,
      });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      await gitOps.handleBranchSelect("/repo", "main");

      // Should activate the live terminal, not restore from saved
      expect(terminalsStore.state.activeId).toBe(id);
      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.terminals.length).toBe(1);
    });

    it("activates existing terminal when branch has one", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Existing",
        cwd: "/repo",
        awaitingInput: null,
      });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      await gitOps.handleBranchSelect("/repo", "main");

      expect(terminalsStore.state.activeId).toBe(id);
    });
  });

  describe("handleAddTerminalToBranch", () => {
    it("creates terminal with branch worktree path", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt-feature" });

      const id = await gitOps.handleAddTerminalToBranch("/repo", "feature");

      expect(id).toBeDefined();
      const t = terminalsStore.get(id!);
      expect(t?.cwd).toBe("/repo/wt-feature");
      expect(t?.name).toContain("feature");
    });

    it("sets status info when max sessions reached", async () => {
      mockPty.canSpawn.mockResolvedValue(false);
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      await gitOps.handleAddTerminalToBranch("/repo", "main");

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
    });
  });

  describe("handleRemoveRepo", () => {
    it("removes repo after confirmation", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "My Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      gitOps.setCurrentRepoPath("/repo");

      await gitOps.handleRemoveRepo("/repo");

      expect(mockDialogs.confirmRemoveRepo).toHaveBeenCalledWith("My Repo");
      expect(repositoriesStore.get("/repo")).toBeUndefined();
      expect(gitOps.currentRepoPath()).toBeUndefined();
    });

    it("does not remove when user cancels", async () => {
      mockDialogs.confirmRemoveRepo.mockResolvedValue(false);
      repositoriesStore.add({ path: "/repo", displayName: "My Repo" });

      await gitOps.handleRemoveRepo("/repo");

      expect(repositoriesStore.get("/repo")).toBeDefined();
    });
  });

  describe("handleRemoveBranch", () => {
    it("removes worktree branch after confirmation", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledWith("feature");
      expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature");
      expect(repositoriesStore.get("/repo")?.branches["feature"]).toBeUndefined();
    });

    it("rejects removal of non-worktree branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", {});

      await gitOps.handleRemoveBranch("/repo", "main");

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Cannot remove main: not a worktree");
    });
  });

  describe("handleRenameBranch", () => {
    it("renames branch in backend and store", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "old-name", { worktreePath: "/repo" });
      gitOps.setBranchToRename({ repoPath: "/repo", branchName: "old-name" });
      gitOps.setCurrentBranch("old-name");

      await gitOps.handleRenameBranch("old-name", "new-name");

      expect(mockRepo.renameBranch).toHaveBeenCalledWith("/repo", "old-name", "new-name");
      expect(repositoriesStore.get("/repo")?.branches["new-name"]).toBeDefined();
      expect(repositoriesStore.get("/repo")?.branches["old-name"]).toBeUndefined();
      expect(gitOps.currentBranch()).toBe("new-name");
    });
  });

  describe("activeWorktreePath", () => {
    it("returns undefined when no active repo", () => {
      expect(gitOps.activeWorktreePath()).toBeUndefined();
    });

    it("returns worktree path of active branch", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo/main" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      expect(gitOps.activeWorktreePath()).toBe("/repo/main");
    });
  });

  describe("activeRunCommand", () => {
    it("returns undefined when no active repo", () => {
      expect(gitOps.activeRunCommand()).toBeUndefined();
    });

    it("returns saved run command", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.setRunCommand("/repo", "main", "npm test");

      expect(gitOps.activeRunCommand()).toBe("npm test");
    });
  });

  describe("handleNewTab", () => {
    it("creates terminal in active branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      await gitOps.handleNewTab();

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.terminals.length).toBeGreaterThan(0);
    });

    it("falls back to createNewTerminal when no active branch", async () => {
      await gitOps.handleNewTab();

      expect(mockCreateNewTerminal).toHaveBeenCalled();
    });
  });

  describe("refreshAllBranchStats", () => {
    it("updates branch stats for all repos", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/repo" });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 5, deletions: 3 });

      await gitOps.refreshAllBranchStats();

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.additions).toBe(5);
      expect(branch?.deletions).toBe(3);
    });

    it("prunes branches not in worktree paths", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "stale", { worktreePath: "/repo/stale" });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/repo" });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.refreshAllBranchStats();

      expect(repositoriesStore.get("/repo")?.branches["stale"]).toBeUndefined();
      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeDefined();
    });

    it("ignores diff stats errors for individual branches", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/repo" });
      mockRepo.getDiffStats.mockRejectedValue(new Error("stats failed"));

      await gitOps.refreshAllBranchStats();

      // Should not throw
      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeDefined();
    });
  });

  describe("handleRemoveBranch (backend failure)", () => {
    it("cleans up UI even when backend removal fails", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      mockRepo.removeWorktree.mockRejectedValue(new Error("git error"));

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(repositoriesStore.get("/repo")?.branches["feature"]).toBeUndefined();
      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("git cleanup may be needed"));
    });

    it("closes branch terminals before removing", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      const id = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "feature", id);

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(mockCloseTerminal).toHaveBeenCalledWith(id, true);
    });

    it("does not remove when user cancels worktree confirmation", async () => {
      mockDialogs.confirmRemoveWorktree.mockResolvedValue(false);
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(repositoriesStore.get("/repo")?.branches["feature"]).toBeDefined();
    });
  });

  describe("handleRemoveRepo (edge cases)", () => {
    it("closes all branch terminals", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: null, awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: null, awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);
      gitOps.setCurrentRepoPath("/repo");

      await gitOps.handleRemoveRepo("/repo");

      expect(mockCloseTerminal).toHaveBeenCalledWith(id1, true);
      expect(mockCloseTerminal).toHaveBeenCalledWith(id2, true);
    });

    it("does nothing for non-existent repo", async () => {
      await gitOps.handleRemoveRepo("/nonexistent");

      expect(mockDialogs.confirmRemoveRepo).not.toHaveBeenCalled();
    });

    it("creates fallback terminal when no terminals remain", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      // No terminals in the branch, so after removal getCount() === 0

      await gitOps.handleRemoveRepo("/repo");

      expect(mockCreateNewTerminal).toHaveBeenCalled();
    });
  });

  describe("handleAddWorktree", () => {
    it("delegates name generation to backend", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.createWorktree.mockResolvedValue({
        name: "bold-nexus-042",
        path: "/repo/.worktrees/bold-nexus-042",
        branch: "bold-nexus-042",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");

      expect(mockRepo.generateWorktreeName).toHaveBeenCalledWith(["main"]);
      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "bold-nexus-042");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree bold-nexus-042");
    });

    it("passes existing branch names to name generator", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature-1", { worktreePath: "/repo/wt1" });
      mockRepo.generateWorktreeName.mockResolvedValue("cool-ripley-007");
      mockRepo.createWorktree.mockResolvedValue({
        name: "cool-ripley-007",
        path: "/repo/.worktrees/cool-ripley-007",
        branch: "cool-ripley-007",
        base_repo: "/repo",
      });

      await gitOps.handleAddWorktree("/repo");

      expect(mockRepo.generateWorktreeName).toHaveBeenCalledWith(["main", "feature-1"]);
      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "cool-ripley-007");
    });

    it("reports error on worktree creation failure", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.createWorktree.mockRejectedValue(new Error("branch exists"));

      await gitOps.handleAddWorktree("/repo");

      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to create worktree"));
    });
  });

  describe("executeRunCommand", () => {
    it("creates terminal and waits for session to send command", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      vi.useFakeTimers();
      await gitOps.executeRunCommand("npm test");

      // Should save command and create terminal
      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.runCommand).toBe("npm test");
      expect(terminalsStore.getCount()).toBeGreaterThan(0);

      // Simulate session becoming available
      const ids = terminalsStore.getIds();
      terminalsStore.update(ids[ids.length - 1], { sessionId: "sess-run" });

      await vi.advanceTimersByTimeAsync(200);

      expect(mockPty.write).toHaveBeenCalledWith("sess-run", "npm test\n");
      vi.useRealTimers();
    });

    it("does nothing when no active repo/branch", async () => {
      await gitOps.executeRunCommand("npm test");

      expect(terminalsStore.getCount()).toBe(0);
    });

    it("shows status when max sessions reached", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      mockPty.canSpawn.mockResolvedValue(false);

      await gitOps.executeRunCommand("npm test");

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
    });

    it("truncates long command names in tab", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      await gitOps.executeRunCommand("npm run test:integration:coverage --verbose");

      const ids = terminalsStore.getIds();
      const t = terminalsStore.get(ids[ids.length - 1]);
      expect(t?.name.length).toBeLessThanOrEqual(28); // 25 + "..."
    });

    it("respects custom maxTabNameLength from config", async () => {
      resetStores();
      vi.clearAllMocks();
      mockPty.canSpawn.mockResolvedValue(true);

      const customGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: mockDialogs,
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 10,
      });

      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      await customGitOps.executeRunCommand("npm run test:integration");

      const ids = terminalsStore.getIds();
      const t = terminalsStore.get(ids[ids.length - 1]);
      expect(t?.name).toBe("npm run te...");
    });
  });

  describe("handleRunCommand", () => {
    it("opens dialog when no saved command", () => {
      const openDialog = vi.fn();
      gitOps.handleRunCommand(false, openDialog);

      expect(openDialog).toHaveBeenCalled();
    });

    it("executes saved command directly", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.setRunCommand("/repo", "main", "npm test");

      const openDialog = vi.fn();
      gitOps.handleRunCommand(false, openDialog);

      expect(openDialog).not.toHaveBeenCalled();
    });

    it("opens dialog when forceDialog is true even with saved command", () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repositoriesStore.setRunCommand("/repo", "main", "npm test");

      const openDialog = vi.fn();
      gitOps.handleRunCommand(true, openDialog);

      expect(openDialog).toHaveBeenCalled();
    });
  });

  describe("handleRepoSettings", () => {
    it("opens settings panel with repo context", () => {
      repositoriesStore.add({ path: "/repo", displayName: "My Repo" });
      const openSettingsPanel = vi.fn();

      gitOps.handleRepoSettings("/repo", openSettingsPanel);

      expect(gitOps.currentRepoPath()).toBe("/repo");
      expect(openSettingsPanel).toHaveBeenCalledWith({
        kind: "repo",
        repoPath: "/repo",
        displayName: "My Repo",
      });
    });
  });

  describe("handleRenameBranch (edge cases)", () => {
    it("does nothing when no branchToRename set", async () => {
      await gitOps.handleRenameBranch("old", "new");

      expect(mockRepo.renameBranch).not.toHaveBeenCalled();
    });

    it("does not update currentBranch if renaming non-active branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      gitOps.setBranchToRename({ repoPath: "/repo", branchName: "feature" });
      gitOps.setCurrentBranch("main");

      await gitOps.handleRenameBranch("feature", "feature-v2");

      expect(gitOps.currentBranch()).toBe("main");
    });
  });

  describe("handleOpenRenameBranchDialog", () => {
    it("sets branchToRename signal", () => {
      gitOps.handleOpenRenameBranchDialog("/repo", "feature");

      expect(gitOps.branchToRename()).toEqual({ repoPath: "/repo", branchName: "feature" });
    });
  });

  describe("handleAddRepo", () => {
    it("does nothing when dialog is cancelled", async () => {
      vi.mocked(open).mockResolvedValue(null);

      await gitOps.handleAddRepo();

      expect(mockRepo.getInfo).not.toHaveBeenCalled();
    });

    it("adds repo when user selects folder", async () => {
      vi.mocked(open).mockResolvedValue("/new-repo");
      mockRepo.getInfo.mockResolvedValue({
        path: "/new-repo",
        name: "new-repo",
        initials: "NR",
        branch: "main",
        status: "clean",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/new-repo" });

      await gitOps.handleAddRepo();

      expect(repositoriesStore.get("/new-repo")).toBeDefined();
      expect(repositoriesStore.get("/new-repo")?.displayName).toBe("new-repo");
      expect(repositoriesStore.get("/new-repo")?.activeBranch).toBe("main");
    });

    it("handles array result from dialog", async () => {
      vi.mocked(open).mockResolvedValue(["/array-repo"] as unknown as string);
      mockRepo.getInfo.mockResolvedValue({
        path: "/array-repo",
        name: "array-repo",
        initials: "AR",
        branch: "develop",
        status: "dirty",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 1, deletions: 0 });
      mockRepo.getWorktreePaths.mockResolvedValue({ develop: "/array-repo" });

      await gitOps.handleAddRepo();

      expect(repositoriesStore.get("/array-repo")).toBeDefined();
    });

    it("reports error when getInfo fails", async () => {
      vi.mocked(open).mockResolvedValue("/bad-repo");
      mockRepo.getInfo.mockRejectedValue(new Error("not a git repo"));

      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      await gitOps.handleAddRepo();
      errSpy.mockRestore();

      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to add repo"));
    });

    it("closes orphan terminals when adding repo", async () => {
      // Create an orphan terminal (not tracked by any branch)
      const orphanId = terminalsStore.add({ sessionId: null, fontSize: 14, name: "orphan", cwd: null, awaitingInput: null });

      vi.mocked(open).mockResolvedValue("/new-repo");
      mockRepo.getInfo.mockResolvedValue({
        path: "/new-repo",
        name: "new-repo",
        initials: "NR",
        branch: "main",
        status: "clean",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/new-repo" });

      await gitOps.handleAddRepo();

      expect(mockCloseTerminal).toHaveBeenCalledWith(orphanId, true);
    });
  });

  describe("executeRunCommand (error path)", () => {
    it("handles write failure gracefully", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      mockPty.write.mockRejectedValue(new Error("write failed"));

      vi.useFakeTimers();
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      await gitOps.executeRunCommand("failing-cmd");

      const ids = terminalsStore.getIds();
      terminalsStore.update(ids[ids.length - 1], { sessionId: "sess-fail" });

      await vi.advanceTimersByTimeAsync(200);

      expect(errSpy).toHaveBeenCalledWith("Failed to send run command:", expect.any(Error));
      errSpy.mockRestore();
      vi.useRealTimers();
    });
  });
});
