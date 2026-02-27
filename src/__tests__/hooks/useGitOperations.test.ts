import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import "../mocks/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { useGitOperations } from "../../hooks/useGitOperations";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  for (const path of repositoriesStore.getPaths()) {
    repositoriesStore.remove(path);
  }
  for (const s of repoSettingsStore.getAll()) {
    repoSettingsStore.remove(s.path);
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
    generateCloneBranchName: vi.fn().mockResolvedValue("feat-auth--bold-nexus-042"),
    listBaseRefOptions: vi.fn().mockResolvedValue(["main"]),
    mergeAndArchiveWorktree: vi.fn().mockResolvedValue({ merged: true, action: "archived", archive_path: null }),
    finalizeMergedWorktree: vi.fn().mockResolvedValue({ merged: true, action: "archived", archive_path: null }),
    listLocalBranches: vi.fn().mockResolvedValue(["main"]),
    getMergedBranches: vi.fn().mockResolvedValue(["main"]),
    checkoutRemoteBranch: vi.fn().mockResolvedValue(undefined),
    switchBranch: vi.fn().mockResolvedValue({ success: true, stashed: false, previous_branch: "main", new_branch: "feature" }),
  };

  const mockPty = {
    canSpawn: vi.fn().mockResolvedValue(true),
    write: vi.fn().mockResolvedValue(undefined),
    getWorktreesDir: vi.fn().mockResolvedValue("/repos/.worktrees"),
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

    it("clears activeId when switching to branch with no terminals", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "develop", { worktreePath: "/repo/wt-dev", hadTerminals: true });

      // Select main first — creates a terminal and sets activeId
      await gitOps.handleBranchSelect("/repo", "main");
      const mainTermId = terminalsStore.state.activeId;
      expect(mainTermId).not.toBeNull();

      // Switch to develop which has hadTerminals but no live terminals
      await gitOps.handleBranchSelect("/repo", "develop");

      // activeId must be cleared so the old terminal doesn't bleed through
      expect(terminalsStore.state.activeId).toBeNull();
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

    it("sets pendingResumeCommand on restored agent terminals", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", {
        worktreePath: "/repo/wt",
        hadTerminals: true,
        savedTerminals: [
          { name: "Claude", cwd: "/repo/wt", fontSize: 14, agentType: "claude" },
          { name: "Plain", cwd: "/repo/wt", fontSize: 14, agentType: null },
        ],
      });

      await gitOps.handleBranchSelect("/repo", "feature");

      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      const agentTerm = terminalsStore.get(branch!.terminals[0]);
      const plainTerm = terminalsStore.get(branch!.terminals[1]);
      // Agent terminal gets pendingResumeCommand for banner display
      expect(agentTerm?.pendingResumeCommand).toBe("claude --continue");
      // Plain terminal does not
      expect(plainTerm?.pendingResumeCommand).toBeNull();
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

    it("preserves active tab when re-clicking the same branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      const id1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T1", cwd: "/repo", awaitingInput: null });
      const id2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T2", cwd: "/repo", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "main", id1);
      repositoriesStore.addTerminalToBranch("/repo", "main", id2);

      // Select the branch first so it becomes the active branch
      await gitOps.handleBranchSelect("/repo", "main");
      // Now set the second tab as active
      terminalsStore.setActive(id2);

      // Click the same branch again — should NOT jump to first tab
      await gitOps.handleBranchSelect("/repo", "main");

      expect(terminalsStore.state.activeId).toBe(id2);
    });

    it("remembers last active tab when switching between branches", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt/feature" });

      // Branch main: 2 terminals
      const m1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "M1", cwd: "/repo", awaitingInput: null });
      const m2 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "M2", cwd: "/repo", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "main", m1);
      repositoriesStore.addTerminalToBranch("/repo", "main", m2);

      // Branch feature: 1 terminal
      const f1 = terminalsStore.add({ sessionId: null, fontSize: 14, name: "F1", cwd: "/repo/wt/feature", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "feature", f1);

      // Activate main, select tab m2
      await gitOps.handleBranchSelect("/repo", "main");
      terminalsStore.setActive(m2);

      // Switch to feature — should save m2 as last active for main
      await gitOps.handleBranchSelect("/repo", "feature");
      expect(terminalsStore.state.activeId).toBe(f1);

      // Switch back to main — should restore m2, not m1
      await gitOps.handleBranchSelect("/repo", "main");
      expect(terminalsStore.state.activeId).toBe(m2);
    });

    it("serializes concurrent calls — no duplicate terminals from savedTerminals", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", {
        worktreePath: "/repo/wt",
        hadTerminals: true,
        savedTerminals: [
          { name: "Claude", cwd: "/repo/wt", fontSize: 14, agentType: "claude" },
        ],
      });

      // Fire two selects concurrently — the second must wait for the first
      const p1 = gitOps.handleBranchSelect("/repo", "feature");
      const p2 = gitOps.handleBranchSelect("/repo", "feature");
      await Promise.all([p1, p2]);

      const branch = repositoriesStore.get("/repo")?.branches["feature"];
      // Only ONE terminal should exist — the second call sees the restored
      // terminal as a live validTerminal and does not duplicate.
      expect(branch?.terminals.length).toBe(1);
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
    it("removes worktree branch after confirmation, passing deleteBranchOnRemove setting", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      // Default deleteBranchOnRemove is true (from repoDefaults)
      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledWith("feature");
      expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", true);
      expect(repositoriesStore.get("/repo")?.branches["feature"]).toBeUndefined();
    });

    it("passes deleteBranch=false when repo setting overrides default", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "feature", { worktreePath: "/repo/wt" });
      // Set per-repo setting to override default deleteBranchOnRemove=true
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { deleteBranchOnRemove: false });

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", false);
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

    it("uses active terminal CWD to find correct branch when store activeBranch is stale", async () => {
      // Setup: repo has main + feature/acme (linked worktree), store says main is active (stale)
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature/acme", { worktreePath: "/repo/.worktrees/acme" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main"); // stale — HEAD actually moved to feature/acme

      // Active terminal is in the feature/acme worktree directory
      const existingTid = terminalsStore.add({ sessionId: "s1", fontSize: 14, name: "T1", cwd: "/repo/.worktrees/acme", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "feature/acme", existingTid);
      terminalsStore.setActive(existingTid);

      await gitOps.handleNewTab();

      // New terminal must go to feature/acme (the CWD-matched branch), not main (stale activeBranch)
      const featureBranch = repositoriesStore.get("/repo")?.branches["feature/acme"];
      expect(featureBranch?.terminals.length).toBe(2); // existing + new
      const mainBranch = repositoriesStore.get("/repo")?.branches["main"];
      expect(mainBranch?.terminals.length).toBe(0);
    });

    it("uses active terminal CWD for main worktree when HEAD changed externally", async () => {
      // Setup: repo with one branch, store activeBranch="old-branch" but terminal CWD is repo root
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "old-branch", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "new-branch", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "old-branch"); // stale

      // Active terminal is at repo root (HEAD moved to new-branch externally)
      const existingTid = terminalsStore.add({ sessionId: "s2", fontSize: 14, name: "T1", cwd: "/repo", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "new-branch", existingTid);
      terminalsStore.setActive(existingTid);

      await gitOps.handleNewTab();

      // New terminal goes to new-branch (matched by CWD), not old-branch
      const newBranch = repositoriesStore.get("/repo")?.branches["new-branch"];
      expect(newBranch?.terminals.length).toBe(2);
      const oldBranch = repositoriesStore.get("/repo")?.branches["old-branch"];
      expect(oldBranch?.terminals.length).toBe(0);
    });
  });

  describe("handleMergeAndArchive", () => {
    it("removes branch from sidebar when action is archive", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "feature/x");
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: "/archived/feature-x" });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeUndefined();
      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("archived"));
    });

    it("sets mergePendingCtx when action is pending (ask mode)", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");

      // Branch stays in sidebar — user must choose
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeDefined();
      // Dialog context is populated
      expect(gitOps.mergePendingCtx()).toEqual({ repoPath: "/repo", branchName: "feature/x" });
    });

    it("archives worktree and removes branch when user chooses archive in pending dialog", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });
      mockRepo.finalizeMergedWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: "/archived/feature-x" });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");
      await gitOps.handleMergePendingChoice("archive");

      expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/x", "archive");
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeUndefined();
      expect(gitOps.mergePendingCtx()).toBeNull();
    });

    it("deletes worktree and removes branch when user chooses delete in pending dialog", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });
      mockRepo.finalizeMergedWorktree.mockResolvedValue({ merged: true, action: "deleted", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");
      await gitOps.handleMergePendingChoice("delete");

      expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/x", "delete");
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeUndefined();
      expect(gitOps.mergePendingCtx()).toBeNull();
    });

    it("keeps worktree in sidebar when user cancels the pending dialog", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");
      await gitOps.handleMergePendingChoice("cancel");

      expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
      // Branch stays — worktree is kept as-is
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeDefined();
      expect(gitOps.mergePendingCtx()).toBeNull();
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

    it("discovers externally created worktrees", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      // Simulate an external `git worktree add` — new branch appears in getWorktreePaths
      mockRepo.getWorktreePaths.mockResolvedValue({
        main: "/repo",
        "feature-external": "/repo/.worktrees/feature-external",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 2, deletions: 1 });

      await gitOps.refreshAllBranchStats();

      const newBranch = repositoriesStore.get("/repo")?.branches["feature-external"];
      expect(newBranch).toBeDefined();
      expect(newBranch?.worktreePath).toBe("/repo/.worktrees/feature-external");
      expect(newBranch?.additions).toBe(2);
      expect(newBranch?.deletions).toBe(1);
    });

    it("removes stale activeBranch when HEAD moved to different branch", async () => {
      // Scenario: repo persisted with activeBranch="main", user switched to
      // "feature/acme" externally. Backend returns only "feature/acme" as worktree.
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActiveBranch("/repo", "main");

      // Backend now says HEAD is on feature/acme (main worktree checked out on different branch)
      mockRepo.getWorktreePaths.mockResolvedValue({ "feature/acme": "/repo" });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 1, deletions: 0 });

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      // "main" should be gone — it's not a worktree
      expect(repo?.branches["main"]).toBeUndefined();
      // "feature/acme" should exist
      expect(repo?.branches["feature/acme"]).toBeDefined();
      // activeBranch should have been updated
      expect(repo?.activeBranch).toBe("feature/acme");
    });

    it("migrates terminals from stale activeBranch to new worktree branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActiveBranch("/repo", "main");
      const tid = terminalsStore.add({ sessionId: "s1", fontSize: 14, name: "T1", cwd: "/repo", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "main", tid);

      mockRepo.getWorktreePaths.mockResolvedValue({ "feature/acme": "/repo" });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      expect(repo?.branches["main"]).toBeUndefined();
      expect(repo?.branches["feature/acme"]?.terminals).toContain(tid);
      expect(repo?.activeBranch).toBe("feature/acme");
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
      mockRepo.removeWorktree.mockRejectedValueOnce(new Error("git error"));

      await gitOps.handleRemoveBranch("/repo", "feature");

      expect(repositoriesStore.get("/repo")?.branches["feature"]).toBeUndefined();
      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("worktree removal failed"));
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

  describe("handleAddWorktree (dialog flow)", () => {
    it("opens dialog with suggested name and branch lists", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main", "develop"]);

      await gitOps.handleAddWorktree("/repo");

      const state = gitOps.worktreeDialogState();
      expect(state).not.toBeNull();
      expect(state?.suggestedName).toBe("bold-nexus-042");
      expect(state?.existingBranches).toEqual(["main", "develop"]);
      expect(state?.worktreeBranches).toEqual(["main"]);
      expect(state?.worktreesDir).toBe("/repos/.worktrees");
    });

    it("passes existing worktree branches to name generator", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature-1", { worktreePath: "/repo/wt1" });
      mockRepo.generateWorktreeName.mockResolvedValue("cool-ripley-007");
      mockRepo.listLocalBranches.mockResolvedValue(["main", "feature-1", "develop"]);

      await gitOps.handleAddWorktree("/repo");

      expect(mockRepo.generateWorktreeName).toHaveBeenCalledWith(["main", "feature-1"]);
    });

    it("skips dialog and creates worktree instantly when promptOnCreate is false", async () => {
      const noPromptGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: mockDialogs,
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
        getPromptOnCreate: () => false,
      });

      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main"]);
      mockRepo.createWorktree.mockResolvedValue({
        name: "bold-nexus-042",
        path: "/repo/.worktrees/bold-nexus-042",
        branch: "bold-nexus-042",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await noPromptGitOps.handleAddWorktree("/repo");

      // Dialog should NOT be open
      expect(noPromptGitOps.worktreeDialogState()).toBeNull();
      // Worktree should be created directly with the auto-generated name
      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "bold-nexus-042", true, "main");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree bold-nexus-042");
    });

    it("shows dialog when promptOnCreate is true (default)", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main"]);

      await gitOps.handleAddWorktree("/repo");

      // Dialog should be open
      expect(gitOps.worktreeDialogState()).not.toBeNull();
      // Worktree should NOT be created yet
      expect(mockRepo.createWorktree).not.toHaveBeenCalled();
    });

    it("uses first baseRef as default when skipping dialog", async () => {
      const noPromptGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: mockDialogs,
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
        getPromptOnCreate: () => false,
      });

      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("cool-ripley-007");
      mockRepo.listLocalBranches.mockResolvedValue(["main", "develop"]);
      mockRepo.listBaseRefOptions.mockResolvedValue(["develop", "main"]);
      mockRepo.createWorktree.mockResolvedValue({
        name: "cool-ripley-007",
        path: "/repo/.worktrees/cool-ripley-007",
        branch: "cool-ripley-007",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await noPromptGitOps.handleAddWorktree("/repo");

      // Should use first baseRef option as the base
      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "cool-ripley-007", true, "develop");
    });
  });

  describe("confirmCreateWorktree", () => {
    it("creates worktree with new branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main"]);
      mockRepo.createWorktree.mockResolvedValue({
        name: "bold-nexus-042",
        path: "/repo/.worktrees/bold-nexus-042",
        branch: "bold-nexus-042",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      // Open dialog first
      await gitOps.handleAddWorktree("/repo");
      // Confirm creation
      await gitOps.confirmCreateWorktree({ branchName: "bold-nexus-042", createBranch: true, baseRef: "main" });

      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "bold-nexus-042", true, "main");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree bold-nexus-042");
    });

    it("creates worktree from existing branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main", "develop"]);
      mockRepo.createWorktree.mockResolvedValue({
        name: "develop",
        path: "/repo/.worktrees/develop",
        branch: "develop",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "develop", createBranch: false, baseRef: "main" });

      expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "develop", false, "main");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree develop");
    });

    it("reports error on worktree creation failure", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
      mockRepo.listLocalBranches.mockResolvedValue(["main"]);
      mockRepo.createWorktree.mockRejectedValue(new Error("branch exists"));

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "bold-nexus-042", createBranch: true, baseRef: "main" });

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

    it("creates exactly one terminal (no double-spawn from branch auto-spawn)", async () => {
      vi.mocked(open).mockResolvedValue("/fresh-repo");
      mockRepo.getInfo.mockResolvedValue({
        path: "/fresh-repo",
        name: "fresh-repo",
        initials: "FR",
        branch: "main",
        status: "clean",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/fresh-repo" });

      await gitOps.handleAddRepo();

      const branch = repositoriesStore.get("/fresh-repo")?.branches["main"];
      // Must create exactly 1 terminal — not 2 from double-spawn chain
      expect(branch?.terminals.length).toBe(1);
      expect(terminalsStore.state.activeId).toBe(branch?.terminals[0]);
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

  describe("handleAddRepo (browser mode)", () => {
    let originalTauriInternals: unknown;

    beforeEach(() => {
      // Simulate browser mode by removing Tauri internals
      originalTauriInternals = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
      delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
      vi.clearAllMocks();
    });

    afterEach(() => {
      // Restore Tauri internals
      (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauriInternals;
    });

    it("uses promptRepoPath callback instead of window.prompt in browser mode", async () => {
      const promptRepoPath = vi.fn().mockResolvedValue("/browser-repo");
      const browserGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: { ...mockDialogs, promptRepoPath },
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
      });
      mockRepo.getInfo.mockResolvedValue({
        path: "/browser-repo",
        name: "browser-repo",
        initials: "BR",
        branch: "main",
        status: "clean",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });
      mockRepo.getWorktreePaths.mockResolvedValue({ main: "/browser-repo" });

      await browserGitOps.handleAddRepo();

      expect(promptRepoPath).toHaveBeenCalledOnce();
      expect(repositoriesStore.get("/browser-repo")).toBeDefined();
    });

    it("does nothing when promptRepoPath returns null in browser mode", async () => {
      const promptRepoPath = vi.fn().mockResolvedValue(null);
      const browserGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: { ...mockDialogs, promptRepoPath },
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
      });

      await browserGitOps.handleAddRepo();

      expect(mockRepo.getInfo).not.toHaveBeenCalled();
    });
  });

  describe("handleCheckoutRemoteBranch", () => {
    it("calls repo.checkoutRemoteBranch and refreshes branch lists", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.listLocalBranches.mockResolvedValue(["main", "feat-remote"]);
      mockRepo.checkoutRemoteBranch.mockResolvedValue(undefined);

      await gitOps.handleCheckoutRemoteBranch("/repo", "feat-remote");

      expect(mockRepo.checkoutRemoteBranch).toHaveBeenCalledWith("/repo", "feat-remote");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Checked out feat-remote");
    });

    it("reports error when checkout fails", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.checkoutRemoteBranch.mockRejectedValue(new Error("branch already exists"));

      await gitOps.handleCheckoutRemoteBranch("/repo", "feat-remote");

      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Checkout failed"));
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

      expect(errSpy).toHaveBeenCalledWith("[terminal]", "Failed to send run command", expect.any(Error));
      errSpy.mockRestore();
      vi.useRealTimers();
    });
  });
});
