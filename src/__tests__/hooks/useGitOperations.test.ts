import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import "../mocks/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { githubStore } from "../../stores/github";
import type { BranchPrStatus } from "../../types";
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
    getRepoSummary: vi.fn().mockResolvedValue({ worktree_paths: {}, merged_branches: [], diff_stats: {}, last_commit_ts: {} }),
    getRepoStructure: vi.fn().mockResolvedValue({ worktree_paths: {}, merged_branches: [] }),
    getRepoDiffStats: vi.fn().mockResolvedValue({ diff_stats: {}, last_commit_ts: {} }),
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
    detectOrphanWorktrees: vi.fn().mockResolvedValue([]),
    removeOrphanWorktree: vi.fn().mockResolvedValue(undefined),
    mergePrViaGithub: vi.fn().mockResolvedValue("abc123sha"),
    switchBranch: vi.fn().mockResolvedValue({ success: true, stashed: false, previous_branch: "main", new_branch: "feature" }),
    runSetupScript: vi.fn().mockResolvedValue({ exit_code: 0, stdout: "", stderr: "" }),
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
      expect(gitOps.mergePendingCtx()).toEqual({ repoPath: "/repo", branchName: "feature/x", baseBranch: "main" });
    });

    it("dismissMergePending clears the context and keeps branch in sidebar", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");
      gitOps.dismissMergePending();

      // Branch stays — cleanup dialog was skipped
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeDefined();
      expect(gitOps.mergePendingCtx()).toBeNull();
    });

    it("keeps branch and terminals when merge fails", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      repositoriesStore.addTerminalToBranch("/repo", "feature/x", "term-99");
      terminalsStore.register("term-99", { sessionId: null, fontSize: 14, name: "T-99", cwd: "/repo/.wt/x", awaitingInput: null });
      mockRepo.mergeAndArchiveWorktree.mockRejectedValueOnce(new Error("Merge failed (conflicts?)"));

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

      // Branch stays in sidebar
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeDefined();
      // Terminal was NOT closed
      expect(mockCloseTerminal).not.toHaveBeenCalled();
      // Error was reported
      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to merge"));
    });
  });

  describe("handleMergeAndArchive - GitHub API path", () => {
    const testPr: BranchPrStatus = {
      branch: "feature/x",
      number: 99,
      title: "Add feature X",
      state: "OPEN",
      url: "https://github.com/owner/repo/pull/99",
      additions: 10,
      deletions: 5,
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
      check_details: [],
      author: "user",
      commits: 2,
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      review_decision: "APPROVED",
      labels: [],
      is_draft: false,
      base_ref_name: "main",
      head_ref_oid: "abc1234",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-02T00:00:00Z",
      merge_state_label: null,
      review_state_label: null,
    };

    beforeEach(() => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", isMain: true });
      repositoriesStore.setBranch("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
      githubStore.updateRepoData("/repo", [testPr]);
    });

    afterEach(() => {
      githubStore.updateRepoData("/repo", []); // clear PR data
    });

    it("uses GitHub API merge when an open PR exists for the branch", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { prMergeStrategy: "squash" });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

      expect(mockRepo.mergePrViaGithub).toHaveBeenCalledWith("/repo", 99, "squash");
      expect(mockRepo.mergeAndArchiveWorktree).not.toHaveBeenCalled();
      expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/x", "archive");
    });

    it("falls back to local git merge when GitHub API fails", async () => {
      mockRepo.mergePrViaGithub.mockRejectedValueOnce(new Error("no token"));
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

      expect(mockRepo.mergePrViaGithub).toHaveBeenCalled();
      expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalledWith("/repo", "feature/x", "main", "archive");
    });

    it("sets mergePendingCtx when afterMerge=ask with GitHub PR merge", async () => {
      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");

      expect(mockRepo.mergePrViaGithub).toHaveBeenCalled();
      expect(gitOps.mergePendingCtx()).toEqual({ repoPath: "/repo", branchName: "feature/x", baseBranch: "main" });
      expect(repositoriesStore.get("/repo")?.branches["feature/x"]).toBeDefined();
    });

    it("uses local git merge when no PR exists for the branch", async () => {
      githubStore.updateRepoData("/repo", []); // no PR
      mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: null });

      await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

      expect(mockRepo.mergePrViaGithub).not.toHaveBeenCalled();
      expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalled();
    });
  });

  describe("refreshAllBranchStats", () => {
    /** Helper: mock both Phase 1 (structure) and Phase 2 (diff stats) from a single summary object */
    function mockSummary(summary: {
      worktree_paths: Record<string, string>;
      merged_branches: string[];
      diff_stats: Record<string, { additions: number; deletions: number }>;
      last_commit_ts: Record<string, number | null>;
    }) {
      mockRepo.getRepoStructure.mockResolvedValue({
        worktree_paths: summary.worktree_paths,
        merged_branches: summary.merged_branches,
      });
      mockRepo.getRepoDiffStats.mockResolvedValue({
        diff_stats: summary.diff_stats,
        last_commit_ts: summary.last_commit_ts,
      });
    }

    it("updates branch stats for all repos", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockSummary({
        worktree_paths: { main: "/repo" },
        merged_branches: [],
        diff_stats: { "/repo": { additions: 5, deletions: 3 } },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      const branch = repositoriesStore.get("/repo")?.branches["main"];
      expect(branch?.additions).toBe(5);
      expect(branch?.deletions).toBe(3);
    });

    it("prunes branches not in worktree paths", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "stale", { worktreePath: "/repo/stale" });
      mockSummary({
        worktree_paths: { main: "/repo" },
        merged_branches: [],
        diff_stats: { "/repo": { additions: 0, deletions: 0 } },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      expect(repositoriesStore.get("/repo")?.branches["stale"]).toBeUndefined();
      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeDefined();
    });

    it("discovers externally created worktrees", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      // Simulate an external `git worktree add` — new branch appears in summary
      mockSummary({
        worktree_paths: { main: "/repo", "feature-external": "/repo/.worktrees/feature-external" },
        merged_branches: [],
        diff_stats: {
          "/repo": { additions: 2, deletions: 1 },
          "/repo/.worktrees/feature-external": { additions: 2, deletions: 1 },
        },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      const newBranch = repositoriesStore.get("/repo")?.branches["feature-external"];
      expect(newBranch).toBeDefined();
      expect(newBranch?.worktreePath).toBe("/repo/.worktrees/feature-external");
      expect(newBranch?.additions).toBe(2);
      expect(newBranch?.deletions).toBe(1);
    });

    it("removes stale activeBranch when HEAD moved to different branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActiveBranch("/repo", "main");

      mockSummary({
        worktree_paths: { "feature/acme": "/repo" },
        merged_branches: [],
        diff_stats: { "/repo": { additions: 1, deletions: 0 } },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      expect(repo?.branches["main"]).toBeUndefined();
      expect(repo?.branches["feature/acme"]).toBeDefined();
      expect(repo?.activeBranch).toBe("feature/acme");
    });

    it("migrates terminals from stale activeBranch to new worktree branch", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActiveBranch("/repo", "main");
      const tid = terminalsStore.add({ sessionId: "s1", fontSize: 14, name: "T1", cwd: "/repo", awaitingInput: null });
      repositoriesStore.addTerminalToBranch("/repo", "main", tid);

      mockSummary({
        worktree_paths: { "feature/acme": "/repo" },
        merged_branches: [],
        diff_stats: { "/repo": { additions: 0, deletions: 0 } },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      expect(repo?.branches["main"]).toBeUndefined();
      expect(repo?.branches["feature/acme"]?.terminals).toContain(tid);
      expect(repo?.activeBranch).toBe("feature/acme");
    });

    it("handles missing diff stats gracefully (no throw)", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockSummary({
        worktree_paths: { main: "/repo" },
        merged_branches: [],
        diff_stats: {},
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      expect(repositoriesStore.get("/repo")?.branches["main"]).toBeDefined();
    });

    it("stores lastCommitTs converted from seconds to milliseconds", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature-x", { worktreePath: "/repo/wt-feature-x" });

      mockSummary({
        worktree_paths: { main: "/repo", "feature-x": "/repo/wt-feature-x" },
        merged_branches: [],
        diff_stats: {
          "/repo": { additions: 0, deletions: 0 },
          "/repo/wt-feature-x": { additions: 0, deletions: 0 },
        },
        last_commit_ts: { main: 1700000001, "feature-x": 1700000042 },
      });

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      expect(repo?.branches["main"]?.lastCommitTs).toBe(1700000001 * 1000);
      expect(repo?.branches["feature-x"]?.lastCommitTs).toBe(1700000042 * 1000);
    });

    it("stores lastCommitTs as null when backend returns null", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo", lastCommitTs: 999 });

      mockSummary({
        worktree_paths: { main: "/repo" },
        merged_branches: [],
        diff_stats: { "/repo": { additions: 0, deletions: 0 } },
        last_commit_ts: { main: null },
      });

      await gitOps.refreshAllBranchStats();

      expect(repositoriesStore.get("/repo")?.branches["main"]?.lastCommitTs).toBeNull();
    });

  });

  describe("refreshAllBranchStats — progressive loading", () => {
    it("Phase 1 updates worktreePath before Phase 2 runs", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      let structureCallOrder = 0;
      let diffStatsCallOrder = 0;
      let callCounter = 0;

      mockRepo.getRepoStructure.mockImplementation(async () => {
        structureCallOrder = ++callCounter;
        return {
          worktree_paths: { main: "/repo", "feature-new": "/repo/wt-new" },
          merged_branches: [],
        };
      });
      mockRepo.getRepoDiffStats.mockImplementation(async () => {
        diffStatsCallOrder = ++callCounter;
        // By now, Phase 1 should have already updated the store
        const repo = repositoriesStore.get("/repo");
        expect(repo?.branches["feature-new"]?.worktreePath).toBe("/repo/wt-new");
        return {
          diff_stats: {
            "/repo": { additions: 1, deletions: 0 },
            "/repo/wt-new": { additions: 3, deletions: 2 },
          },
          last_commit_ts: { main: 1700000001, "feature-new": 1700000042 },
        };
      });

      await gitOps.refreshAllBranchStats();

      expect(structureCallOrder).toBeLessThan(diffStatsCallOrder);
      const repo = repositoriesStore.get("/repo");
      expect(repo?.branches["feature-new"]?.additions).toBe(3);
      expect(repo?.branches["feature-new"]?.deletions).toBe(2);
    });

    it("Phase 2 failure does not corrupt Phase 1 state", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      mockRepo.getRepoStructure.mockResolvedValue({
        worktree_paths: { main: "/repo", "feature-a": "/repo/wt-a" },
        merged_branches: ["feature-a"],
      });
      mockRepo.getRepoDiffStats.mockRejectedValue(new Error("git diff failed"));

      await gitOps.refreshAllBranchStats();

      const repo = repositoriesStore.get("/repo");
      // Phase 1 state should be intact
      expect(repo?.branches["feature-a"]?.worktreePath).toBe("/repo/wt-a");
      expect(repo?.branches["feature-a"]?.isMerged).toBe(true);
      // Stats should be at defaults (Phase 2 failed)
      expect(repo?.branches["feature-a"]?.additions).toBe(0);
    });

    it("auto-archive runs after Phase 1, before Phase 2", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { autoArchiveMerged: true });

      let archiveCalledBeforeDiffStats = false;

      mockRepo.getRepoStructure.mockResolvedValue({
        worktree_paths: { main: "/repo", "merged-branch": "/repo/wt-merged" },
        merged_branches: ["merged-branch"],
      });
      mockRepo.finalizeMergedWorktree.mockImplementation(async () => {
        archiveCalledBeforeDiffStats = !mockRepo.getRepoDiffStats.mock.calls.length;
        return { merged: true, action: "archived", archive_path: null };
      });
      mockRepo.getRepoDiffStats.mockResolvedValue({
        diff_stats: { "/repo": { additions: 0, deletions: 0 } },
        last_commit_ts: {},
      });

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "merged-branch", "archive");
      expect(archiveCalledBeforeDiffStats).toBe(true);
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

    it("runs setupScript after worktree creation", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { setupScript: "npm install" });

      mockRepo.createWorktree.mockResolvedValue({
        name: "feat-test",
        path: "/repo/wt/feat-test",
        branch: "feat-test",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "feat-test", createBranch: true, baseRef: "main" });

      expect(mockRepo.runSetupScript).toHaveBeenCalledWith("npm install", "/repo/wt/feat-test");
    });

    it("does not run setupScript when empty", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

      mockRepo.createWorktree.mockResolvedValue({
        name: "feat-test",
        path: "/repo/wt/feat-test",
        branch: "feat-test",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "feat-test", createBranch: true, baseRef: "main" });

      expect(mockRepo.runSetupScript).not.toHaveBeenCalled();
    });

    it("sets pendingInitCommand from runScript", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { runScript: "npm run dev" });

      mockRepo.createWorktree.mockResolvedValue({
        name: "feat-test",
        path: "/repo/wt/feat-test",
        branch: "feat-test",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "feat-test", createBranch: true, baseRef: "main" });

      // Find the terminal created for this worktree
      const branch = repositoriesStore.get("/repo")?.branches["feat-test"];
      expect(branch?.terminals.length).toBeGreaterThan(0);
      const termId = branch!.terminals[0];
      const terminal = terminalsStore.get(termId);
      expect(terminal?.pendingInitCommand).toBe("npm run dev");
    });

    it("warns but continues when setupScript fails", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { setupScript: "exit 1" });

      mockRepo.createWorktree.mockResolvedValue({
        name: "feat-test",
        path: "/repo/wt/feat-test",
        branch: "feat-test",
        base_repo: "/repo",
      });
      mockRepo.runSetupScript.mockResolvedValue({ exit_code: 1, stdout: "", stderr: "failed" });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleAddWorktree("/repo");
      await gitOps.confirmCreateWorktree({ branchName: "feat-test", createBranch: true, baseRef: "main" });

      // Should still create a terminal despite script failure
      const branch = repositoriesStore.get("/repo")?.branches["feat-test"];
      expect(branch?.terminals.length).toBeGreaterThan(0);
      // Should warn about failure
      expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Setup script failed"));
    });
  });

  describe("handleCreateWorktreeFromBranch", () => {
    it("runs setupScript after clone-worktree creation", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { setupScript: "npm ci" });

      mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
      mockRepo.createWorktree.mockResolvedValue({
        name: "main--wt-42",
        path: "/repo/wt/main--wt-42",
        branch: "main--wt-42",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

      expect(mockRepo.runSetupScript).toHaveBeenCalledWith("npm ci", "/repo/wt/main--wt-42");
    });

    it("sets pendingInitCommand from runScript on clone-worktree", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { runScript: "make dev" });

      mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
      mockRepo.createWorktree.mockResolvedValue({
        name: "main--wt-42",
        path: "/repo/wt/main--wt-42",
        branch: "main--wt-42",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

      const branch = repositoriesStore.get("/repo")?.branches["main--wt-42"];
      expect(branch?.terminals.length).toBeGreaterThan(0);
      const termId = branch!.terminals[0];
      const terminal = terminalsStore.get(termId);
      expect(terminal?.pendingInitCommand).toBe("make dev");
    });

    it("does not run scripts when none configured", async () => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");

      mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
      mockRepo.createWorktree.mockResolvedValue({
        name: "main--wt-42",
        path: "/repo/wt/main--wt-42",
        branch: "main--wt-42",
        base_repo: "/repo",
      });
      mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

      await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

      expect(mockRepo.runSetupScript).not.toHaveBeenCalled();
      const branch = repositoriesStore.get("/repo")?.branches["main--wt-42"];
      const termId = branch!.terminals[0];
      expect(terminalsStore.get(termId)?.pendingInitCommand).toBeNull();
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
      mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: { main: "/new-repo" }, merged_branches: [] });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

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
      mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: { develop: "/array-repo" }, merged_branches: [] });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

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
      mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: { main: "/fresh-repo" }, merged_branches: [] });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

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
      mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: { main: "/new-repo" }, merged_branches: [] });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

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

  describe("orphan worktree cleanup", () => {
    beforeEach(() => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: { main: "/repo" }, merged_branches: [] });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
    });

    it("auto-removes orphans silently when orphanCleanup=on", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { orphanCleanup: "on" });
      mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/detached-1");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Removed 1 orphaned worktree(s)");
    });

    it("asks user before removing when orphanCleanup=ask and user confirms", async () => {
      const confirmOrphanCleanup = vi.fn().mockResolvedValue(true);
      const askGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: { ...mockDialogs, confirmOrphanCleanup },
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
      });
      // orphanCleanup defaults to "ask" when no per-repo override
      mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

      await askGitOps.refreshAllBranchStats();

      expect(confirmOrphanCleanup).toHaveBeenCalledWith(["/wt/detached-1"]);
      expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/detached-1");
    });

    it("skips removal when orphanCleanup=ask and user cancels", async () => {
      const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
      const askGitOps = useGitOperations({
        repo: mockRepo,
        pty: mockPty,
        dialogs: { ...mockDialogs, confirmOrphanCleanup },
        closeTerminal: mockCloseTerminal,
        createNewTerminal: mockCreateNewTerminal,
        setStatusInfo: mockSetStatusInfo,
        getDefaultFontSize: () => 14,
        getMaxTabNameLength: () => 25,
      });
      mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

      await askGitOps.refreshAllBranchStats();

      expect(confirmOrphanCleanup).toHaveBeenCalled();
      expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
    });

    it("does nothing when orphanCleanup=off", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { orphanCleanup: "off" });
      mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.detectOrphanWorktrees).not.toHaveBeenCalled();
      expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
    });

    it("does nothing when no orphans found", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { orphanCleanup: "on" });
      mockRepo.detectOrphanWorktrees.mockResolvedValue([]);

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("orphaned"));
    });
  });

  describe("auto-archive merged worktrees", () => {
    beforeEach(() => {
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      mockRepo.getRepoStructure.mockResolvedValue({
        worktree_paths: { main: "/repo", "feature/x": "/repo/.worktrees/feature-x" },
        merged_branches: ["feature/x"],
      });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
    });

    it("archives merged linked worktrees when autoArchiveMerged=true", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { autoArchiveMerged: true });

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/x", "archive");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Auto-archived 1 merged worktree(s)");
    });

    it("does nothing when autoArchiveMerged=false", async () => {
      // Default is false — no setting override needed
      await gitOps.refreshAllBranchStats();

      expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Auto-archived"));
    });

    it("skips the main worktree even when it reports as merged", async () => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { autoArchiveMerged: true });
      // main branch worktreePath === repoPath → must be skipped
      mockRepo.getRepoStructure.mockResolvedValue({
        worktree_paths: { main: "/repo" },
        merged_branches: ["main"],
      });
      mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

      await gitOps.refreshAllBranchStats();

      expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
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

  describe("handleTerminalCwdChange (OSC 7)", () => {
    const addTerminal = (opts: { sessionId: string | null; cwd: string; name?: string }) => {
      return terminalsStore.add({
        sessionId: opts.sessionId,
        fontSize: 14,
        name: opts.name ?? "T",
        cwd: opts.cwd,
        awaitingInput: null,
      });
    };

    beforeEach(() => {
      vi.useFakeTimers();
      // Set up repo with main branch and a worktree branch
      repositoriesStore.add({ path: "/repo", displayName: "Repo" });
      repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
      repositoriesStore.setBranch("/repo", "feature-x", { worktreePath: "/repo/.worktrees/feature-x" });
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("reassigns terminal from main to worktree branch on cwd change", async () => {
      const id = addTerminal({ sessionId: "s1", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);
      terminalsStore.setActive(id);

      gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
      await vi.advanceTimersByTimeAsync(300);

      const main = repositoriesStore.get("/repo")?.branches["main"];
      const feature = repositoriesStore.get("/repo")?.branches["feature-x"];
      expect(main?.terminals).not.toContain(id);
      expect(feature?.terminals).toContain(id);
    });

    it("does nothing when cwd maps to the same branch", async () => {
      const id = addTerminal({ sessionId: "s2", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      gitOps.handleTerminalCwdChange(id, "/repo/src/deep/folder");
      await vi.advanceTimersByTimeAsync(300);

      // Still on main — /repo/src is a subdirectory of /repo (main's worktreePath)
      const main = repositoriesStore.get("/repo")?.branches["main"];
      expect(main?.terminals).toContain(id);
    });

    it("longest prefix wins when worktrees nest", async () => {
      const id = addTerminal({ sessionId: "s3", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);
      terminalsStore.setActive(id);

      // cwd inside the feature-x worktree subdirectory
      gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x/src/components");
      await vi.advanceTimersByTimeAsync(300);

      const feature = repositoriesStore.get("/repo")?.branches["feature-x"];
      expect(feature?.terminals).toContain(id);
    });

    it("does not match repo-old when cwd is /repo (boundary check)", async () => {
      // Add a second repo whose path is a string-prefix of "/repo" but not a path-prefix
      repositoriesStore.add({ path: "/repo-old", displayName: "RepoOld" });
      repositoriesStore.setBranch("/repo-old", "main", { worktreePath: "/repo-old" });
      const id = addTerminal({ sessionId: "s4", cwd: "/repo-old" });
      repositoriesStore.addTerminalToBranch("/repo-old", "main", id);

      // cwd "/repo" should NOT match "/repo-old" (the "/" boundary guard prevents it)
      gitOps.handleTerminalCwdChange(id, "/repo");
      await vi.advanceTimersByTimeAsync(300);

      // Terminal should have moved away from /repo-old
      const repoOldMain = repositoriesStore.get("/repo-old")?.branches["main"];
      expect(repoOldMain?.terminals).not.toContain(id);
    });

    it("does nothing for cwd outside all known repos", async () => {
      const id = addTerminal({ sessionId: "s5", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      gitOps.handleTerminalCwdChange(id, "/tmp/random/path");
      await vi.advanceTimersByTimeAsync(300);

      // Should still be on main — no reassignment
      const main = repositoriesStore.get("/repo")?.branches["main"];
      expect(main?.terminals).toContain(id);
    });

    it("debounces rapid cwd changes — only the last one takes effect", async () => {
      const id = addTerminal({ sessionId: "s6", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);
      terminalsStore.setActive(id);

      // Rapid fire: main → feature-x → main
      gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
      gitOps.handleTerminalCwdChange(id, "/repo");
      await vi.advanceTimersByTimeAsync(300);

      // Should end up on main (last cwd wins)
      const main = repositoriesStore.get("/repo")?.branches["main"];
      expect(main?.terminals).toContain(id);
    });

    it("does not crash when terminal was closed during debounce window", async () => {
      const id = addTerminal({ sessionId: "s7", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);

      gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
      // Close terminal before debounce fires
      repositoriesStore.removeTerminalFromBranch("/repo", "main", id);
      terminalsStore.remove(id);

      // Should not throw
      await vi.advanceTimersByTimeAsync(300);
    });

    it("cancelCwdTracking cancels pending debounce timer", async () => {
      const id = addTerminal({ sessionId: "s8", cwd: "/repo" });
      repositoriesStore.addTerminalToBranch("/repo", "main", id);
      terminalsStore.setActive(id);

      gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
      gitOps.cancelCwdTracking(id);
      await vi.advanceTimersByTimeAsync(300);

      // Timer was cancelled — terminal should still be on main
      const main = repositoriesStore.get("/repo")?.branches["main"];
      expect(main?.terminals).toContain(id);
    });
  });
});
