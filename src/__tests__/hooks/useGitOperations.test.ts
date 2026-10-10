import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { createRepositoryRefreshCoordinator } from "../../hooks/git/createRepositoryRefreshCoordinator";
import { buildAgentSeed, useGitOperations } from "../../hooks/useGitOperations";
import * as platform from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { getForRepo as getFocusForRepo, recordTerminalRepo } from "../../stores/focusRegistry";
import { githubStore } from "../../stores/github";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import type { BranchPrStatus } from "../../types";
import { openDialog as open } from "../../utils/nativeDialog";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { branchFacts, STALE_AFTER_DAYS } from "../../utils/sidebarRich";
import { makeTerminal } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

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
	editorTabsStore.clearAll();
	diffTabsStore.clearAll();
	mdTabsStore.clearAll();
}

function defaultInvoke(cmd: string): Promise<unknown> {
	if (cmd === "load_agents_config") return Promise.resolve({ agents: {} });
	if (cmd === "run_git_command") return Promise.resolve({ stdout: "", stderr: "" });
	if (cmd === "check_worktree_dirty") return Promise.resolve(false);
	return Promise.resolve(undefined);
}

/** Build the id-keyed workspace map the backend now returns from a plain
 *  branch -> path object. Under the identity migration a git worktree's
 *  workspace id IS its branch, so the key is reused as the id and the branch
 *  travels as a field on the value (#726-5ac7). */
function wtPaths(byBranch: Record<string, string>): Record<string, { branch: string; path: string; kind: "worktree" }> {
	return Object.fromEntries(
		Object.entries(byBranch).map(([branch, path]) => [branch, { branch, path, kind: "worktree" }]),
	);
}

describe("buildAgentSeed", () => {
	let isWindowsSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		mockInvoke.mockImplementation(defaultInvoke);
		// Installed fresh per test and fully restored in afterEach so the mock
		// cannot leak into later describes (e.g. handleConflictAssist's POSIX
		// quoting assertions) — a plain mockReset() left the spy installed.
		isWindowsSpy = vi.spyOn(platform, "isWindows").mockReturnValue(false);
	});

	afterEach(() => {
		isWindowsSpy.mockRestore();
	});

	it("wraps the prompt as a POSIX single-quoted argument to the launch command", async () => {
		isWindowsSpy.mockReturnValue(false);
		const seed = await buildAgentSeed("fix the bug");
		expect(seed.initCommand).toBe(`${seed.launchCommand} 'fix the bug'`);
		expect(seed.agentType).toBe("claude");
	});

	it("escapes single quotes in the prompt as '\\''", async () => {
		isWindowsSpy.mockReturnValue(false);
		const seed = await buildAgentSeed("hello 'world'");
		// Each ' becomes '\'' — close quote, escaped quote, reopen quote.
		expect(seed.initCommand).toBe(`${seed.launchCommand} 'hello '\\''world'\\'''`);
	});

	it("keeps launchCommand bare (no embedded prompt) so it can be reused for resume", async () => {
		isWindowsSpy.mockReturnValue(false);
		const seed = await buildAgentSeed("multi\nline prompt");
		expect(seed.initCommand.startsWith(`${seed.launchCommand} `)).toBe(true);
		expect(seed.launchCommand).not.toContain("multi");
	});

	it("wraps the prompt as a Windows double-quoted argument on cmd.exe", async () => {
		isWindowsSpy.mockReturnValue(true);
		const seed = await buildAgentSeed("fix the bug");
		expect(seed.initCommand).toBe(`${seed.launchCommand} "fix the bug"`);
	});
});

describe("useGitOperations", () => {
	const mockRepo = {
		getInfo: vi.fn(),
		getDiffStats: vi.fn().mockResolvedValue({ additions: 0, deletions: 0 }),
		getWorktreePaths: vi.fn().mockResolvedValue({}),
		getRepoSummary: vi
			.fn()
			.mockResolvedValue({ worktree_paths: wtPaths({}), merged_branches: [], diff_stats: {}, last_commit_ts: {} }),
		getRepoStructure: vi.fn().mockResolvedValue({ worktree_paths: wtPaths({}), merged_branches: [] }),
		getRepoDiffStats: vi.fn().mockResolvedValue({ diff_stats: {}, last_commit_ts: {} }),
		removeWorktree: vi.fn().mockResolvedValue(undefined),
		createWorktree: vi.fn(),
		renameBranch: vi.fn().mockResolvedValue(undefined),
		createBranch: vi.fn().mockResolvedValue(undefined),
		generateWorktreeName: vi.fn().mockResolvedValue("bold-nexus-042"),
		generateCloneBranchName: vi.fn().mockResolvedValue("feat-auth--bold-nexus-042"),
		listBaseRefOptions: vi.fn().mockResolvedValue([{ name: "main", kind: "local", is_default: true }]),
		mergeAndArchiveWorktree: vi.fn().mockResolvedValue({ merged: true, action: "archived", archive_path: null }),
		finalizeMergedWorktree: vi.fn().mockResolvedValue({ merged: true, action: "archived", archive_path: null }),
		listLocalBranches: vi.fn().mockResolvedValue(["main"]),
		getMergedBranches: vi.fn().mockResolvedValue(["main"]),
		checkoutRemoteBranch: vi.fn().mockResolvedValue(undefined),
		detectOrphanWorktrees: vi.fn().mockResolvedValue([]),
		assessOrphanCleanup: vi.fn(),
		beginOrphanCleanup: vi.fn().mockResolvedValue(undefined),
		pendingOrphanCleanupAnswer: vi.fn().mockResolvedValue(null),
		clearOrphanCleanup: vi.fn().mockResolvedValue(undefined),
		removeOrphanWorktree: vi.fn().mockResolvedValue(undefined),
		mergePrViaGithub: vi.fn().mockResolvedValue("abc123sha"),
		switchBranch: vi
			.fn()
			.mockResolvedValue({ success: true, stashed: false, previous_branch: "main", new_branch: "feature" }),
		runSetupScript: vi.fn().mockResolvedValue({ exit_code: 0, stdout: "", stderr: "" }),
		getWorkspaceLifecycle: vi.fn().mockResolvedValue({
			dirtyFingerprint: "confirmed-worktree",
			dirtyFiles: 0,
			commitStatus: "merged",
			removalSafety: "safe",
		}),
	};

	const mockPty = {
		canSpawn: vi.fn().mockResolvedValue(true),
		write: vi.fn().mockResolvedValue(undefined),
		getWorktreesDir: vi.fn().mockResolvedValue("/repos/.worktrees"),
	};

	const mockDialogs = {
		confirmRemoveRepo: vi.fn().mockResolvedValue(true),
		confirmRemoveWorktree: vi.fn().mockResolvedValue(true),
		confirmRemoveLockedWorktree: vi.fn().mockResolvedValue(true),
		confirmStashAndSwitch: vi.fn().mockResolvedValue(true),
		confirmDirtyWorktreeCleanup: vi.fn().mockResolvedValue(true),
		reportGitError: vi.fn().mockResolvedValue(false),
	};

	const mockCloseTerminal = vi.fn().mockResolvedValue(undefined);
	const mockCreateNewTerminal = vi.fn().mockResolvedValue("term-new");
	const mockSetStatusInfo = vi.fn();

	let gitOps: ReturnType<typeof useGitOperations>;

	beforeEach(() => {
		vi.useFakeTimers();
		resetStores();
		paneLayoutStore.reset();
		resetGroupCounter();
		vi.clearAllMocks();
		// Linked worktrees under /repo/ are gone from disk unless a test says otherwise.
		mockRepo.getInfo.mockReset();
		mockRepo.getInfo.mockImplementation(async (path: string) => {
			if (path.startsWith("/repo/")) return { branch: "", is_git_repo: false };
			throw new Error(`unmocked getInfo(${path})`);
		});
		mockRepo.pendingOrphanCleanupAnswer.mockResolvedValue(null);
		mockRepo.assessOrphanCleanup.mockImplementation(async (repoPath: string) =>
			(await mockRepo.detectOrphanWorktrees(repoPath)).map((path: string) => ({ path, safe: true })),
		);
		mockPty.canSpawn.mockResolvedValue(true);
		mockDialogs.confirmRemoveRepo.mockResolvedValue(true);
		mockDialogs.confirmRemoveWorktree.mockResolvedValue(true);
		mockDialogs.confirmRemoveLockedWorktree.mockResolvedValue(true);
		mockDialogs.confirmStashAndSwitch.mockResolvedValue(true);
		mockDialogs.reportGitError.mockResolvedValue(false);
		// The post-merge cleanup dialog asks two dirtiness questions before it opens.
		// Both fail SAFE (an unanswered question reads as dirty), so a bare
		// `resolves undefined` mock would make every ask-mode test look dirty.
		mockInvoke.mockImplementation(defaultInvoke);
		mockRepo.switchBranch.mockResolvedValue({
			success: true,
			stashed: false,
			previous_branch: "main",
			new_branch: "feature",
		});

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

	afterEach(() => {
		vi.useRealTimers();
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	describe("handleNewTab (#81 — Cmd+T / palette / File>New Tab path)", () => {
		it("registers the new terminal in the active branch.terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			await gitOps.handleBranchSelect("/repo", "main"); // auto-spawns the first terminal

			const before = repositoriesStore.get("/repo")?.workspaces["main"]?.terminals.length ?? 0;
			await gitOps.handleNewTab();

			// The bug: Cmd+T routed to createNewTerminal, which never added the id to
			// branch.terminals, so the tab only appeared after a worktree switch.
			const branch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(branch?.terminals.length).toBe(before + 1);
			expect(branch?.terminals).toContain(terminalsStore.state.activeId);
		});

		it("falls back to createNewTerminal when no repo is active", async () => {
			mockCreateNewTerminal.mockClear();
			await gitOps.handleNewTab();
			expect(mockCreateNewTerminal).toHaveBeenCalledTimes(1);
		});
	});

	describe("handleBranchSelect", () => {
		it("sets active repo and branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			await gitOps.handleBranchSelect("/repo", "main");

			expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("main");
			expect(gitOps.currentRepoPath()).toBe("/repo");
			expect(gitOps.currentBranch()).toBe("main");
		});

		it("derives the branch after an agent-row navigation switches repositories", async () => {
			// Clicking an agent row in the sidebar goes through navigateToTerminal,
			// which only calls repositoriesStore.setActive. The side panels read
			// currentRepoPath, so it must follow the store, or Notes, Git and Files
			// keep showing the previous repo.
			repositoriesStore.add({ path: "/a", displayName: "A" });
			repositoriesStore.setWorkspace("/a", "main", { worktreePath: "/a" });
			repositoriesStore.add({ path: "/b", displayName: "B" });
			repositoriesStore.setWorkspace("/b", "feature/b", { worktreePath: "/b" });
			await gitOps.handleBranchSelect("/a", "main");
			const agentTab = terminalsStore.add({
				sessionId: "sess-b",
				fontSize: 14,
				name: "Agent in B",
				cwd: "/b",
				awaitingInput: null,
			});
			repositoriesStore.addTerminalToWorkspace("/b", "feature/b", agentTab);

			navigateToTerminal(agentTab);

			expect(gitOps.currentRepoPath()).toBe("/b");
			expect(gitOps.currentBranch()).toBe("feature/b");
		});

		it("derives the branch after navigation to a linked-worktree terminal", () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature/worktree", { worktreePath: "/repo/.worktrees/feature" });
			const worktreeTerminal = terminalsStore.add({
				sessionId: "worktree-session",
				fontSize: 14,
				name: "Worktree terminal",
				cwd: "/repo/.worktrees/feature",
				awaitingInput: null,
			});
			repositoriesStore.addTerminalToWorkspace("/repo", "feature/worktree", worktreeTerminal);

			navigateToTerminal(worktreeTerminal);

			expect(gitOps.currentBranch()).toBe("feature/worktree");
		});

		it("does not retain a deleted active branch", () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature/deleted", { worktreePath: "/repo/.worktrees/deleted" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "feature/deleted");

			repositoriesStore.removeWorkspace("/repo", "feature/deleted");

			expect(gitOps.currentBranch()).toBe("main");
		});

		it("serializes 3+ concurrent selects (no overlapping inner runs)", async () => {
			// Regression: the old single-in-flight-promise guard let 3+ concurrent
			// callers all wake from the SAME promise and run handleBranchSelectInner
			// simultaneously (duplicate terminals, pane-layout races). The FIFO queue
			// must run them strictly one at a time.
			//
			// Each fresh-branch select awaits handleAddTerminalToWorkspace → pty.canSpawn(),
			// so canSpawn is the inner's yield point. Instrument it to detect overlap:
			// with the bug, the three inner runs interleave here and maxActive reaches >1.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			for (const b of ["b1", "b2", "b3"]) {
				repositoriesStore.setWorkspace("/repo", b, { worktreePath: `/repo/wt-${b}` });
			}

			let active = 0;
			let maxActive = 0;
			mockPty.canSpawn.mockImplementation(async () => {
				active++;
				maxActive = Math.max(maxActive, active);
				await Promise.resolve();
				await Promise.resolve();
				active--;
				return true;
			});

			// Fire three WITHOUT awaiting in between → they pile up concurrently.
			const all = Promise.all([
				gitOps.handleBranchSelect("/repo", "b1"),
				gitOps.handleBranchSelect("/repo", "b2"),
				gitOps.handleBranchSelect("/repo", "b3"),
			]);
			await vi.runAllTimersAsync();
			await all;

			expect(maxActive).toBe(1); // never ran two inner selects at once
			// All three selects completed and each spawned exactly one terminal.
			for (const b of ["b1", "b2", "b3"]) {
				expect(repositoriesStore.get("/repo")?.workspaces[b]?.terminals.length).toBe(1);
			}
		});

		it("auto-spawns terminal on first branch select", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			await gitOps.handleBranchSelect("/repo", "feature");

			// First time → should auto-create a terminal
			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			expect(branch?.terminals.length).toBeGreaterThan(0);
			expect(branch?.hadTerminals).toBe(true);
		});

		it("does not auto-spawn after user closed all terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt", hadTerminals: true });

			await gitOps.handleBranchSelect("/repo", "feature");

			// hadTerminals is true but no live terminals → show empty state, no spawn
			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			expect(branch?.terminals.length).toBe(0);
		});

		it("clears activeId when switching to branch with no terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "develop", { worktreePath: "/repo/wt-dev", hadTerminals: true });

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
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [
					{ name: "Terminal 1", cwd: "/repo/wt", fontSize: 14, agentType: "claude" },
					{ name: "Agent", cwd: "/repo/wt", fontSize: 12, agentType: "claude" },
				],
			});

			await gitOps.handleBranchSelect("/repo", "feature");

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			expect(branch?.terminals.length).toBe(2);
			// savedTerminals should be consumed
			expect(branch?.savedTerminals?.length).toBe(0);
			// First restored terminal should be active
			expect(terminalsStore.state.activeId).toBe(branch?.terminals[0]);
		});

		it("skips plain shell tabs and spawns fresh terminal on restore", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [
					{ name: "Shell 1", cwd: "/repo/wt", fontSize: 14, agentType: null },
					{ name: "Shell 2", cwd: "/repo/wt", fontSize: 12, agentType: null },
				],
			});

			await gitOps.handleBranchSelect("/repo", "feature");

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			// Plain shell tabs filtered out → fresh terminal spawned
			expect(branch?.terminals.length).toBe(1);
			expect(branch?.savedTerminals?.length).toBe(0);
		});

		it("preserves terminal metadata during lazy restore", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [{ name: "My Terminal", cwd: "/custom/path", fontSize: 16, agentType: "claude" }],
			});

			await gitOps.handleBranchSelect("/repo", "feature");

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			const termId = branch?.terminals[0];
			const terminal = termId ? terminalsStore.get(termId) : undefined;
			expect(terminal?.name).toBe("My Terminal");
			expect(terminal?.cwd).toBe("/custom/path");
			expect(terminal?.fontSize).toBe(16);
			expect(terminal?.sessionId).toBeNull();
		});

		it("preserves agentLaunchCommand during lazy restore", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [
					{
						name: "Claude Agent",
						cwd: "/repo/wt",
						fontSize: 14,
						agentType: "claude",
						agentLaunchCommand: "claude --project /repo/wt --model opus",
					},
				],
			});

			await gitOps.handleBranchSelect("/repo", "feature");

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			const termId = branch?.terminals[0];
			const terminal = termId ? terminalsStore.get(termId) : undefined;
			expect(terminal?.agentLaunchCommand).toBe("claude --project /repo/wt --model opus");
		});

		it("remaps disk-restored pane layout terminal IDs during lazy restore", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			// Simulate old terminal IDs in the branch (from before app restart)
			const oldTermId1 = "old-term-1";
			const oldTermId2 = "old-term-2";
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				terminals: [oldTermId1, oldTermId2],
				savedTerminals: [
					{ name: "Term 1", cwd: "/repo/wt", fontSize: 14, agentType: null },
					{ name: "Term 2", cwd: "/repo/wt", fontSize: 14, agentType: null },
				],
			});

			// Simulate a disk-restored pane layout referencing old terminal IDs
			resetGroupCounter();
			paneLayoutStore.restore({
				root: {
					type: "branch",
					direction: "horizontal",
					children: [
						{ type: "leaf", id: "g1" },
						{ type: "leaf", id: "g2" },
					],
					ratios: [0.5, 0.5],
				},
				groups: {
					g1: { id: "g1", tabs: [{ id: oldTermId1, type: "terminal" }], activeTabId: oldTermId1 },
					g2: { id: "g2", tabs: [{ id: oldTermId2, type: "terminal" }], activeTabId: oldTermId2 },
				},
				activeGroupId: "g1",
			});
			// Test remapTerminalIds directly
			const serializedBefore = paneLayoutStore.serialize();
			expect(serializedBefore.groups["g1"].tabs[0].id).toBe(oldTermId1);
			expect(serializedBefore.groups["g2"].tabs[0].id).toBe(oldTermId2);

			const idMap = new Map([
				[oldTermId1, "new-term-1"],
				[oldTermId2, "new-term-2"],
			]);
			paneLayoutStore.remapTerminalIds(idMap);

			const serializedAfter = paneLayoutStore.serialize();
			expect(serializedAfter.groups["g1"].tabs[0].id).toBe("new-term-1");
			expect(serializedAfter.groups["g1"].activeTabId).toBe("new-term-1");
			expect(serializedAfter.groups["g2"].tabs[0].id).toBe("new-term-2");
			expect(serializedAfter.groups["g2"].activeTabId).toBe("new-term-2");
			// Tree structure preserved
			expect(serializedAfter.root).toEqual(serializedBefore.root);

			paneLayoutStore.reset();
		});

		it("sets pendingResumeCommand on restored agent terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [
					{ name: "Claude", cwd: "/repo/wt", fontSize: 14, agentType: "claude" },
					{ name: "Plain", cwd: "/repo/wt", fontSize: 14, agentType: null },
				],
			});

			await gitOps.handleBranchSelect("/repo", "feature");

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			// Only the agent tab is restored (plain shell filtered out)
			expect(branch?.terminals.length).toBe(1);
			// Resume verification is a deliberate non-blocking second pass
			// (it asks the backend for launch arguments), so wait for it.
			await vi.waitFor(() =>
				expect(terminalsStore.get(branch!.terminals[0])?.pendingResumeCommand).toBe("claude --continue"),
			);
		});

		it("does not restore savedTerminals when live terminals exist", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", {
				worktreePath: "/repo",
				savedTerminals: [{ name: "Saved", cwd: "/repo", fontSize: 14, agentType: null }],
			});

			// Add a live terminal
			const id = terminalsStore.add({
				sessionId: "sess-1",
				fontSize: 14,
				name: "Live",
				cwd: "/repo",
				awaitingInput: null,
			});
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);

			await gitOps.handleBranchSelect("/repo", "main");

			// Should activate the live terminal, not restore from saved
			expect(terminalsStore.state.activeId).toBe(id);
			const branch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(branch?.terminals.length).toBe(1);
		});

		it("activates existing terminal when branch has one", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			const id = terminalsStore.add({
				sessionId: null,
				fontSize: 14,
				name: "Existing",
				cwd: "/repo",
				awaitingInput: null,
			});
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);

			await gitOps.handleBranchSelect("/repo", "main");

			expect(terminalsStore.state.activeId).toBe(id);
		});

		it("preserves active tab when re-clicking the same branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			const id1 = terminalsStore.add(makeTerminal({ name: "T1", cwd: "/repo" }));
			const id2 = terminalsStore.add(makeTerminal({ name: "T2", cwd: "/repo" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id1);
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id2);

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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt/feature" });

			// Branch main: 2 terminals
			const m1 = terminalsStore.add(makeTerminal({ name: "M1", cwd: "/repo" }));
			const m2 = terminalsStore.add(makeTerminal({ name: "M2", cwd: "/repo" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "main", m1);
			repositoriesStore.addTerminalToWorkspace("/repo", "main", m2);

			// Branch feature: 1 terminal
			const f1 = terminalsStore.add(makeTerminal({ name: "F1", cwd: "/repo/wt/feature" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "feature", f1);

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

		it("persists and restores pane layout across branch switches", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			// Start on main — auto-spawns a terminal
			await gitOps.handleBranchSelect("/repo", "main");
			const t1 = terminalsStore.state.activeId!;
			expect(t1).toBeTruthy();

			// Create a split layout on main
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: t1, type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: g1 });
			paneLayoutStore.setActiveGroup(g1);
			paneLayoutStore.split(g1, "vertical");
			expect(paneLayoutStore.isSplit()).toBe(true);

			// Switch to feature — layout should be saved, reset on feature
			await gitOps.handleBranchSelect("/repo", "feature");
			expect(paneLayoutStore.isSplit()).toBe(false);

			// Switch back to main — layout should be restored
			await gitOps.handleBranchSelect("/repo", "main");
			expect(paneLayoutStore.isSplit()).toBe(true);
			expect(paneLayoutStore.getAllGroupIds()).toContain(g1);
		});

		it("clears branchSwitching even when auto-spawn throws (story 1281-a37d)", async () => {
			// Regression: before the try/finally wrap, any throw inside
			// handleBranchSelectInner (e.g. pty.canSpawn rejecting, close_pty failing,
			// resume-command verification bubbling a sync error) stranded the
			// branchSwitching flag at true. The TabBar reads that flag and shows
			// the previous repo's tabs until the app restarts.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt-feature" });
			// Brand-new branch with no terminals triggers the auto-spawn path at the
			// end of handleBranchSelectInner, which awaits handleAddTerminalToWorkspace.
			mockPty.canSpawn.mockRejectedValueOnce(new Error("pty boom"));

			await expect(gitOps.handleBranchSelect("/repo", "feature")).rejects.toThrow("pty boom");

			expect(repositoriesStore.state.branchSwitching).toBe(false);
		});

		it("serializes concurrent calls — no duplicate terminals from savedTerminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				worktreePath: "/repo/wt",
				hadTerminals: true,
				savedTerminals: [{ name: "Claude", cwd: "/repo/wt", fontSize: 14, agentType: "claude" }],
			});

			// Fire two selects concurrently — the second must wait for the first
			const p1 = gitOps.handleBranchSelect("/repo", "feature");
			const p2 = gitOps.handleBranchSelect("/repo", "feature");
			await Promise.all([p1, p2]);

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			// Only ONE terminal should exist — the second call sees the restored
			// terminal as a live validTerminal and does not duplicate.
			expect(branch?.terminals.length).toBe(1);
		});
	});

	describe("handleAddTerminalToWorkspace", () => {
		it("creates terminal with branch worktree path", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt-feature" });

			const id = await gitOps.handleAddTerminalToWorkspace("/repo", "feature");

			expect(id).toBeDefined();
			const t = terminalsStore.get(id!);
			expect(t?.cwd).toBe("/repo/wt-feature");
			expect(t?.name).toContain("feature");
		});

		it("sets status info when max sessions reached", async () => {
			mockPty.canSpawn.mockResolvedValue(false);
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			await gitOps.handleAddTerminalToWorkspace("/repo", "main");

			expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
		});
	});

	describe("handleRemoveRepo", () => {
		it("removes repo after confirmation", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "My Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");

			await gitOps.handleRemoveRepo("/repo");

			expect(mockDialogs.confirmRemoveRepo).toHaveBeenCalledWith("My Repo");
			expect(repositoriesStore.get("/repo")).toBeUndefined();
			expect(gitOps.currentRepoPath()).toBeUndefined();
			expect(gitOps.currentBranch()).toBeNull();
		});

		it("does not remove when user cancels", async () => {
			mockDialogs.confirmRemoveRepo.mockResolvedValue(false);
			repositoriesStore.add({ path: "/repo", displayName: "My Repo" });

			await gitOps.handleRemoveRepo("/repo");

			expect(repositoriesStore.get("/repo")).toBeDefined();
		});

		it("calls stop_repo_watcher with the removed repo path (story 1372-1e58)", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "My Repo" });

			await gitOps.handleRemoveRepo("/repo");

			expect(mockInvoke).toHaveBeenCalledWith("stop_repo_watcher", { repoPath: "/repo" });
		});

		it("does not call stop_repo_watcher when user cancels", async () => {
			mockDialogs.confirmRemoveRepo.mockResolvedValue(false);
			repositoriesStore.add({ path: "/repo", displayName: "My Repo" });

			await gitOps.handleRemoveRepo("/repo");

			expect(mockInvoke).not.toHaveBeenCalledWith("stop_repo_watcher", expect.anything());
		});
	});

	describe("handleRemoveWorkspace", () => {
		it("keeps an unmerged worktree and its terminals when branch deletion was requested", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 0,
				commitStatus: "unmerged",
				removalSafety: "safe",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt", terminals: ["term-1"] });

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).not.toHaveBeenCalled();
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.worktreePath).toBe("/repo/wt");
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("unmerged commits"));
		});
		// Catches: a rejected confirmation retaining the lock and suppressing a later removal.
		it("rejected_confirmation_then_second_removal_proceeds", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			const id = terminalsStore.add(makeTerminal({ name: "T1" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "feature", id);
			const dialogError = new Error("confirmation failed");
			mockDialogs.confirmRemoveWorktree.mockRejectedValueOnce(dialogError);

			await expect(gitOps.handleRemoveWorkspace("/repo", "feature")).rejects.toBe(dialogError);

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.terminals).toEqual([id]);
			expect(terminalsStore.get(id)).toBeDefined();
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.isRemoving).toBeFalsy();
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledTimes(2);
			expect(mockCloseTerminal).toHaveBeenCalledWith(id, true);
			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(1);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		// Catches: synchronous dialog failures after successful preflight leaking the removal lock.
		it("throwing_confirmation_after_preflight_preserves_workspace_and_permits_retry", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			const id = terminalsStore.add(makeTerminal({ name: "T1" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "feature", id);
			const dialogError = new Error("dialog threw");
			mockDialogs.confirmRemoveWorktree.mockImplementationOnce(() => {
				expect(mockRepo.getWorkspaceLifecycle).toHaveBeenCalledWith("/repo", "feature");
				throw dialogError;
			});

			await expect(gitOps.handleRemoveWorkspace("/repo", "feature")).rejects.toBe(dialogError);

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.terminals).toEqual([id]);
			expect(terminalsStore.get(id)).toBeDefined();
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledTimes(2);
			expect(mockCloseTerminal).toHaveBeenCalledWith(id, true);
			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(1);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		// Catches: cancellation closing terminals or retaining the lock so a later removal is ignored.
		it("false_confirmation_preserves_workspace_and_terminals_and_permits_retry", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			const id = terminalsStore.add(makeTerminal({ name: "T1" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "feature", id);
			mockDialogs.confirmRemoveWorktree.mockResolvedValueOnce(false);

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.terminals).toEqual([id]);
			expect(terminalsStore.get(id)).toBeDefined();
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledTimes(2);
			expect(mockCloseTerminal).toHaveBeenCalledWith(id, true);
			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(1);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("removes worktree branch after confirmation, passing deleteBranchOnRemove setting", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			// Default deleteBranchOnRemove is true (from repoDefaults)
			await gitOps.handleRemoveWorkspace("/repo", "feature");

			// The fresh lifecycle verdict travels with the removal question.
			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledWith(
				"feature",
				expect.objectContaining({ removalSafety: "safe" }),
				true,
			);
			expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", true, false);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("surfaces partial branch-delete warning after removing worktree", async () => {
			mockRepo.removeWorktree.mockResolvedValueOnce({
				branch_delete_warning: "git branch -d refused because branch is not fully merged",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.setLabel("/repo", "feature", "Feature label");

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
			expect(repoSettingsStore.get("/repo")?.branchLabels["feature"]).toBe("Feature label");
			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Removed feature worktree; branch was kept: git branch -d refused because branch is not fully merged",
			);
		});

		it("passes deleteBranch=false when repo setting overrides default", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 0,
				commitStatus: "unmerged",
				removalSafety: "safe",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			// Set per-repo setting to override default deleteBranchOnRemove=true
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { deleteBranchOnRemove: false });

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", false, false);
		});

		it("rejects removal of non-worktree branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", {});

			await gitOps.handleRemoveWorkspace("/repo", "main");

			expect(mockSetStatusInfo).toHaveBeenCalledWith("Cannot remove main: not a worktree");
		});

		it("sets isRemoving=true on store before invoking backend", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			// Capture isRemoving state when removeWorktree is invoked
			let isRemovingWhenInvoked: boolean | undefined;
			mockRepo.removeWorktree = vi.fn(async () => {
				isRemovingWhenInvoked = repositoriesStore.get("/repo")?.workspaces["feature"]?.isRemoving;
			});

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(isRemovingWhenInvoked).toBe(true);
			// After success: branch fully removed from store
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("prevents concurrent remove calls for the same branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			// Block the first invoke until we release it
			let resolveFirst: (() => void) | undefined;
			let resolveFirstInvoked: (() => void) | undefined;
			const firstInvokedPromise = new Promise<void>((r) => {
				resolveFirstInvoked = r;
			});
			let invocationCount = 0;
			mockRepo.removeWorktree = vi.fn(async () => {
				invocationCount++;
				if (invocationCount === 1) {
					resolveFirstInvoked?.();
					await new Promise<void>((resolve) => {
						resolveFirst = resolve;
					});
				}
			});

			const first = gitOps.handleRemoveWorkspace("/repo", "feature");
			// Wait until the first call has reached removeWorktree (lock is set)
			await firstInvokedPromise;
			// Now fire the second call — should hit the lock and no-op
			const second = gitOps.handleRemoveWorkspace("/repo", "feature");
			await second;
			// Release the first call
			resolveFirst?.();
			await first;

			expect(invocationCount).toBe(1);
		});
	});

	describe("handleRenameBranch", () => {
		it("renames branch in backend and store", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "old-name", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "old-name");
			gitOps.setBranchToRename({ repoPath: "/repo", branchName: "old-name" });

			await gitOps.handleRenameBranch("old-name", "new-name");

			expect(mockRepo.renameBranch).toHaveBeenCalledWith("/repo", "old-name", "new-name");
			expect(repositoriesStore.get("/repo")?.workspaces["new-name"]).toBeDefined();
			expect(repositoriesStore.get("/repo")?.workspaces["old-name"]).toBeUndefined();
			expect(gitOps.currentBranch()).toBe("new-name");
		});
	});

	describe("handleCreateBranch", () => {
		it("creates a branch in backend + store and checks it out when requested", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setActive("/repo");
			gitOps.setBranchToCreate({ repoPath: "/repo", startPoint: "main" });

			await gitOps.handleCreateBranch("feat/new", true);

			expect(mockRepo.createBranch).toHaveBeenCalledWith("/repo", "feat/new", "main", true);
			expect(repositoriesStore.get("/repo")?.workspaces["feat/new"]).toBeDefined();
			expect(gitOps.currentBranch()).toBe("feat/new");
		});

		it("keeps the app on the active repo when it checks out a branch in a background repo", async () => {
			repositoriesStore.add({ path: "/active", displayName: "Active" });
			repositoriesStore.setWorkspace("/active", "main", { worktreePath: "/active" });
			repositoriesStore.setActive("/active");
			repositoriesStore.add({ path: "/background", displayName: "Background" });
			gitOps.setBranchToCreate({ repoPath: "/background", startPoint: "main" });

			await gitOps.handleCreateBranch("feat/bg", true);

			expect(repositoriesStore.state.activeRepoPath).toBe("/active");
			expect(repositoriesStore.get("/background")?.activeWorkspaceId).toBe("feat/bg");
		});

		it("does not switch the current branch when checkout is false", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repositoriesStore.add({ path: "/repo2", displayName: "Repo2" });
			gitOps.setBranchToCreate({ repoPath: "/repo2", startPoint: null });

			await gitOps.handleCreateBranch("feat/x", false);

			expect(mockRepo.createBranch).toHaveBeenCalledWith("/repo2", "feat/x", null, false);
			expect(gitOps.currentBranch()).toBe("main");
		});
	});

	describe("activeWorktreePath", () => {
		it("returns undefined when no active repo", () => {
			expect(gitOps.activeWorktreePath()).toBeUndefined();
		});

		it("returns worktree path of active branch", () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo/main" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			expect(gitOps.activeWorktreePath()).toBe("/repo/main");
		});
	});

	describe("activeRunCommand", () => {
		it("returns undefined when no active repo", () => {
			expect(gitOps.activeRunCommand()).toBeUndefined();
		});

		it("returns saved run command", () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repositoriesStore.setRunCommand("/repo", "main", "npm test");

			expect(gitOps.activeRunCommand()).toBe("npm test");
		});
	});

	describe("handleNewTab", () => {
		it("creates terminal in active branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			await gitOps.handleNewTab();

			const branch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(branch?.terminals.length).toBeGreaterThan(0);
		});

		it("falls back to createNewTerminal when no active branch", async () => {
			await gitOps.handleNewTab();

			expect(mockCreateNewTerminal).toHaveBeenCalled();
		});

		it("uses active terminal CWD to find correct branch when store activeBranch is stale", async () => {
			// Setup: repo has main + feature/acme (linked worktree), store says main is active (stale)
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature/acme", { worktreePath: "/repo/.worktrees/acme" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main"); // stale — HEAD actually moved to feature/acme

			// Active terminal is in the feature/acme worktree directory
			const existingTid = terminalsStore.add(
				makeTerminal({ name: "T1", sessionId: "s1", cwd: "/repo/.worktrees/acme" }),
			);
			repositoriesStore.addTerminalToWorkspace("/repo", "feature/acme", existingTid);
			terminalsStore.setActive(existingTid);

			await gitOps.handleNewTab();

			// New terminal must go to feature/acme (the CWD-matched branch), not main (stale activeBranch)
			const featureBranch = repositoriesStore.get("/repo")?.workspaces["feature/acme"];
			expect(featureBranch?.terminals.length).toBe(2); // existing + new
			const mainBranch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(mainBranch?.terminals.length).toBe(0);
		});

		it("uses active terminal CWD for main worktree when HEAD changed externally", async () => {
			// Setup: repo with one branch, store activeBranch="old-branch" but terminal CWD is repo root
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "old-branch", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "new-branch", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "old-branch"); // stale

			// Active terminal is at repo root (HEAD moved to new-branch externally)
			const existingTid = terminalsStore.add(makeTerminal({ name: "T1", sessionId: "s2", cwd: "/repo" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "new-branch", existingTid);
			terminalsStore.setActive(existingTid);

			await gitOps.handleNewTab();

			// New terminal goes to new-branch (matched by CWD), not old-branch
			const newBranch = repositoriesStore.get("/repo")?.workspaces["new-branch"];
			expect(newBranch?.terminals.length).toBe(2);
			const oldBranch = repositoriesStore.get("/repo")?.workspaces["old-branch"];
			expect(oldBranch?.terminals.length).toBe(0);
		});
	});

	describe("handleMergeAndArchive", () => {
		it("removes branch from sidebar when action is archive", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "feature/x");
			mockRepo.mergeAndArchiveWorktree.mockResolvedValue({
				merged: true,
				action: "archived",
				archive_path: "/archived/feature-x",
			});

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeUndefined();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("archived"));
		});

		// Archiving or deleting a worktree ends in `git worktree remove --force`, which
		// destroys uncommitted work. The backend refuses whenever it cannot confirm the
		// worktree is clean — with or without commits to merge — and asks first.
		describe("dirty-worktree guard", () => {
			function seedBranch() {
				repositoriesStore.add({ path: "/repo", displayName: "Repo" });
				repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
				repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			}

			it("keeps a clean live worktree when the user declines one-click cleanup", async () => {
				seedBranch();
				mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
					dirtyFiles: 0,
					dirtyFingerprint: "confirmed-clean",
					commitStatus: "unmerged",
					removalSafety: "safe",
					liveSessions: [{ sessionId: "pty-active", name: "Codex: gate work" }],
					warnings: ["Live session: Codex: gate work"],
				});
				mockDialogs.confirmDirtyWorktreeCleanup.mockResolvedValueOnce(false);
				mockRepo.mergeAndArchiveWorktree.mockResolvedValueOnce({
					merged: false,
					action: "needs_confirmation",
					archive_path: null,
					commits_ahead: 1,
					worktree_dirty: false,
				});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

				expect(mockDialogs.confirmDirtyWorktreeCleanup).toHaveBeenCalledWith(
					"feature/x",
					"archive",
					1,
					expect.objectContaining({ liveSessions: [{ sessionId: "pty-active", name: "Codex: gate work" }] }),
				);
				expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalledTimes(1);
				expect(mockSetStatusInfo).toHaveBeenCalledWith("Left feature/x alone — its worktree needs review");
				expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			});

			it("keeps the branch and does not force when the user declines", async () => {
				seedBranch();
				mockDialogs.confirmDirtyWorktreeCleanup.mockResolvedValueOnce(false);
				mockRepo.mergeAndArchiveWorktree.mockResolvedValueOnce({
					merged: false,
					action: "needs_confirmation",
					archive_path: null,
					commits_ahead: 0,
					worktree_dirty: true,
				});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

				expect(mockDialogs.confirmDirtyWorktreeCleanup).toHaveBeenCalledWith(
					"feature/x",
					"archive",
					0,
					expect.objectContaining({ dirtyFingerprint: "confirmed-worktree" }),
				);
				expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalledTimes(1);
				expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			});

			// The old guard only fired on an empty branch, so a worktree full of
			// uncommitted work was force-removed without a word the moment the branch
			// carried a single commit. `commits_ahead` must not enter the decision.
			it("asks even when the branch has commits to merge", async () => {
				seedBranch();
				mockDialogs.confirmDirtyWorktreeCleanup.mockResolvedValueOnce(false);
				mockRepo.mergeAndArchiveWorktree.mockResolvedValueOnce({
					merged: false,
					action: "needs_confirmation",
					archive_path: null,
					commits_ahead: 7,
					worktree_dirty: true,
				});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "delete");

				expect(mockDialogs.confirmDirtyWorktreeCleanup).toHaveBeenCalledWith(
					"feature/x",
					"delete",
					7,
					expect.objectContaining({ dirtyFingerprint: "confirmed-worktree" }),
				);
				expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			});

			it("retries with force once the user confirms", async () => {
				seedBranch();
				mockDialogs.confirmDirtyWorktreeCleanup.mockResolvedValueOnce(true);
				mockRepo.mergeAndArchiveWorktree
					.mockResolvedValueOnce({
						merged: false,
						action: "needs_confirmation",
						archive_path: null,
						commits_ahead: 0,
						worktree_dirty: true,
					})
					.mockResolvedValueOnce({
						merged: true,
						action: "archived",
						archive_path: "/archived/feature-x",
						commits_ahead: 0,
						worktree_dirty: true,
					});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

				// Branch AND workspace id: the merge subject and the checkout to
				// dispose of. Equal here only because of the identity migration.
				expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenLastCalledWith(
					"/repo",
					"feature/x",
					"feature/x",
					"main",
					"archive",
					true,
					"confirmed-worktree",
				);
				expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeUndefined();
			});

			it("reports what the merge actually carried across", async () => {
				seedBranch();
				mockRepo.mergeAndArchiveWorktree.mockResolvedValueOnce({
					merged: true,
					action: "archived",
					archive_path: "/archived/feature-x",
					commits_ahead: 7,
					worktree_dirty: false,
				});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

				expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("7 commits"));
			});

			it("says so when the merge was a no-op", async () => {
				seedBranch();
				mockRepo.mergeAndArchiveWorktree.mockResolvedValueOnce({
					merged: true,
					action: "archived",
					archive_path: "/archived/feature-x",
					commits_ahead: 0,
					worktree_dirty: false,
				});

				await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

				expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("nothing to merge"));
			});
		});

		it("sets mergePendingCtx when action is pending (ask mode)", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");

			// Branch stays in sidebar — user must choose
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			// Dialog context is populated
			expect(gitOps.mergePendingCtx()).toEqual({
				repoPath: "/repo",
				workspaceId: "feature/x",
				branchName: "feature/x",
				baseBranch: "main",
				hasDirtyFiles: false,
				worktreeDirty: false,
				worktreeFingerprint: "confirmed-worktree",
			});
		});

		it("dismissMergePending clears the context and keeps branch in sidebar", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "pending", archive_path: null });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");
			gitOps.dismissMergePending();

			// Branch stays — cleanup dialog was skipped
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			expect(gitOps.mergePendingCtx()).toBeNull();
		});

		it("keeps branch and terminals when merge fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			repositoriesStore.addTerminalToWorkspace("/repo", "feature/x", "term-99");
			terminalsStore.register("term-99", makeTerminal({ name: "T-99", cwd: "/repo/.wt/x" }));
			mockRepo.mergeAndArchiveWorktree.mockRejectedValueOnce(new Error("Merge failed (conflicts?)"));

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			// Branch stays in sidebar
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
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
			conflict_state: "clear" as const,
			merge_state_status: "CLEAN",
			review_decision: "APPROVED",
			viewer_did_approve: false,
			labels: [],
			is_draft: false,
			base_ref_name: "main",
			head_ref_oid: "abc1234",
			created_at: "2026-01-01T00:00:00Z",
			updated_at: "2026-01-02T00:00:00Z",
			merge_state_label: null,
			review_state_label: null,
			merge_commit_allowed: true,
			squash_merge_allowed: true,
			rebase_merge_allowed: true,
			unresolved_threads: 0,
			unresolved_threads_truncated: false,
		};

		beforeEach(() => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			repositoriesStore.setWorkspace("/repo", "feature/x", { worktreePath: "/repo/.wt/x" });
			githubStore.updateRepoData("/repo", [testPr]);
		});

		afterEach(() => {
			githubStore.updateRepoData("/repo", []); // clear PR data
		});

		it("uses GitHub API merge when an open PR exists for the branch", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { prMergeStrategy: "squash" });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(mockRepo.mergePrViaGithub).toHaveBeenCalledWith("/repo", 99, "squash", "abc1234");
			expect(mockRepo.mergeAndArchiveWorktree).not.toHaveBeenCalled();
			expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/x", "archive");
		});

		it("falls back to local git merge when GitHub API fails", async () => {
			mockRepo.mergePrViaGithub.mockRejectedValueOnce(new Error("no token"));
			mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: null });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(mockRepo.mergePrViaGithub).toHaveBeenCalled();
			expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalledWith(
				"/repo",
				"feature/x",
				"feature/x",
				"main",
				"archive",
			);
		});

		it("sets mergePendingCtx when afterMerge=ask with GitHub PR merge", async () => {
			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "ask");

			expect(mockRepo.mergePrViaGithub).toHaveBeenCalled();
			expect(gitOps.mergePendingCtx()).toEqual({
				repoPath: "/repo",
				workspaceId: "feature/x",
				branchName: "feature/x",
				baseBranch: "main",
				hasDirtyFiles: false,
				worktreeDirty: false,
				worktreeFingerprint: "confirmed-worktree",
			});
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
		});

		it("uses local git merge when no PR exists for the branch", async () => {
			githubStore.updateRepoData("/repo", []); // no PR
			mockRepo.mergeAndArchiveWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: null });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(mockRepo.mergePrViaGithub).not.toHaveBeenCalled();
			expect(mockRepo.mergeAndArchiveWorktree).toHaveBeenCalled();
		});

		it("uses effectiveMergeMethod to pick allowed method when preferred is disallowed", async () => {
			// Repo only allows squash; preferred is "merge"
			githubStore.updateRepoData("/repo", [
				{ ...testPr, merge_commit_allowed: false, squash_merge_allowed: true, rebase_merge_allowed: false },
			]);
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { prMergeStrategy: "merge" });

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(mockRepo.mergePrViaGithub).toHaveBeenCalledWith("/repo", 99, "squash", "abc1234");
		});

		it("re-throws 405 error instead of silently falling back to local merge", async () => {
			const err = new Error("GitHub merge failed (405): Merge commits are not allowed on this repository.");
			mockRepo.mergePrViaGithub.mockRejectedValueOnce(err);

			await expect(gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive")).rejects.toThrow("405");

			expect(mockRepo.mergeAndArchiveWorktree).not.toHaveBeenCalled();
		});

		it("does not fall back to a local merge when GitHub says the PR head changed", async () => {
			// A local merge would include the unreviewed commits the pin refused.
			mockRepo.mergePrViaGithub.mockRejectedValueOnce(new Error("PR head changed: refresh and review"));

			await gitOps.handleMergeAndArchive("/repo", "feature/x", "main", "archive");

			expect(mockRepo.mergeAndArchiveWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("PR head changed"));
		});
	});

	describe("handleConflictAssist", () => {
		// vi.clearAllMocks() (beforeEach) clears call history but NOT implementations,
		// so restore mockInvoke to its default here to avoid leaking into later tests.
		afterEach(() => {
			mockInvoke.mockReset();
			mockInvoke.mockResolvedValue(undefined);
		});

		function mockConflictAssist(result: {
			status: "clean" | "clean_unverified" | "conflicts";
			worktree_path: string;
			branch: string;
			base: string;
			base_source: "fetched_remote" | "existing_tracking" | "local_fallback";
			base_warning: string | null;
			conflicted_files: string[];
			prompt: string;
		}) {
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "start_conflict_assist" ? Promise.resolve(result) : defaultInvoke(cmd),
			);
		}

		it("reports a clean rebase and does not register a worktree", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			mockConflictAssist({
				status: "clean",
				worktree_path: "/repo/.worktrees/feature",
				branch: "feature",
				base: "main",
				base_source: "fetched_remote",
				base_warning: null,
				conflicted_files: [],
				prompt: "",
			});

			await gitOps.handleConflictAssist("/repo", 42);

			expect(mockInvoke).toHaveBeenCalledWith("start_conflict_assist", { repoPath: "/repo", prNumber: 42 });
			expect(mockSetStatusInfo).toHaveBeenCalledWith("PR #42 rebased cleanly onto main");
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("warns when a conflict-free rebase used a base that could not be refreshed", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			mockConflictAssist({
				status: "clean_unverified",
				worktree_path: "/repo/.worktrees/feature",
				branch: "feature",
				base: "main",
				base_source: "existing_tracking",
				base_warning: "Could not refresh origin/main; using the existing remote-tracking ref.",
				conflicted_files: [],
				prompt: "",
			});

			await gitOps.handleConflictAssist("/repo", 43);

			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Could not refresh origin/main"));
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("registers the existing worktree and seeds an agent with the resolution prompt on conflicts", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			mockConflictAssist({
				status: "conflicts",
				worktree_path: "/repo/.worktrees/feature",
				branch: "feature",
				base: "main",
				base_source: "fetched_remote",
				base_warning: null,
				conflicted_files: ["a.ts"],
				prompt: "Resolve the conflicts in a.ts",
			});

			await gitOps.handleConflictAssist("/repo", 7);

			const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
			expect(branch?.worktreePath).toBe("/repo/.worktrees/feature");
			const termId = branch?.terminals[0];
			const term = termId ? terminalsStore.get(termId) : undefined;
			expect(term?.agentType).toBe("claude");
			expect(term?.pendingInitCommand).toContain("'Resolve the conflicts in a.ts'");
			// createWorktree is NOT called — the backend already created the worktree.
			expect(mockRepo.createWorktree).not.toHaveBeenCalled();
		});

		it("surfaces backend errors via status info without throwing", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "start_conflict_assist" ? Promise.reject(new Error("no PR head")) : Promise.resolve(undefined),
			);

			await gitOps.handleConflictAssist("/repo", 9);

			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to resolve conflicts for PR #9"));
			// Lock released — a second call is not blocked.
			expect(gitOps.creatingWorktreeRepos().has("/repo")).toBe(false);
		});
	});

	describe("refreshAllBranchStats", () => {
		/** Helper: mock both Phase 1 (structure) and Phase 2 (diff stats) from a single summary object */
		function mockSummary(summary: {
			worktree_paths: Record<string, { branch: string; path: string }>;
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
				diff_stats: { "/repo": { additions: 5, deletions: 3 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			const branch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(branch?.additions).toBe(5);
			expect(branch?.deletions).toBe(3);
		});

		it("refreshes the active repo first and caps repo fan-out", async () => {
			const pending: Array<{ path: string; resolve: (value: unknown) => void }> = [];
			for (let index = 0; index < 7; index++) {
				const path = `/repo-${index}`;
				repositoriesStore.add({ path, displayName: `Repo ${index}` });
				repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
			}
			repositoriesStore.setActive("/repo-6");
			mockRepo.getRepoStructure.mockImplementation(
				(path: string) => new Promise((resolve) => pending.push({ path, resolve })),
			);
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			const refresh = gitOps.refreshAllBranchStats();
			await vi.waitFor(() => expect(pending.length).toBeGreaterThan(0));

			expect(pending[0].path).toBe("/repo-6");
			expect(pending.length).toBeLessThanOrEqual(4);

			let released = 0;
			while (released < 7) {
				while (released < pending.length) {
					const item = pending[released++];
					item.resolve({ worktree_paths: wtPaths({ main: item.path }), merged_branches: [] });
				}
				if (released < 7) {
					await vi.waitFor(() => expect(pending.length).toBeGreaterThan(released));
				}
			}
			await refresh;
		});

		it("refreshes every active repo once when the last repo has focus", async () => {
			for (const path of ["/repo-a", "/repo-b", "/repo-c"]) {
				repositoriesStore.add({ path, displayName: path });
				repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
			}
			repositoriesStore.setActive("/repo-c");
			mockRepo.getRepoStructure.mockImplementation(async (path: string) => ({
				worktree_paths: wtPaths({ main: path }),
				merged_branches: [],
			}));
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.getRepoStructure.mock.calls.map(([path]) => path)).toEqual(["/repo-c", "/repo-a", "/repo-b"]);
		});

		it("starts a new backend pass after the previous refresh completes", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");
			vi.advanceTimersByTime(5_000);
			await gitOps.refreshAllBranchStats("/repo");

			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(2);
		});

		it("git init: carries shell-branch terminals into the new git branch instead of orphaning them", async () => {
			// Repro: a clean shell-only repo with two open terminals; `git init` used to
			// remove the shell branch and create an empty git branch, orphaning the
			// terminals so they vanished from the view until a later branch-select.
			const t1 = terminalsStore.add(makeTerminal({ name: "t1" }));
			const t2 = terminalsStore.add(makeTerminal({ name: "t2" }));
			repositoriesStore.add({ path: "/shell-repo", displayName: "Shell", isGitRepo: false });
			repositoriesStore.setWorkspace("/shell-repo", "shell", {
				worktreePath: "/shell-repo",
				isMain: true,
				isShell: true,
			});
			repositoriesStore.addTerminalToWorkspace("/shell-repo", "shell", t1);
			repositoriesStore.addTerminalToWorkspace("/shell-repo", "shell", t2);
			repositoriesStore.setWorkspace("/shell-repo", "shell", { lastActiveTerminal: t2 });
			repositoriesStore.setActiveWorkspace("/shell-repo", "shell");

			mockRepo.getInfo.mockResolvedValue({
				path: "/shell-repo",
				name: "shell-repo",
				initials: "SR",
				branch: "main",
				status: "clean",
				is_git_repo: true,
			});

			await gitOps.refreshAllBranchStats("/shell-repo");

			const repo = repositoriesStore.get("/shell-repo");
			expect(repo?.isGitRepo).toBe(true);
			expect(repo?.activeWorkspaceId).toBe("main");
			expect(repo?.workspaces["shell"]).toBeUndefined();
			// Both terminals carried over (not orphaned) + last-active preserved.
			expect(repo?.workspaces["main"]?.terminals).toEqual([t1, t2]);
			expect(repo?.workspaces["main"]?.lastActiveTerminal).toBe(t2);
			expect(mockInvoke).toHaveBeenCalledWith("stop_repo_watcher", { repoPath: "/shell-repo" });
			expect(mockInvoke).toHaveBeenCalledWith("start_repo_watcher", { repoPath: "/shell-repo" });
		});

		it("git init preserves the last active terminal when a later shell row has none", async () => {
			const tid = terminalsStore.add(makeTerminal({ name: "Active shell" }));
			repositoriesStore.add({ path: "/shell-repo", displayName: "Shell", isGitRepo: false });
			repositoriesStore.setWorkspace("/shell-repo", "shell", { worktreePath: "/shell-repo", isShell: true });
			repositoriesStore.addTerminalToWorkspace("/shell-repo", "shell", tid);
			repositoriesStore.setWorkspace("/shell-repo", "shell", { lastActiveTerminal: tid });
			repositoriesStore.setWorkspace("/shell-repo", "extra-shell", { worktreePath: "/shell-repo", isShell: true });
			mockRepo.getInfo.mockResolvedValue({ branch: "main", is_git_repo: true });

			await gitOps.refreshAllBranchStats("/shell-repo");

			expect(repositoriesStore.get("/shell-repo")?.workspaces.main?.lastActiveTerminal).toBe(tid);
		});

		it("keeps a shell repository when Git has no branch yet", async () => {
			repositoriesStore.add({ path: "/shell-repo", displayName: "Shell", isGitRepo: false });
			repositoriesStore.setWorkspace("/shell-repo", "shell", { worktreePath: "/shell-repo", isShell: true });
			mockRepo.getInfo.mockResolvedValue({ branch: "", is_git_repo: true });

			await gitOps.refreshAllBranchStats("/shell-repo");

			const repo = repositoriesStore.get("/shell-repo");
			expect(repo?.isGitRepo).toBe(false);
			expect(Object.keys(repo?.workspaces ?? {})).toEqual(["shell"]);
			expect(mockInvoke).not.toHaveBeenCalledWith("stop_repo_watcher", expect.anything());
		});

		it("moves every terminal to shell when the Git directory disappears", async () => {
			const mainTerm = terminalsStore.add(makeTerminal({ name: "Main", cwd: "/repo" }));
			const linkedTerm = terminalsStore.add(makeTerminal({ name: "Linked", cwd: "/repo/wt" }));
			repositoriesStore.add({ path: "/repo", displayName: "Repo", isGitRepo: true });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", branchName: "main" });
			repositoriesStore.setWorkspace("/repo", "ws-linked", { worktreePath: "/repo/wt", branchName: "feature/x" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", mainTerm);
			repositoriesStore.addTerminalToWorkspace("/repo", "ws-linked", linkedTerm);
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: {}, merged_branches: [] });
			mockRepo.getInfo.mockResolvedValue({ branch: "", is_git_repo: false });

			await gitOps.refreshAllBranchStats("/repo");

			const repo = repositoriesStore.get("/repo");
			expect(repo?.isGitRepo).toBe(false);
			expect(Object.keys(repo?.workspaces ?? {})).toEqual(["shell"]);
			expect(repo?.activeWorkspaceId).toBe("shell");
			expect(repo?.workspaces.shell).toMatchObject({
				worktreePath: "/repo",
				isMain: true,
				isShell: true,
			});
			expect(repo?.workspaces.shell.terminals).toEqual([mainTerm, linkedTerm]);
		});

		it("preserves workspaces when a still-Git repository has a transient empty structure", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo", isGitRepo: true });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: {}, merged_branches: [] });
			mockRepo.getInfo.mockResolvedValue({ branch: "main", is_git_repo: true });

			await gitOps.refreshAllBranchStats("/repo");

			expect(Object.keys(repositoriesStore.get("/repo")?.workspaces ?? {})).toEqual(["main", "feature"]);
			expect(mockRepo.getRepoDiffStats).not.toHaveBeenCalled();
		});

		it("scopes to a single repo when a path is given — other repos untouched", async () => {
			repositoriesStore.add({ path: "/repo-a", displayName: "A" });
			repositoriesStore.setWorkspace("/repo-a", "main", { worktreePath: "/repo-a" });
			repositoriesStore.add({ path: "/repo-b", displayName: "B" });
			repositoriesStore.setWorkspace("/repo-b", "main", { worktreePath: "/repo-b" });
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo-a" }),
				merged_branches: [],
				diff_stats: { "/repo-a": { additions: 7, deletions: 2 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats("/repo-a");

			// Only the scoped repo's structure was fetched — no fan-out to /repo-b.
			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(1);
			expect(mockRepo.getRepoStructure).toHaveBeenCalledWith("/repo-a");
		});

		it("prunes branches not in worktree paths", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "stale", { worktreePath: "/repo/stale" });
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["stale"]).toBeUndefined();
			expect(repositoriesStore.get("/repo")?.workspaces["main"]).toBeDefined();
		});

		it("keeps a just-created worktree through the grace deadline, then removes it", async () => {
			vi.setSystemTime(new Date("2026-09-29T12:00:00Z"));
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "new", { worktreePath: "/repo/.worktrees/new" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
			const coordinator = createRepositoryRefreshCoordinator({
				repo: mockRepo,
				dialogs: {},
				closeTerminal: mockCloseTerminal,
				closeTerminalsInWorktree: vi.fn(),
				setStatusInfo: mockSetStatusInfo,
			});
			coordinator.markRecentlyCreated("/repo", "new");

			await coordinator.refreshAllBranchStats("/repo");
			expect(repositoriesStore.get("/repo")?.workspaces.new).toBeDefined();
			vi.setSystemTime(new Date("2026-09-29T12:01:00Z"));
			await coordinator.refreshAllBranchStats("/repo");
			expect(repositoriesStore.get("/repo")?.workspaces.new).toBeDefined();
			vi.setSystemTime(new Date("2026-09-29T12:01:05Z"));
			await coordinator.refreshAllBranchStats("/repo");
			expect(repositoriesStore.get("/repo")?.workspaces.new).toBeUndefined();
		});

		it("coalesces refresh storms without starving stale-worktree pruning", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "ghost-a", { worktreePath: "/repo/wt-a" });
			repositoriesStore.setWorkspace("/repo", "ghost-b", { worktreePath: "/repo/wt-b" });

			const structureResolvers: Array<(value: unknown) => void> = [];
			mockRepo.getRepoStructure.mockImplementation(() => new Promise((resolve) => structureResolvers.push(resolve)));
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: {},
			});

			const first = gitOps.refreshAllBranchStats("/repo");
			await vi.waitFor(() => expect(structureResolvers).toHaveLength(1));
			const queued = Array.from({ length: 100 }, () => gitOps.refreshAllBranchStats("/repo"));
			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(1);

			structureResolvers[0]({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			await vi.advanceTimersByTimeAsync(5_000);
			await vi.waitFor(() => expect(structureResolvers).toHaveLength(2));
			structureResolvers[1]({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			await Promise.all([first, ...queued]);

			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(2);
			expect(Object.keys(repositoriesStore.get("/repo")?.workspaces ?? {})).toEqual(["main"]);
		});

		it("runs the queued refresh after the active refresh fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			let rejectFirst!: (reason: Error) => void;
			mockRepo.getRepoStructure
				.mockReturnValueOnce(new Promise((_, reject) => (rejectFirst = reject)))
				.mockResolvedValueOnce({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: {},
			});

			const first = gitOps.refreshAllBranchStats("/repo");
			await vi.waitFor(() => expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(1));
			const queued = gitOps.refreshAllBranchStats("/repo");
			rejectFirst(new Error("transient structure failure"));
			await vi.advanceTimersByTimeAsync(5_000);
			await Promise.all([first, queued]);

			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(2);
			expect(mockRepo.getRepoDiffStats).toHaveBeenCalledTimes(1);
		});

		it("does not refresh a parked repository even when explicitly scoped", async () => {
			repositoriesStore.add({ path: "/parked", displayName: "Parked" });
			repositoriesStore.setWorkspace("/parked", "main", { worktreePath: "/parked" });
			repositoriesStore.setPark("/parked", true);

			await gitOps.refreshAllBranchStats("/parked");

			expect(mockRepo.getRepoStructure).not.toHaveBeenCalled();
			expect(mockRepo.getRepoDiffStats).not.toHaveBeenCalled();
		});

		it("does not revive a parked repository just because it retains focus", async () => {
			repositoriesStore.add({ path: "/parked", displayName: "Parked" });
			repositoriesStore.setWorkspace("/parked", "main", { worktreePath: "/parked" });
			repositoriesStore.add({ path: "/active", displayName: "Active" });
			repositoriesStore.setWorkspace("/active", "main", { worktreePath: "/active" });
			repositoriesStore.setActive("/parked");
			repositoriesStore.setPark("/parked", true);
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/active" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.getRepoStructure.mock.calls.map(([path]) => path)).toEqual(["/active"]);
		});

		it("discovers externally created worktrees", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			// Simulate an external `git worktree add` — new branch appears in summary
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo", "feature-external": "/repo/.worktrees/feature-external" }),
				merged_branches: [],
				diff_stats: {
					"/repo": { additions: 2, deletions: 1 },
					"/repo/.worktrees/feature-external": { additions: 2, deletions: 1 },
				},
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			const newBranch = repositoriesStore.get("/repo")?.workspaces["feature-external"];
			expect(newBranch).toBeDefined();
			expect(newBranch?.worktreePath).toBe("/repo/.worktrees/feature-external");
			expect(newBranch?.additions).toBe(2);
			expect(newBranch?.deletions).toBe(1);
		});

		it("removes stale activeBranch when HEAD moved to different branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActiveWorkspace("/repo", "main");

			mockSummary({
				worktree_paths: wtPaths({ "feature/acme": "/repo" }),
				merged_branches: [],
				diff_stats: { "/repo": { additions: 1, deletions: 0 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			const repo = repositoriesStore.get("/repo");
			expect(repo?.workspaces["main"]).toBeUndefined();
			expect(repo?.workspaces["feature/acme"]).toBeDefined();
			expect(repo?.activeWorkspaceId).toBe("feature/acme");
		});

		it("migrates terminals from stale activeBranch to new worktree branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const tid = terminalsStore.add(makeTerminal({ name: "T1", sessionId: "s1", cwd: "/repo" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "main", tid);

			mockSummary({
				worktree_paths: wtPaths({ "feature/acme": "/repo" }),
				merged_branches: [],
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			const repo = repositoriesStore.get("/repo");
			expect(repo?.workspaces["main"]).toBeUndefined();
			expect(repo?.workspaces["feature/acme"]?.terminals).toContain(tid);
			expect(repo?.activeWorkspaceId).toBe("feature/acme");
		});

		it("keeps a background worktree terminal when its branch changes", async () => {
			const worktreePath = "/repo/.worktrees/agent-abc";
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "old-branch", { worktreePath });
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const tid = terminalsStore.add(makeTerminal({ name: "Agent", sessionId: "agent-session", cwd: worktreePath }));
			repositoriesStore.addTerminalToWorkspace("/repo", "old-branch", tid);

			mockSummary({
				worktree_paths: wtPaths({ main: "/repo", "new-branch": worktreePath }),
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats("/repo");

			const repo = repositoriesStore.get("/repo");
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(terminalsStore.get(tid)).toBeDefined();
			expect(repo?.workspaces["old-branch"]).toBeUndefined();
			expect(repo?.workspaces["new-branch"]?.terminals).toContain(tid);
			expect(repo?.workspaces["new-branch"]?.branchName).toBe("new-branch");
			expect(repo?.activeWorkspaceId).toBe("main");
		});

		it("keeps an active worktree terminal when its branch changes", async () => {
			const worktreePath = "/repo/.worktrees/agent-abc";
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "old-branch", { worktreePath });
			repositoriesStore.setActiveWorkspace("/repo", "old-branch");
			const tid = terminalsStore.add(makeTerminal({ name: "Agent", sessionId: "agent-session", cwd: worktreePath }));
			repositoriesStore.addTerminalToWorkspace("/repo", "old-branch", tid);
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo", "new-branch": worktreePath }),
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats("/repo");

			const repo = repositoriesStore.get("/repo");
			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(repo?.activeWorkspaceId).toBe("new-branch");
			expect(repo?.workspaces["new-branch"]?.terminals).toContain(tid);
		});

		it("handles missing diff stats gracefully (no throw)", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockSummary({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["main"]).toBeDefined();
		});

		it("stores lastCommitTs converted from seconds to milliseconds", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature-x", { worktreePath: "/repo/wt-feature-x" });

			mockSummary({
				worktree_paths: wtPaths({ main: "/repo", "feature-x": "/repo/wt-feature-x" }),
				merged_branches: [],
				diff_stats: {
					"/repo": { additions: 0, deletions: 0 },
					"/repo/wt-feature-x": { additions: 0, deletions: 0 },
				},
				last_commit_ts: { main: 1700000001, "feature-x": 1700000042 },
			});

			await gitOps.refreshAllBranchStats();

			const repo = repositoriesStore.get("/repo");
			expect(repo?.workspaces["main"]?.lastCommitTs).toBe(1700000001 * 1000);
			expect(repo?.workspaces["feature-x"]?.lastCommitTs).toBe(1700000042 * 1000);
		});

		// Catches: the store (ms) and branchFacts disagreeing on the unit, so a 2 h old Rust %ct reads "<1m" and never goes stale.
		it("renders a Rust %ct seconds value written by the refresh as its real age and stale state", async () => {
			const nowMs = 1_800_000_000_000;
			const secondsAgo = (s: number) => nowMs / 1000 - s;
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "recent", { worktreePath: "/repo/wt-recent" });
			repositoriesStore.setWorkspace("/repo", "old", { worktreePath: "/repo/wt-old" });
			mockSummary({
				worktree_paths: wtPaths({ recent: "/repo/wt-recent", old: "/repo/wt-old" }),
				merged_branches: [],
				diff_stats: {
					"/repo/wt-recent": { additions: 0, deletions: 0 },
					"/repo/wt-old": { additions: 0, deletions: 0 },
				},
				last_commit_ts: {
					recent: secondsAgo(2 * 3600),
					old: secondsAgo((STALE_AFTER_DAYS + 1) * 86_400),
				},
			});

			await gitOps.refreshAllBranchStats();

			const facts = (name: string) => {
				const ws = repositoriesStore.get("/repo")?.workspaces[name];
				return branchFacts(
					{ lastCommitTs: ws?.lastCommitTs ?? null, additions: 0, deletions: 0, dirtyFiles: 0, isMerged: false },
					nowMs,
				);
			};
			expect(facts("recent")).toMatchObject({ commitAge: "2h", state: null });
			expect(facts("old").state).toBe("stale");
		});

		it("stores lastCommitTs as null when backend returns null", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", lastCommitTs: 999 });

			mockSummary({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: { main: null },
			});

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["main"]?.lastCommitTs).toBeNull();
		});

		it("closes terminals and removes branch when worktree was deleted externally", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "worktree-agent-abc", { worktreePath: "/repo/.worktrees/agent-abc" });
			repositoriesStore.setActiveWorkspace("/repo", "main");

			// Add a live terminal on the worktree branch
			const tid = terminalsStore.add(
				makeTerminal({ name: "WT", sessionId: "wt-sess", cwd: "/repo/.worktrees/agent-abc" }),
			);
			repositoriesStore.addTerminalToWorkspace("/repo", "worktree-agent-abc", tid);

			// Backend reports worktree is gone (only main remains)
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo": { additions: 0, deletions: 0 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			// Terminal should have been closed
			expect(mockCloseTerminal).toHaveBeenCalledWith(tid, true);
			// Branch should have been removed from the store
			expect(repositoriesStore.get("/repo")?.workspaces["worktree-agent-abc"]).toBeUndefined();
		});

		it("does not close the terminals of a worktree that entered the store while the structure fetch was in flight (#1317)", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			let resolveStructure: (value: unknown) => void = () => {};
			mockRepo.getRepoStructure.mockImplementation(() => new Promise((resolve) => (resolveStructure = resolve)));
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			const refresh = gitOps.refreshAllBranchStats("/repo");
			for (let i = 0; i < 20 && mockRepo.getRepoStructure.mock.calls.length === 0; i++) await Promise.resolve();
			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(1);

			// A worktree created elsewhere (MCP worktree_create: no creation grace) gets a tab
			// after the snapshot was requested, but the snapshot predates the checkout.
			repositoriesStore.setWorkspace("/repo", "fresh", { worktreePath: "/repo/.worktrees/fresh" });
			const tid = terminalsStore.add(makeTerminal({ name: "Fresh", cwd: "/repo/.worktrees/fresh" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "fresh", tid);
			resolveStructure({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			await refresh;

			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces.fresh).toBeDefined();
		});

		it("keeps the terminals of worktrees created in one burst when the next structure snapshot still predates them (#1317)", async () => {
			vi.setSystemTime(new Date("2026-10-01T00:25:00Z"));
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
			await gitOps.refreshAllBranchStats("/repo");

			// Three MCP-created worktrees enter the store and get terminals. The next refresh
			// STARTS after they exist (so they are in priorBranchKeys), but the backend
			// coalesces/caches worktree_paths and answers with a map computed before them.
			const fresh = ["fix/1322", "fix/1276", "fix/1088"];
			const tids = fresh.map((name) => {
				repositoriesStore.setWorkspace("/repo", name, { worktreePath: `/repo/.worktrees/${name}` });
				const tid = terminalsStore.add(makeTerminal({ name, cwd: `/repo/.worktrees/${name}` }));
				repositoriesStore.addTerminalToWorkspace("/repo", name, tid);
				return tid;
			});
			vi.setSystemTime(new Date("2026-10-01T00:25:05Z"));
			await gitOps.refreshAllBranchStats("/repo");

			expect(mockCloseTerminal).not.toHaveBeenCalled();
			for (const name of fresh) expect(repositoriesStore.get("/repo")?.workspaces[name]).toBeDefined();

			// The protection is bounded: a snapshot requested after the grace window is trusted.
			vi.setSystemTime(new Date("2026-10-01T00:26:10Z"));
			await gitOps.refreshAllBranchStats("/repo");

			for (const tid of tids) expect(mockCloseTerminal).toHaveBeenCalledWith(tid, true);
			for (const name of fresh) expect(repositoriesStore.get("/repo")?.workspaces[name]).toBeUndefined();
		});

		it("keeps the terminals of worktrees whose checkout is still on disk when the snapshot omits them (#1317 live 2026-10-01 00:30/00:34)", async () => {
			// Replays the live sequence: worktrees 33-45s old with a tab, a snapshot that lacks
			// them (coalesced/cached, or judged by a concurrent run), no merge and no removal of
			// these worktrees. Their directories still exist, so nothing may be closed.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			const live = ["gate/tuic-mutants", "fix/1265-scrollbar"];
			for (const name of live) {
				repositoriesStore.setWorkspace("/repo", name, { worktreePath: `/repo/.worktrees/${name}` });
				const tid = terminalsStore.add(makeTerminal({ name, cwd: `/repo/.worktrees/${name}` }));
				repositoriesStore.addTerminalToWorkspace("/repo", name, tid);
			}
			mockRepo.getInfo.mockImplementation(async (path: string) => ({
				branch: "x",
				is_git_repo: path === "/repo" || live.some((name) => path === `/repo/.worktrees/${name}`),
			}));
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");

			expect(mockCloseTerminal).not.toHaveBeenCalled();
			for (const name of live) expect(repositoriesStore.get("/repo")?.workspaces[name]).toBeDefined();
		});

		it("keeps the terminals when the checkout probe fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			const tid = terminalsStore.add(makeTerminal({ name: "wt", cwd: "/repo/wt" }));
			repositoriesStore.setWorkspace("/repo", "linked", { worktreePath: "/repo/wt", terminals: [tid] });
			mockRepo.getInfo.mockRejectedValue(new Error("probe failed"));
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");

			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces.linked).toBeDefined();
		});

		it("closes only live terminals from a deleted linked worktree", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			const liveId = terminalsStore.add(makeTerminal({ name: "Live", cwd: "/repo/wt" }));
			repositoriesStore.setWorkspace("/repo", "linked", {
				worktreePath: "/repo/wt",
				terminals: [liveId, "closed-earlier"],
			});
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");

			expect(mockCloseTerminal).toHaveBeenCalledTimes(1);
			expect(mockCloseTerminal).toHaveBeenCalledWith(liveId, true);
			expect(repositoriesStore.get("/repo")?.workspaces.linked).toBeUndefined();
		});

		it("removes a stale pathless workspace when it has no live terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "stale", { worktreePath: null });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces.stale).toBeUndefined();
		});

		it("removes a recreated stale branch after the duplicate-removal window expires", async () => {
			vi.setSystemTime(new Date("2026-09-29T12:00:00Z"));
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "stale", { worktreePath: "/repo/.worktrees/old" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");
			expect(repositoriesStore.get("/repo")?.workspaces.stale).toBeUndefined();
			repositoriesStore.setWorkspace("/repo", "stale", { worktreePath: "/repo/.worktrees/recreated" });
			vi.setSystemTime(new Date("2026-09-29T12:00:02.001Z"));
			vi.advanceTimersByTime(5_000);
			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces.stale).toBeUndefined();
		});

		it("keeps a pathless workspace while its terminal is still open", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			const tid = terminalsStore.add(makeTerminal({ name: "Still open", cwd: "/repo" }));
			repositoriesStore.setWorkspace("/repo", "old-main", { worktreePath: null, lastCommitTs: 123 });
			repositoriesStore.addTerminalToWorkspace("/repo", "old-main", tid);
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: { "old-main": 999 } });

			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces["old-main"]).toMatchObject({
				terminals: [tid],
				lastCommitTs: 123,
			});
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(tid, true);
		});

		it("keeps a main-checkout terminal when detached HEAD leaves only linked worktrees", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			const tid = terminalsStore.add(makeTerminal({ name: "Main shell", cwd: "/repo" }));
			repositoriesStore.setWorkspace("/repo", "old-main", {
				worktreePath: "/repo",
				kind: "worktree",
				isMerged: true,
			});
			repositoriesStore.addTerminalToWorkspace("/repo", "old-main", tid);
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ linked: "/repo/.worktrees/linked" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces["old-main"]).toMatchObject({ terminals: [tid] });
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(tid, true);
			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalledWith("/repo", "old-main", "archive");
		});

		it("does not resurrect a branch deleted by user while refresh was in-flight", async () => {
			// Regression: refreshAllBranchStats fetches worktree_paths before the deletion
			// completes. When the batch runs with stale data (deleted branch still in
			// worktree_paths), setWorkspace must not re-add it if the user already removed it.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/.worktrees/feature" });

			// Hold getRepoStructure so we can inject a user deletion before it resolves
			let resolveStructure!: (v: unknown) => void;
			mockRepo.getRepoStructure.mockReturnValueOnce(new Promise((r) => (resolveStructure = r)));
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			const refreshPromise = gitOps.refreshAllBranchStats();

			// User deletes "feature" while refresh is awaiting getRepoStructure
			repositoriesStore.removeWorkspace("/repo", "feature");

			// Resolve with STALE data: "feature" still present in worktree_paths
			resolveStructure({
				worktree_paths: wtPaths({ main: "/repo", feature: "/repo/.worktrees/feature" }),
				merged_branches: [],
			});

			await refreshPromise;

			// Refresh must not resurrect the user-deleted branch
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
			expect(repositoriesStore.get("/repo")?.workspaces["main"]).toBeDefined();
		});

		it("still adds genuinely new external worktrees that were not in the snapshot", async () => {
			// Guard: the race-condition fix must only skip branches that were in the
			// snapshot AND are now gone. A brand-new external worktree (never in snapshot)
			// must still be discovered and added.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo", "external-new": "/repo/.worktrees/external-new" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: {
					"/repo": { additions: 0, deletions: 0 },
					"/repo/.worktrees/external-new": { additions: 2, deletions: 1 },
				},
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			const newBranch = repositoriesStore.get("/repo")?.workspaces["external-new"];
			expect(newBranch).toBeDefined();
			expect(newBranch?.worktreePath).toBe("/repo/.worktrees/external-new");
		});
	});

	describe("refreshAllBranchStats — progressive loading", () => {
		it("joins stats by path, commit time by branch, and lifecycle by workspace ID", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: {
					main: { branch: "main", path: "/repo", kind: "worktree" },
					"ws-1": { branch: "feature/x", path: "/repo/wt-x", kind: "worktree" },
				},
				merged_branches: ["feature/x"],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo/wt-x": { additions: 7, deletions: 3 } },
				last_commit_ts: { "feature/x": 1_700_000_000 },
				workspace_statuses: {
					"ws-1": { dirty_files: 2, commit_status: "merged", removal_safety: "needs_review" },
				},
			});

			await gitOps.refreshAllBranchStats("/repo");

			const workspace = repositoriesStore.get("/repo")?.workspaces["ws-1"];
			expect(repositoriesStore.get("/repo")?.workspaces.main.kind).toBe("main");
			expect(workspace?.branchName).toBe("feature/x");
			expect(workspace?.isMerged).toBe(true);
			expect(workspace?.additions).toBe(7);
			expect(workspace?.deletions).toBe(3);
			expect(workspace?.lastCommitTs).toBe(1_700_000_000_000);
			expect(workspace?.lifecycleStatus).toMatchObject({
				dirtyFiles: 2,
				commitStatus: "merged",
				removalSafety: "needs_review",
			});
		});

		it("applies lifecycle while preserving commit time when stats omit timestamps", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", lastCommitTs: 123_000 });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: {},
				workspace_statuses: {
					main: { dirty_files: 3, commit_status: "unmerged", removal_safety: "needs_review" },
				},
			});

			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces.main.lastCommitTs).toBe(123_000);
			expect(repositoriesStore.get("/repo")?.workspaces.main.lifecycleStatus?.dirtyFiles).toBe(3);
		});

		it("applies lifecycle status by workspace id", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: {
					main: { branch: "main", path: "/repo", kind: "worktree" },
					"feature-a": { branch: "feature-a", path: "/repo/wt-a", kind: "worktree" },
					"feature-b": { branch: "feature-b", path: "/repo/wt-b", kind: "worktree" },
				},
				merged_branches: ["feature-b"],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: {},
				last_commit_ts: { "feature-a": 1700000000, "feature-b": 1700000000 },
				workspace_statuses: {
					"feature-a": {
						dirty_files: 0,
						commit_status: "unmerged",
						removal_safety: "safe",
					},
					"feature-b": {
						dirty_files: 0,
						commit_status: "merged",
						removal_safety: "safe",
					},
				},
			});

			await gitOps.refreshAllBranchStats();

			const repo = repositoriesStore.get("/repo");
			expect(repo?.workspaces["feature-a"]?.lifecycleStatus?.commitStatus).toBe("unmerged");
			expect(repo?.workspaces["feature-a"]?.isMerged).toBe(false);
			expect(repo?.workspaces["feature-b"]?.lifecycleStatus?.commitStatus).toBe("merged");
			expect(repo?.workspaces["feature-b"]?.isMerged).toBe(true);
		});

		it("Phase 1 updates worktreePath before Phase 2 runs", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			let structureCallOrder = 0;
			let diffStatsCallOrder = 0;
			let callCounter = 0;

			mockRepo.getRepoStructure.mockImplementation(async () => {
				structureCallOrder = ++callCounter;
				return {
					worktree_paths: wtPaths({ main: "/repo", "feature-new": "/repo/wt-new" }),
					merged_branches: [],
				};
			});
			mockRepo.getRepoDiffStats.mockImplementation(async () => {
				diffStatsCallOrder = ++callCounter;
				// By now, Phase 1 should have already updated the store
				const repo = repositoriesStore.get("/repo");
				expect(repo?.workspaces["feature-new"]?.worktreePath).toBe("/repo/wt-new");
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
			expect(repo?.workspaces["feature-new"]?.additions).toBe(3);
			expect(repo?.workspaces["feature-new"]?.deletions).toBe(2);
		});

		it("Phase 2 failure does not corrupt Phase 1 state", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo", "feature-a": "/repo/wt-a" }),
				merged_branches: ["feature-a"],
			});
			mockRepo.getRepoDiffStats.mockRejectedValue(new Error("git diff failed"));

			await gitOps.refreshAllBranchStats();

			const repo = repositoriesStore.get("/repo");
			// Phase 1 state should be intact
			expect(repo?.workspaces["feature-a"]?.worktreePath).toBe("/repo/wt-a");
			expect(repo?.workspaces["feature-a"]?.isMerged).toBe(true);
			// Stats should be at defaults (Phase 2 failed)
			expect(repo?.workspaces["feature-a"]?.additions).toBe(0);
		});

		it("auto-archive runs after Phase 1, before Phase 2", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });

			let archiveCalledBeforeDiffStats = false;

			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo", "merged-branch": "/repo/wt-merged" }),
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

	describe("handleRemoveWorkspace (backend failure)", () => {
		it("keeps the workspace visible when backend removal fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error("git error"));

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeDefined();
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]?.isRemoving).toBe(false);
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Failed to remove feature: git error");
		});

		it("blocks removal when lifecycle safety is unknown", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: null,
				commitStatus: "unknown",
				removalSafety: "unknown",
				error: "parent ref unavailable",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).not.toHaveBeenCalled();
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeDefined();
		});

		it("passes force only after confirming destructive state", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 2,
				dirtyFingerprint: "confirmed-state",
				submoduleUnpushedCommits: [{ path: "plugins", count: 3 }],
				commitStatus: "merged",
				removalSafety: "requires_force",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", {
				branchName: "feature",
				kind: "worktree",
				worktreePath: "/repo/wt",
			});

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", true, true, false, "confirmed-state");
		});

		it("confirms a missing registered checkout without a fabricated fingerprint", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: null,
				missingCheckout: true,
				commitStatus: "in_sync",
				removalSafety: "requires_force",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalled();
			expect(mockRepo.removeWorktree).toHaveBeenCalledWith("/repo", "feature", true, true, false, undefined, true);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("closes branch terminals before removing", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			const id = terminalsStore.add(makeTerminal({ name: "T1" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "feature", id);

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockCloseTerminal).toHaveBeenCalledWith(id, true);
		});

		// Catches: destructive cleanup deleting dirty files or closing sessions after cancellation.
		it("dirty_worktree_cleanup_cancellation_preserves_files_and_sessions", async () => {
			mockDialogs.confirmRemoveWorktree.mockResolvedValue(false);
			const lifecycle = {
				dirtyFiles: 35,
				dirtyFingerprint: "dirty-worktree",
				commitStatus: "merged",
				removalSafety: "destructive",
			};
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce(lifecycle);
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeDefined();
			expect(mockDialogs.confirmRemoveWorktree).toHaveBeenCalledWith("feature", lifecycle, true);
			expect(mockRepo.removeWorktree).not.toHaveBeenCalled();
			expect(mockCloseTerminal).not.toHaveBeenCalled();
		});
	});

	describe("handleRemoveWorkspace (locked worktree)", () => {
		const LOCKED_ERROR = "worktree_locked:fatal: cannot remove a locked working tree, lock reason: claude agent";

		it("asks separately before overriding a missing checkout lock", async () => {
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: null,
				missingCheckout: true,
				commitStatus: "in_sync",
				removalSafety: "requires_force",
			});
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error(LOCKED_ERROR)).mockResolvedValueOnce(undefined);

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockDialogs.confirmRemoveLockedWorktree).toHaveBeenCalledWith("feature", true);
			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(2);
			expect(mockRepo.removeWorktree).toHaveBeenLastCalledWith("/repo", "feature", true, true, true, undefined, true);
		});

		it("shows confirmation dialog when worktree is locked by agent", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error(LOCKED_ERROR));

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			// Dialog receives the requested branch action while the backend
			// independently proves whether deleting that branch is safe.
			expect(mockDialogs.confirmRemoveLockedWorktree).toHaveBeenCalledWith("feature", true);
		});

		it("retries with an explicit lock override after confirmation", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree
				.mockRejectedValueOnce(new Error(LOCKED_ERROR)) // first attempt: locked
				.mockResolvedValueOnce(undefined); // second attempt (lock override): success

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(2);
			expect(mockRepo.removeWorktree).toHaveBeenLastCalledWith("/repo", "feature", true, false, true);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeUndefined();
		});

		it("keeps branch in store when user cancels force removal of locked worktree", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error(LOCKED_ERROR));
			mockDialogs.confirmRemoveLockedWorktree.mockResolvedValue(false);

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(1); // no retry
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeDefined();
		});

		it("keeps branch in store when force removal also fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			mockRepo.removeWorktree
				.mockRejectedValueOnce(new Error(LOCKED_ERROR))
				.mockRejectedValueOnce(new Error("git worktree remove failed (locked): permission denied"));

			await gitOps.handleRemoveWorkspace("/repo", "feature");

			expect(mockRepo.removeWorktree).toHaveBeenCalledTimes(2);
			expect(repositoriesStore.get("/repo")?.workspaces["feature"]).toBeDefined();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to remove"));
		});
	});

	describe("handleRemoveWorkspace (main worktree)", () => {
		const MAIN_ERROR = "worktree_is_main:fatal: '/repo' is a main working tree";

		it("keeps branch in store when worktree is the main repo directory", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feat/main-checkout", { worktreePath: "/repo" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error(MAIN_ERROR));

			await gitOps.handleRemoveWorkspace("/repo", "feat/main-checkout");

			expect(repositoriesStore.get("/repo")?.workspaces["feat/main-checkout"]).toBeDefined();
		});

		it("shows descriptive status message when worktree is main repo", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "feat/main-checkout", { worktreePath: "/repo" });
			mockRepo.removeWorktree.mockRejectedValueOnce(new Error(MAIN_ERROR));

			await gitOps.handleRemoveWorkspace("/repo", "feat/main-checkout");

			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("main worktree"));
		});
	});

	describe("handleRemoveRepo (edge cases)", () => {
		it("closes all branch terminals", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
			const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id1);
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id2);

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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			// No terminals in the branch, so after removal getCount() === 0

			await gitOps.handleRemoveRepo("/repo");

			expect(mockCreateNewTerminal).toHaveBeenCalled();
		});
	});

	describe("handleAddWorktree (dialog flow)", () => {
		it("opens dialog with suggested name and branch lists", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature-1", { worktreePath: "/repo/wt1" });
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
			mockRepo.listLocalBranches.mockResolvedValue(["main"]);
			mockRepo.createWorktree.mockResolvedValue({
				name: "bold-nexus-042",
				path: "/repo/.worktrees/bold-nexus-042",
				workspace_id: "bold-nexus-042",
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.generateWorktreeName.mockResolvedValue("cool-ripley-007");
			mockRepo.listLocalBranches.mockResolvedValue(["main", "develop"]);
			mockRepo.listBaseRefOptions.mockResolvedValue([
				{ name: "develop", kind: "local", is_default: false },
				{ name: "main", kind: "local", is_default: true },
			]);
			mockRepo.createWorktree.mockResolvedValue({
				name: "cool-ripley-007",
				path: "/repo/.worktrees/cool-ripley-007",
				workspace_id: "cool-ripley-007",
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
			mockRepo.listLocalBranches.mockResolvedValue(["main"]);
			mockRepo.createWorktree.mockResolvedValue({
				name: "bold-nexus-042",
				path: "/repo/.worktrees/bold-nexus-042",
				workspace_id: "bold-nexus-042",
				branch: "bold-nexus-042",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			// Open dialog first
			await gitOps.handleAddWorktree("/repo");
			// Confirm creation
			await gitOps.confirmCreateWorktree({
				branchName: "bold-nexus-042",
				createBranch: true,
				baseRef: "main",
			});

			expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "bold-nexus-042", true, "main");
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree bold-nexus-042");
		});

		it("keeps a new worktree through a refresh before Git lists it", async () => {
			vi.setSystemTime(new Date("2026-09-29T12:00:00Z"));
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.createWorktree.mockResolvedValue({
				name: "new",
				path: "/repo/.worktrees/new",
				workspace_id: "new",
				branch: "new",
				base_repo: "/repo",
			});
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({ branchName: "new", createBranch: true, baseRef: "main" });
			await gitOps.refreshAllBranchStats("/repo");

			expect(repositoriesStore.get("/repo")?.workspaces.new).toBeDefined();
		});

		it("creates worktree from existing branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
			mockRepo.listLocalBranches.mockResolvedValue(["main", "develop"]);
			mockRepo.createWorktree.mockResolvedValue({
				name: "develop",
				path: "/repo/.worktrees/develop",
				workspace_id: "develop",
				branch: "develop",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({
				branchName: "develop",
				createBranch: false,
				baseRef: "main",
			});

			expect(mockRepo.createWorktree).toHaveBeenCalledWith("/repo", "develop", false, "main");
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Created worktree develop");
		});

		it("reports error on worktree creation failure", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			mockRepo.generateWorktreeName.mockResolvedValue("bold-nexus-042");
			mockRepo.listLocalBranches.mockResolvedValue(["main"]);
			mockRepo.createWorktree.mockRejectedValue(new Error("branch exists"));

			await gitOps.handleAddWorktree("/repo");
			// confirmCreateWorktree re-throws so the dialog can show the error
			await expect(
				gitOps.confirmCreateWorktree({
					branchName: "bold-nexus-042",
					createBranch: true,
					baseRef: "main",
				}),
			).rejects.toThrow("branch exists");

			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Failed to create worktree"));
		});

		it("runs setupScript after worktree creation", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { setupScript: "npm install" });

			mockRepo.createWorktree.mockResolvedValue({
				name: "feat-test",
				path: "/repo/wt/feat-test",
				workspace_id: "feat-test",
				branch: "feat-test",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({
				branchName: "feat-test",
				createBranch: true,
				baseRef: "main",
			});

			expect(mockRepo.runSetupScript).toHaveBeenCalledWith("npm install", "/repo/wt/feat-test");
		});

		it("does not run setupScript when empty", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

			mockRepo.createWorktree.mockResolvedValue({
				name: "feat-test",
				path: "/repo/wt/feat-test",
				workspace_id: "feat-test",
				branch: "feat-test",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({
				branchName: "feat-test",
				createBranch: true,
				baseRef: "main",
			});

			expect(mockRepo.runSetupScript).not.toHaveBeenCalled();
		});

		it("sets pendingInitCommand from runScript", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { runScript: "npm run dev" });

			mockRepo.createWorktree.mockResolvedValue({
				name: "feat-test",
				path: "/repo/wt/feat-test",
				workspace_id: "feat-test",
				branch: "feat-test",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({
				branchName: "feat-test",
				createBranch: true,
				baseRef: "main",
			});

			// Find the terminal created for this worktree
			const branch = repositoriesStore.get("/repo")?.workspaces["feat-test"];
			expect(branch?.terminals.length).toBeGreaterThan(0);
			const termId = branch!.terminals[0];
			const terminal = terminalsStore.get(termId);
			expect(terminal?.pendingInitCommand).toBe("npm run dev");
		});

		it("warns but continues when setupScript fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { setupScript: "exit 1" });

			mockRepo.createWorktree.mockResolvedValue({
				name: "feat-test",
				path: "/repo/wt/feat-test",
				workspace_id: "feat-test",
				branch: "feat-test",
				base_repo: "/repo",
			});
			mockRepo.runSetupScript.mockResolvedValue({ exit_code: 1, stdout: "", stderr: "failed" });
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleAddWorktree("/repo");
			await gitOps.confirmCreateWorktree({
				branchName: "feat-test",
				createBranch: true,
				baseRef: "main",
			});

			// Should still create a terminal despite script failure
			const branch = repositoriesStore.get("/repo")?.workspaces["feat-test"];
			expect(branch?.terminals.length).toBeGreaterThan(0);
			// Should warn about failure
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Setup script failed"));
		});
	});

	describe("handleCreateWorktreeFromBranch", () => {
		it("runs setupScript after clone-worktree creation", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { setupScript: "npm ci" });

			mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
			mockRepo.createWorktree.mockResolvedValue({
				name: "main--wt-42",
				path: "/repo/wt/main--wt-42",
				workspace_id: "main--wt-42",
				branch: "main--wt-42",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

			expect(mockRepo.runSetupScript).toHaveBeenCalledWith("npm ci", "/repo/wt/main--wt-42");
		});

		it("sets pendingInitCommand from runScript on clone-worktree", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { runScript: "make dev" });

			mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
			mockRepo.createWorktree.mockResolvedValue({
				name: "main--wt-42",
				path: "/repo/wt/main--wt-42",
				workspace_id: "main--wt-42",
				branch: "main--wt-42",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

			const branch = repositoriesStore.get("/repo")?.workspaces["main--wt-42"];
			expect(branch?.terminals.length).toBeGreaterThan(0);
			const termId = branch!.terminals[0];
			const terminal = terminalsStore.get(termId);
			expect(terminal?.pendingInitCommand).toBe("make dev");
		});

		it("does not run scripts when none configured", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			mockRepo.generateCloneBranchName.mockResolvedValue("main--wt-42");
			mockRepo.createWorktree.mockResolvedValue({
				name: "main--wt-42",
				path: "/repo/wt/main--wt-42",
				workspace_id: "main--wt-42",
				branch: "main--wt-42",
				base_repo: "/repo",
			});
			mockRepo.getDiffStats.mockResolvedValue({ additions: 0, deletions: 0 });

			await gitOps.handleCreateWorktreeFromBranch("/repo", "main");

			expect(mockRepo.runSetupScript).not.toHaveBeenCalled();
			const branch = repositoriesStore.get("/repo")?.workspaces["main--wt-42"];
			const termId = branch!.terminals[0];
			expect(terminalsStore.get(termId)?.pendingInitCommand).toBeNull();
		});
	});

	describe("executeRunCommand", () => {
		it("creates terminal and waits for session to send command", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			await gitOps.executeRunCommand("npm test");

			// Should save command and create terminal
			const branch = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(branch?.runCommand).toBe("npm test");
			expect(terminalsStore.getCount()).toBeGreaterThan(0);

			// Simulate session becoming available
			const ids = terminalsStore.getIds();
			terminalsStore.update(ids.at(-1)!, { sessionId: "sess-run" });

			await vi.advanceTimersByTimeAsync(500);

			expect(mockPty.write).toHaveBeenCalledWith("sess-run", "npm test\n");
		});

		it("does nothing when no active repo/branch", async () => {
			await gitOps.executeRunCommand("npm test");

			expect(terminalsStore.getCount()).toBe(0);
		});

		it("shows status when max sessions reached", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			mockPty.canSpawn.mockResolvedValue(false);

			await gitOps.executeRunCommand("npm test");

			expect(mockSetStatusInfo).toHaveBeenCalledWith("Max sessions reached (50)");
		});

		it("truncates long command names in tab", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			await gitOps.executeRunCommand("npm run test:integration:coverage --verbose");

			const ids = terminalsStore.getIds();
			const t = terminalsStore.get(ids.at(-1)!);
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			await customGitOps.executeRunCommand("npm run test:integration");

			const ids = terminalsStore.getIds();
			const t = terminalsStore.get(ids.at(-1)!);
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repositoriesStore.setRunCommand("/repo", "main", "npm test");

			const openDialog = vi.fn();
			gitOps.handleRunCommand(false, openDialog);

			expect(openDialog).not.toHaveBeenCalled();
		});

		it("opens dialog when forceDialog is true even with saved command", () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
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

			expect(openSettingsPanel).toHaveBeenCalledWith({
				kind: "repo",
				repoPath: "/repo",
			});
		});

		it("leaves the side panels on the active repo", () => {
			// Settings for another repo open in the settings panel only. Moving the
			// Notes, Git and Files panels there too was the drift this derivation removes.
			repositoriesStore.add({ path: "/active", displayName: "Active" });
			repositoriesStore.add({ path: "/other", displayName: "Other" });
			repositoriesStore.setActive("/active");

			gitOps.handleRepoSettings("/other", vi.fn());

			expect(gitOps.currentRepoPath()).toBe("/active");
		});
	});

	describe("handleRenameBranch (edge cases)", () => {
		it("does nothing when no branchToRename set", async () => {
			await gitOps.handleRenameBranch("old", "new");

			expect(mockRepo.renameBranch).not.toHaveBeenCalled();
		});

		it("does not update currentBranch if renaming non-active branch", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			gitOps.setBranchToRename({ repoPath: "/repo", branchName: "feature" });

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
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/new-repo" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.handleAddRepo();

			expect(repositoriesStore.get("/new-repo")).toBeDefined();
			expect(repositoriesStore.get("/new-repo")?.displayName).toBe("new-repo");
			expect(repositoriesStore.get("/new-repo")?.activeWorkspaceId).toBe("main");
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
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ develop: "/array-repo" }),
				merged_branches: [],
			});
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
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/fresh-repo" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });

			await gitOps.handleAddRepo();

			const branch = repositoriesStore.get("/fresh-repo")?.workspaces["main"];
			// Must create exactly 1 terminal — not 2 from double-spawn chain
			expect(branch?.terminals.length).toBe(1);
			expect(terminalsStore.state.activeId).toBe(branch?.terminals[0]);
		});

		it("closes orphan terminals when adding repo", async () => {
			// Create an orphan terminal (not tracked by any branch)
			const orphanId = terminalsStore.add(makeTerminal({ name: "orphan" }));

			vi.mocked(open).mockResolvedValue("/new-repo");
			mockRepo.getInfo.mockResolvedValue({
				path: "/new-repo",
				name: "new-repo",
				initials: "NR",
				branch: "main",
				status: "clean",
			});
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/new-repo" }),
				merged_branches: [],
			});
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
			const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
				const url = String(input);
				if (!url.endsWith("/watchers/hot-repos") && !url.endsWith("/watchers/repo?path=%2Fbrowser-repo")) {
					throw new Error(`unexpected browser request: ${url}`);
				}
				return new Response(JSON.stringify({ ok: true }), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			});
			onTestFinished(() => fetchMock.mockRestore());
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
			expect(fetchMock).toHaveBeenCalledWith(
				"http://localhost:3000/watchers/repo?path=%2Fbrowser-repo",
				expect.objectContaining({ method: "POST" }),
			);
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
		it("opens the remote checkout without waiting for the sidebar refresh window", async () => {
			// Catches: remote checkout awaits a throttled refresh before selecting its workspace and opening a terminal.
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo" }),
				merged_branches: [],
			});
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {}, workspace_statuses: {} });
			await gitOps.refreshAllBranchStats("/repo");
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ "feat-remote": "/repo" }),
				merged_branches: [],
			});
			mockRepo.checkoutRemoteBranch.mockResolvedValue(undefined);

			await gitOps.handleCheckoutRemoteBranch("/repo", "feat-remote");

			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("feat-remote");
			expect(mockRepo.getRepoStructure).toHaveBeenCalledTimes(2);
			expect(repositoriesStore.get("/repo")?.workspaces["feat-remote"].terminals).toHaveLength(1);
		});

		it("calls repo.checkoutRemoteBranch and refreshes branch lists", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.listLocalBranches.mockResolvedValue(["main", "feat-remote"]);
			mockRepo.checkoutRemoteBranch.mockResolvedValue(undefined);

			await gitOps.handleCheckoutRemoteBranch("/repo", "feat-remote");

			expect(mockRepo.checkoutRemoteBranch).toHaveBeenCalledWith("/repo", "feat-remote");
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Checked out feat-remote");
		});

		it("reports error when checkout fails", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.checkoutRemoteBranch.mockRejectedValue(new Error("branch already exists"));

			await gitOps.handleCheckoutRemoteBranch("/repo", "feat-remote");

			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Checkout failed"));
		});
	});

	describe("handleSwitchBranch (dirty + stash recovery)", () => {
		const seedRepo = () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		};

		it("declining the stash prompt aborts without a second switch attempt", async () => {
			seedRepo();
			mockDialogs.confirmStashAndSwitch.mockResolvedValueOnce(false);
			mockRepo.switchBranch.mockRejectedValueOnce("dirty");

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockRepo.switchBranch).toHaveBeenCalledTimes(1);
			expect(mockDialogs.reportGitError).not.toHaveBeenCalled();
		});

		it("stashes and switches when the user confirms", async () => {
			seedRepo();
			mockRepo.switchBranch
				.mockRejectedValueOnce("dirty")
				.mockResolvedValueOnce({ success: true, stashed: true, previous_branch: "main", new_branch: "feature" });

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockDialogs.confirmStashAndSwitch).toHaveBeenCalledWith("feature");
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Switched to feature (changes stashed)");
			expect(mockDialogs.reportGitError).not.toHaveBeenCalled();
		});

		it("offers a retry on a stale-lock stash failure and shows the full git output", async () => {
			seedRepo();
			const lockErr = "Stash failed: git exited with code 1: error: could not write index";
			mockRepo.switchBranch.mockRejectedValueOnce("dirty").mockRejectedValueOnce(lockErr);

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockDialogs.reportGitError).toHaveBeenCalledWith(
				"Stash & switch failed",
				expect.stringContaining("could not write index"),
				true,
			);
			// The full stderr is surfaced, not a truncated one-liner.
			expect(mockDialogs.reportGitError).toHaveBeenCalledWith(
				"Stash & switch failed",
				expect.stringContaining("stale git lock"),
				true,
			);
		});

		it("retries the stash+switch when the user clicks Retry and it then succeeds", async () => {
			seedRepo();
			mockDialogs.reportGitError.mockResolvedValueOnce(true);
			mockRepo.switchBranch
				.mockRejectedValueOnce("dirty")
				.mockRejectedValueOnce("error: could not write index")
				.mockResolvedValueOnce({ success: true, stashed: true, previous_branch: "main", new_branch: "feature" });

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockRepo.switchBranch).toHaveBeenCalledTimes(3);
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Switched to feature (changes stashed)");
		});

		it("does not offer a retry for a non-lock stash failure", async () => {
			seedRepo();
			mockRepo.switchBranch
				.mockRejectedValueOnce("dirty")
				.mockRejectedValueOnce("Stash failed: No space left on device");

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockDialogs.reportGitError).toHaveBeenCalledWith(
				"Stash & switch failed",
				expect.stringContaining("No space left on device"),
				false,
			);
		});

		it("surfaces a non-dirty switch failure in the error dialog", async () => {
			seedRepo();
			mockRepo.switchBranch.mockRejectedValueOnce("Checkout failed: local changes would be overwritten");

			await gitOps.handleSwitchBranch("/repo", "feature");

			expect(mockDialogs.confirmStashAndSwitch).not.toHaveBeenCalled();
			expect(mockDialogs.reportGitError).toHaveBeenCalledWith(
				"Branch switch failed",
				expect.stringContaining("local changes would be overwritten"),
				false,
			);
		});
	});

	describe("orphan cleanup deletion guard (critic-1188)", () => {
		const live = (path: string) => ({
			path,
			safe: false,
			reason: "live session: Claude: refactor",
			live_sessions: [{ session_id: "s1", name: "Claude: refactor" }],
		});
		const gitOpsWith = (
			confirmOrphanCleanup: NonNullable<Parameters<typeof useGitOperations>[0]["dialogs"]["confirmOrphanCleanup"]>,
		) =>
			useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});

		beforeEach(() => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
			repoSettingsStore.getOrCreate("/repo", "Repo");
		});

		// Catches: closing the worktree's terminals BEFORE the backend guard has accepted the
		// removal — a session that started after the assessment is killed, then the removal is refused.
		it("auto mode: does not close terminals of a checkout whose removal the backend refuses", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/late", safe: true }]);
			mockRepo.removeOrphanWorktree.mockRejectedValueOnce(new Error("live session: Claude: late"));
			terminalsStore.add(makeTerminal({ name: "Late agent", cwd: "/wt/late" }));

			await gitOps.refreshAllBranchStats();

			expect(mockCloseTerminal).not.toHaveBeenCalled();
		});

		// Catches: the confirmed (safeOnly=false) removal closing terminals before the backend
		// refused it because a session started after the dialog was shown.
		it("ask mode: a refused confirmed removal closes no terminal and sends the seen session ids", async () => {
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(true);
			mockRepo.assessOrphanCleanup.mockResolvedValue([live("/wt/busy")]);
			mockRepo.removeOrphanWorktree.mockRejectedValueOnce(new Error("live session: Claude: new; not confirmed"));
			terminalsStore.add(makeTerminal({ name: "New agent", cwd: "/wt/busy" }));

			await gitOpsWith(confirmOrphanCleanup).refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/busy", false, ["s1"]);
			expect(mockCloseTerminal).not.toHaveBeenCalled();
		});

		// Catches: a removal that fails (a Windows lock on a checkout a PTY still holds) being reported as
		// done, or its terminals being closed anyway, leaving a half-removed checkout with no terminal.
		it("auto mode: a removal error is logged as a failure, closes nothing and reports no removal", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/locked", safe: true }]);
			mockRepo.removeOrphanWorktree.mockRejectedValueOnce(new Error("The process cannot access the file"));
			terminalsStore.add(makeTerminal({ name: "Holds lock", cwd: "/wt/locked" }));

			await gitOps.refreshAllBranchStats();

			expect(mockCloseTerminal).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Removed"));
			expect(appLogger.getEntries()).toContainEqual(
				expect.objectContaining({
					level: "warn",
					message: "Failed to auto-remove orphan worktree /wt/locked",
				}),
			);
		});

		// Catches: a terminal that fails to close after the removal turning a removed checkout into
		// "Failed to remove" with no count in the status line.
		it("auto mode: a close error after a successful removal still counts the removal", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/gone", safe: true }]);
			mockCloseTerminal.mockRejectedValueOnce(new Error("pty already closed"));
			terminalsStore.add(makeTerminal({ name: "Stuck", cwd: "/wt/gone" }));

			await gitOps.refreshAllBranchStats();

			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Removed 1 orphaned worktree(s); 1 terminal(s) could not be closed",
			);
			expect(appLogger.getEntries()).not.toContainEqual(
				expect.objectContaining({ message: "Failed to auto-remove orphan worktree /wt/gone" }),
			);
		});

		// Catches: terminals left alive on a removed checkout with only a log line: the status line must
		// say how many could not be closed, counting every failing terminal, not just the first.
		it("auto mode: the status line sums the terminals that could not be closed across several removed orphans", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([
				{ path: "/wt/a", safe: true },
				{ path: "/wt/b", safe: true },
			]);
			for (let i = 0; i < 3; i++) mockCloseTerminal.mockRejectedValueOnce(new Error("pty busy"));
			terminalsStore.add(makeTerminal({ name: "A1", cwd: "/wt/a" }));
			terminalsStore.add(makeTerminal({ name: "A2", cwd: "/wt/a/sub" }));
			terminalsStore.add(makeTerminal({ name: "B1", cwd: "/wt/b" }));

			await gitOps.refreshAllBranchStats();

			expect(mockCloseTerminal).toHaveBeenCalledTimes(3);
			expect(mockSetStatusInfo).toHaveBeenCalledTimes(1);
			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Removed 2 orphaned worktree(s); 3 terminal(s) could not be closed",
			);
		});

		// Catches: widening the Auto-mode review to every unsafe orphan, which would pop a dialog
		// for dirty checkouts that Auto mode has always skipped silently.
		it("auto mode: a dirty orphan without a live session stays skipped and opens no dialog", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/dirty", safe: false, reason: "untracked files" }]);
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(true);

			await gitOpsWith(confirmOrphanCleanup).refreshAllBranchStats();

			expect(confirmOrphanCleanup).not.toHaveBeenCalled();
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		// Catches: a Keep on a live orphan not being remembered, so every refresh poll
		// re-opens the same review dialog.
		it("auto mode: a Keep on a live orphan is not asked again on the next refresh", async () => {
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockResolvedValue([live("/wt/busy")]);
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
			const ops = gitOpsWith(confirmOrphanCleanup);

			await ops.refreshAllBranchStats();
			vi.advanceTimersByTime(5_000);
			await ops.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		// Catches: the confirmed removal of a live orphan going out as safeOnly=true (the backend
		// then refuses the very removal the user just approved), or a safe sibling losing its recheck.
		it("ask mode: after confirm the live orphan is removed unchecked and the safe one rechecked", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/idle", safe: true }, live("/wt/busy")]);
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(true);

			await gitOpsWith(confirmOrphanCleanup).refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/busy", false, ["s1"]);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/idle", true);
		});
	});

	describe("orphan worktree cleanup", () => {
		beforeEach(() => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
		});

		// The ask-mode dialog only reviews what the backend judged unsafe: a safe orphan is removed unasked.
		const dirtyOrphans = () =>
			mockRepo.assessOrphanCleanup.mockImplementation(async (repoPath: string) =>
				(await mockRepo.detectOrphanWorktrees(repoPath)).map((path: string) => ({
					path,
					safe: false,
					reason: "untracked files",
				})),
			);

		it("never auto-removes a dirty orphan", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/dirty"]);
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/dirty", safe: false, reason: "untracked files" }]);

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		// Catches: Auto mode closing the terminals of and removing a clean detached
		// checkout while an agent session still works in it.
		it("holds a live-session orphan for review instead of auto-removing it (orphanCleanup=on)", async () => {
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
			const onGitOps = useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			const live = {
				path: "/wt/busy",
				safe: false,
				reason: "live session: Claude: refactor",
				live_sessions: [{ session_id: "s1", name: "Claude: refactor" }],
			};
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/idle", safe: true }, live]);
			terminalsStore.add(makeTerminal({ name: "Busy", cwd: "/wt/busy" }));

			await onGitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledTimes(1);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/idle", true);
			expect(confirmOrphanCleanup).toHaveBeenCalledWith("/repo", [live], 10);
			expect(mockCloseTerminal).not.toHaveBeenCalled();
		});

		// Catches: the confirmation listing paths only, so the user approves removal
		// without learning an agent is running there.
		it("passes the live session to the confirmation and removes after approval (orphanCleanup=ask)", async () => {
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
			const live = {
				path: "/wt/busy",
				safe: false,
				reason: "live session: Claude: refactor",
				live_sessions: [{ session_id: "s1", name: "Claude: refactor" }],
			};
			mockRepo.assessOrphanCleanup.mockResolvedValue([live]);

			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledWith("/repo", [live], 10);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/busy", false, ["s1"]);
		});

		it("auto-removes orphans silently when orphanCleanup=on", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/detached-1", true);
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Removed 1 orphaned worktree(s)");
		});

		it("reports only successfully removed orphans after a partial failure", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/blocked", "/wt/removed"]);
			mockRepo.removeOrphanWorktree.mockImplementation(async (_repoPath: string, wtPath: string) => {
				if (wtPath === "/wt/blocked") throw new Error("permission denied");
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledTimes(2);
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Removed 1 orphaned worktree(s)");
			expect(mockSetStatusInfo).not.toHaveBeenCalledWith("Removed 2 orphaned worktree(s)");
		});

		it("does not claim removal when every orphan cleanup fails", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/blocked"]);
			mockRepo.removeOrphanWorktree.mockRejectedValueOnce(new Error("permission denied"));

			await gitOps.refreshAllBranchStats();

			expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Removed"));
		});

		it("closes terminals in orphan worktree after the backend accepts auto-removal (orphanCleanup=on)", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

			const termInOrphan = terminalsStore.add(makeTerminal({ name: "In orphan", cwd: "/wt/detached-1/subdir" }));
			const termElsewhere = terminalsStore.add(makeTerminal({ name: "Elsewhere", cwd: "/repo" }));

			await gitOps.refreshAllBranchStats();

			expect(mockCloseTerminal).toHaveBeenCalledWith(termInOrphan, true);
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(termElsewhere, true);
			// the backend accepts the removal first; terminals are closed only after it
			const closeOrder = mockCloseTerminal.mock.invocationCallOrder[0];
			const removeOrder = mockRepo.removeOrphanWorktree.mock.invocationCallOrder[0];
			expect(removeOrder).toBeLessThan(closeOrder);
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
			dirtyOrphans();

			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledWith(
				"/repo",
				[{ path: "/wt/detached-1", safe: false, reason: "untracked files" }],
				10,
			);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/detached-1", false, []);
			expect(mockRepo.beginOrphanCleanup).toHaveBeenCalledWith("/repo", ["/wt/detached-1"]);
			expect(mockRepo.clearOrphanCleanup).toHaveBeenCalledWith("/repo", false);
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Removed 1 orphaned worktree(s)");
		});

		it("lets an agent keep a pending orphan before the countdown removes it", async () => {
			let resolveDialog!: (remove: boolean) => void;
			const confirmOrphanCleanup = vi.fn(
				() =>
					new Promise<boolean>((resolve) => {
						resolveDialog = resolve;
					}),
			);
			const answerOrphanCleanup = vi.fn((_repoPath: string, remove: boolean) => resolveDialog(remove));
			const askGitOps = useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup, answerOrphanCleanup },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();
			mockRepo.pendingOrphanCleanupAnswer.mockResolvedValue(false);
			const refresh = askGitOps.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(500);
			await refresh;
			expect(answerOrphanCleanup).toHaveBeenCalledWith("/repo", false);
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
			expect(mockRepo.clearOrphanCleanup).toHaveBeenCalledWith("/repo", true);
		});

		// Catches: a UI Keep that never reaches the backend, so another client's countdown for the
		// same worktree keeps running and removes it (story 1289-27f8).
		it("records a Keep click on the backend so other clients stop their countdown", async () => {
			const askGitOps = useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup: vi.fn().mockResolvedValue(false) },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();

			await askGitOps.refreshAllBranchStats();

			expect(mockRepo.clearOrphanCleanup).toHaveBeenCalledWith("/repo", true);
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		it("honors an agent Keep answer that arrives at the end of the countdown", async () => {
			const askGitOps = useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup: vi.fn().mockResolvedValue(true) },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();
			mockRepo.pendingOrphanCleanupAnswer.mockResolvedValue(false);
			await askGitOps.refreshAllBranchStats();
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		it("keeps an orphan when Ask mode has no confirmation dialog", async () => {
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo": { additions: 3, deletions: 1 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Removed"));
			expect(repositoriesStore.get("/repo")?.workspaces.main?.additions).toBe(3);
		});

		it("still updates branch stats when orphan detection fails", async () => {
			mockRepo.detectOrphanWorktrees.mockRejectedValueOnce(new Error("scan unavailable"));
			mockRepo.getRepoDiffStats.mockResolvedValue({
				diff_stats: { "/repo": { additions: 7, deletions: 2 } },
				last_commit_ts: {},
			});

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces.main?.additions).toBe(7);
		});

		it("scans orphans for a Git repository whose workspace list starts empty", async () => {
			repositoriesStore.removeWorkspace("/repo", "main");
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: {}, merged_branches: [] });
			mockRepo.getInfo.mockResolvedValue({ branch: "main", is_git_repo: true });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/old"]);

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/old", true);
		});

		it("does not report a removal after confirmed orphan cleanup fails", async () => {
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
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/blocked"]);
			dirtyOrphans();
			mockRepo.removeOrphanWorktree.mockRejectedValueOnce(new Error("permission denied"));

			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledWith(
				"/repo",
				[{ path: "/wt/blocked", safe: false, reason: "untracked files" }],
				10,
			);
			expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Removed"));
		});

		it("closes terminals in orphan worktree after the backend accepts the confirmed removal (orphanCleanup=ask)", async () => {
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
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();

			const termInOrphan = terminalsStore.add(makeTerminal({ name: "In orphan", cwd: "/wt/detached-1" }));
			const termElsewhere = terminalsStore.add(makeTerminal({ name: "Elsewhere", cwd: "/other" }));

			await askGitOps.refreshAllBranchStats();

			expect(mockCloseTerminal).toHaveBeenCalledWith(termInOrphan, true);
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(termElsewhere, true);
			const closeOrder = mockCloseTerminal.mock.invocationCallOrder[0];
			const removeOrder = mockRepo.removeOrphanWorktree.mock.invocationCallOrder[0];
			expect(removeOrder).toBeLessThan(closeOrder);
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
			dirtyOrphans();

			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalled();
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		it("does not re-prompt for an orphan the user chose to keep (#65)", async () => {
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
			// Same orphan detected on every refresh
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();

			// First refresh: user clicks "Keep" (cancel)
			await askGitOps.refreshAllBranchStats();
			// Subsequent refreshes must NOT re-open the dialog for the kept orphan
			vi.advanceTimersByTime(5_000);
			await askGitOps.refreshAllBranchStats();
			vi.advanceTimersByTime(5_000);
			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		const askOps = (
			confirmOrphanCleanup: NonNullable<Parameters<typeof useGitOperations>[0]["dialogs"]["confirmOrphanCleanup"]>,
		) =>
			useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
		const dirty = (fingerprint: string) => ({
			path: "/wt/hooked",
			safe: false,
			reason: "untracked files",
			dirty_fingerprint: fingerprint,
		});

		// Catches: "ask" mode opening a dialog for a clean, merged detached checkout, so a worktree that holds
		// nothing keeps interrupting the user until they click Keep or Remove (orphan-dialog-repeats).
		it("removes a safe orphan without asking when orphanCleanup=ask", async () => {
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/clean", safe: true }]);

			await askOps(confirmOrphanCleanup).refreshAllBranchStats();

			expect(confirmOrphanCleanup).not.toHaveBeenCalled();
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/clean", true);
		});

		// Catches: a Keep that is forgotten when the dirty state is unchanged, or that outlives a change:
		// the same fingerprint stays quiet, a different one is judged afresh.
		it("a Keep holds only while the orphan's dirty fingerprint is unchanged", async () => {
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
			const ops = askOps(confirmOrphanCleanup);
			mockRepo.assessOrphanCleanup.mockResolvedValue([dirty("fp-1")]);

			await ops.refreshAllBranchStats();
			vi.advanceTimersByTime(5_000);
			await ops.refreshAllBranchStats();
			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);

			mockRepo.assessOrphanCleanup.mockResolvedValue([dirty("fp-2")]);
			vi.advanceTimersByTime(5_000);
			await ops.refreshAllBranchStats();
			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
		});

		// Catches: a kept orphan that has since become clean staying parked behind its old Keep instead of
		// being re-evaluated and removed.
		it("a kept orphan that became clean is removed without asking", async () => {
			const confirmOrphanCleanup = vi.fn().mockResolvedValue(false);
			const ops = askOps(confirmOrphanCleanup);
			mockRepo.assessOrphanCleanup.mockResolvedValue([dirty("fp-1")]);
			await ops.refreshAllBranchStats();

			mockRepo.assessOrphanCleanup.mockResolvedValue([
				{ path: "/wt/hooked", safe: true, dirty_fingerprint: "fp-clean" },
			]);
			vi.advanceTimersByTime(5_000);
			await ops.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/hooked", true);
		});

		it("offers a newly found orphan after the user kept an earlier one", async () => {
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
			mockRepo.detectOrphanWorktrees.mockResolvedValueOnce(["/wt/kept"]).mockResolvedValueOnce(["/wt/kept", "/wt/new"]);
			dirtyOrphans();

			await askGitOps.refreshAllBranchStats();
			vi.advanceTimersByTime(5_000);
			await askGitOps.refreshAllBranchStats();

			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
			expect(confirmOrphanCleanup).toHaveBeenNthCalledWith(
				2,
				"/repo",
				[{ path: "/wt/new", safe: false, reason: "untracked files" }],
				10,
			);
		});

		it("does nothing when orphanCleanup=off", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "off" });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.detectOrphanWorktrees).not.toHaveBeenCalled();
			expect(mockRepo.removeOrphanWorktree).not.toHaveBeenCalled();
		});

		it("suppresses duplicate dialog when two repos have orphans in same refresh", async () => {
			// Two repos processed in parallel by Promise.all — both have orphans.
			// The second should see orphanDialogOpen=true and skip.
			repositoriesStore.add({ path: "/repo2", displayName: "Repo2" });
			repositoriesStore.setWorkspace("/repo2", "main", { worktreePath: "/repo2" });

			let resolveDialog!: (value: boolean) => void;
			const confirmOrphanCleanup = vi.fn().mockImplementation(
				() =>
					new Promise<boolean>((r) => {
						resolveDialog = r;
					}),
			);
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

			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
			mockRepo.detectOrphanWorktrees.mockResolvedValue(["/wt/detached-1"]);
			dirtyOrphans();

			// Single refresh processes both repos in parallel via Promise.all
			const p = askGitOps.refreshAllBranchStats();

			// Yield microtasks so both repos reach handleOrphanCleanup
			await vi.advanceTimersByTimeAsync(0);
			resolveDialog(true);
			await p;

			// Dialog should only have been shown once despite two repos having orphans
			expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo", "feature/x": "/repo/.worktrees/feature-x" }),
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

		it("keeps an untouched worktree occupied by a live agent during automatic archive", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 0,
				commitStatus: "in_sync",
				removalSafety: "safe",
				liveSessions: [{ sessionId: "pty-active", name: "Codex: gate work" }],
				warnings: ["This branch has nothing of its own, not merged work", "Live session: Codex: gate work"],
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Codex: gate work"));
		});

		it("keeps a merged clean worktree while an agent runs in it", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 0,
				commitStatus: "merged",
				removalSafety: "safe",
				liveSessions: [{ sessionId: "pty-active", name: "Codex: gate work" }],
				warnings: ["Live session: Codex: gate work"],
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Codex: gate work"));
		});

		it.each([
			["dirty files", { dirtyFiles: 1, removalSafety: "safe" }, "Uncommitted files"],
			["unsafe removal", { dirtyFiles: 0, removalSafety: "needs_review" }, "Removal needs review"],
		] as const)("keeps a merged worktree with %s", async (_case, preview, warning) => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				...preview,
				commitStatus: "merged",
				liveSessions: [],
				warnings: [warning],
			});

			vi.advanceTimersByTime(5_000);

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining(warning));
		});

		it("keeps an unmerged linked worktree out of automatic archive", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({ main: "/repo", "feature/x": "/repo/.worktrees/feature-x" }),
				merged_branches: [],
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.getWorkspaceLifecycle).not.toHaveBeenCalled();
			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
		});

		it("does not archive a merged row whose worktree path is missing", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			const tid = terminalsStore.add(makeTerminal({ name: "Open on old row", cwd: "/repo" }));
			repositoriesStore.setWorkspace("/repo", "old-row", { worktreePath: null, isMerged: true });
			repositoriesStore.addTerminalToWorkspace("/repo", "old-row", tid);
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["old-row"]).toBeDefined();
			expect(mockRepo.getWorkspaceLifecycle).not.toHaveBeenCalledWith("/repo", "old-row");
			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalledWith("/repo", "old-row", "archive");
		});

		it("reports review required when archive refuses an uncommitted worktree", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.finalizeMergedWorktree.mockResolvedValueOnce({
				merged: true,
				action: "needs_confirmation",
				archive_path: null,
			});

			await gitOps.refreshAllBranchStats();

			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Auto-archived 0 merged worktree(s), kept 1: feature/x: uncommitted work",
			);
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
		});

		it("continues archiving another worktree when one backend archive fails", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({
					main: "/repo",
					"feature/x": "/repo/.worktrees/feature-x",
					"feature/y": "/repo/.worktrees/feature-y",
				}),
				merged_branches: ["feature/x", "feature/y"],
			});
			mockRepo.finalizeMergedWorktree.mockRejectedValueOnce(new Error("locked worktree"));

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).toHaveBeenCalledWith("/repo", "feature/y", "archive");
			expect(mockSetStatusInfo).toHaveBeenCalledWith("Auto-archived 1 merged worktree(s)");
			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
		});

		it("does not claim automatic archive success when the only archive fails", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.finalizeMergedWorktree.mockRejectedValueOnce(new Error("locked worktree"));

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			expect(mockSetStatusInfo).not.toHaveBeenCalledWith(expect.stringContaining("Auto-archived"));
		});

		it("keeps a merged worktree when its safety preview fails", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getWorkspaceLifecycle.mockRejectedValueOnce(new Error("preview unavailable"));

			await gitOps.refreshAllBranchStats();

			expect(repositoriesStore.get("/repo")?.workspaces["feature/x"]).toBeDefined();
			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Auto-archived 0 merged worktree(s), kept 1: feature/x: removal preview failed",
			);
		});

		it("reports every unsafe archive reason and a fallback when warnings are missing", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getRepoStructure.mockResolvedValue({
				worktree_paths: wtPaths({
					main: "/repo",
					"feature/x": "/repo/.worktrees/x",
					"feature/y": "/repo/.worktrees/y",
					"feature/z": "/repo/.worktrees/z",
				}),
				merged_branches: ["feature/x", "feature/y", "feature/z"],
			});
			const unsafe = { dirtyFiles: 1, commitStatus: "merged", removalSafety: "needs_review", liveSessions: [] };
			mockRepo.getWorkspaceLifecycle
				.mockResolvedValueOnce(unsafe)
				.mockResolvedValueOnce({ ...unsafe, warnings: ["dirty", "busy"] })
				.mockResolvedValueOnce({ ...unsafe, warnings: ["unsafe"] });

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
			expect(mockSetStatusInfo).toHaveBeenCalledWith(
				"Auto-archived 0 merged worktree(s), kept 3: feature/x: removal needs review | feature/y: dirty; busy | feature/z: unsafe",
			);
		});

		it("keeps an untouched clean worktree even when the old merged list includes it", async () => {
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { autoArchiveMerged: true });
			mockRepo.getWorkspaceLifecycle.mockResolvedValueOnce({
				dirtyFiles: 0,
				commitStatus: "in_sync",
				removalSafety: "safe",
				liveSessions: [],
				warnings: ["This branch has nothing of its own, not merged work"],
			});

			await gitOps.refreshAllBranchStats();

			expect(mockRepo.finalizeMergedWorktree).not.toHaveBeenCalled();
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
				worktree_paths: wtPaths({ main: "/repo" }),
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
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");

			mockPty.write.mockRejectedValue(new Error("write failed"));

			await gitOps.executeRunCommand("failing-cmd");

			const ids = terminalsStore.getIds();
			terminalsStore.update(ids.at(-1)!, { sessionId: "sess-fail" });

			await vi.advanceTimersByTimeAsync(500);

			expect(appLogger.getEntries()).toContainEqual(
				expect.objectContaining({
					level: "error",
					source: "terminal",
					message: "Failed to send run command",
					data: expect.objectContaining({ message: "write failed" }),
				}),
			);
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
			// Set up repo with main branch and a worktree branch
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setWorkspace("/repo", "feature-x", { worktreePath: "/repo/.worktrees/feature-x" });
		});

		it("reassigns terminal from main to worktree branch on cwd change", async () => {
			const id = addTerminal({ sessionId: "s1", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			terminalsStore.setActive(id);

			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
			await vi.advanceTimersByTimeAsync(300);

			const main = repositoriesStore.get("/repo")?.workspaces["main"];
			const feature = repositoriesStore.get("/repo")?.workspaces["feature-x"];
			expect(main?.terminals).not.toContain(id);
			expect(feature?.terminals).toContain(id);
			// The active terminal moved, so the derived branch follows it.
			expect(gitOps.currentBranch()).toBe("feature-x");
		});

		it("does nothing when cwd maps to the same branch", async () => {
			const id = addTerminal({ sessionId: "s2", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);

			gitOps.handleTerminalCwdChange(id, "/repo/src/deep/folder");
			await vi.advanceTimersByTimeAsync(300);

			// Still on main — /repo/src is a subdirectory of /repo (main's worktreePath)
			const main = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(main?.terminals).toContain(id);
		});

		it("longest prefix wins when worktrees nest", async () => {
			const id = addTerminal({ sessionId: "s3", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setActive(id);

			// cwd inside the feature-x worktree subdirectory
			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x/src/components");
			await vi.advanceTimersByTimeAsync(300);

			const feature = repositoriesStore.get("/repo")?.workspaces["feature-x"];
			expect(feature?.terminals).toContain(id);
		});

		it("does not match repo-old when cwd is /repo (boundary check)", async () => {
			// Add a second repo whose path is a string-prefix of "/repo" but not a path-prefix
			repositoriesStore.add({ path: "/repo-old", displayName: "RepoOld" });
			repositoriesStore.setWorkspace("/repo-old", "main", { worktreePath: "/repo-old" });
			const id = addTerminal({ sessionId: "s4", cwd: "/repo-old" });
			repositoriesStore.addTerminalToWorkspace("/repo-old", "main", id);

			// cwd "/repo" should NOT match "/repo-old" (the "/" boundary guard prevents it)
			gitOps.handleTerminalCwdChange(id, "/repo");
			await vi.advanceTimersByTimeAsync(300);

			// Terminal should have moved away from /repo-old
			const repoOldMain = repositoriesStore.get("/repo-old")?.workspaces["main"];
			expect(repoOldMain?.terminals).not.toContain(id);
		});

		it("does nothing for cwd outside all known repos", async () => {
			const id = addTerminal({ sessionId: "s5", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);

			gitOps.handleTerminalCwdChange(id, "/tmp/random/path");
			await vi.advanceTimersByTimeAsync(300);

			// Should still be on main — no reassignment
			const main = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(main?.terminals).toContain(id);
		});

		it("debounces rapid cwd changes — only the last one takes effect", async () => {
			const id = addTerminal({ sessionId: "s6", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setActive(id);

			// Rapid fire: main → feature-x → main
			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
			gitOps.handleTerminalCwdChange(id, "/repo");
			await vi.advanceTimersByTimeAsync(300);

			// Should end up on main (last cwd wins)
			const main = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(main?.terminals).toContain(id);
		});

		it("does not crash when terminal was closed during debounce window", async () => {
			const id = addTerminal({ sessionId: "s7", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);

			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
			// Close terminal before debounce fires
			repositoriesStore.removeTerminalFromWorkspace("/repo", "main", id);
			terminalsStore.remove(id);

			// Should not throw
			await vi.advanceTimersByTimeAsync(300);
		});

		it("keeps an owned tab in its own repo when the cwd moves to another repo", async () => {
			repositoriesStore.add({ path: "/other", displayName: "Other" });
			repositoriesStore.setWorkspace("/other", "main", { worktreePath: "/other" });
			const id = addTerminal({ sessionId: "s9", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setRepoPath(id, "/repo");
			terminalsStore.setActive(id);
			repositoriesStore.setActive("/repo");

			gitOps.handleTerminalCwdChange(id, "/other/src");
			await vi.advanceTimersByTimeAsync(300);

			expect(repositoriesStore.get("/repo")?.workspaces["main"]?.terminals).toContain(id);
			expect(repositoriesStore.get("/other")?.workspaces["main"]?.terminals).not.toContain(id);
			expect(terminalsStore.get(id)?.repoPath).toBe("/repo");
			// The sidebar must not follow a cd either — that is what read as the app
			// switching repo on its own.
			expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
		});

		it("still follows a worktree switch inside the owning repo", async () => {
			const id = addTerminal({ sessionId: "s10", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setRepoPath(id, "/repo");
			terminalsStore.setActive(id);

			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
			await vi.advanceTimersByTimeAsync(300);

			expect(repositoriesStore.get("/repo")?.workspaces["feature-x"]?.terminals).toContain(id);
			expect(repositoriesStore.get("/repo")?.workspaces["main"]?.terminals).not.toContain(id);
		});

		it("still settles a parked tab that cds into a registered repo", async () => {
			repositoriesStore.add({ path: "/other", displayName: "Other" });
			repositoriesStore.setWorkspace("/other", "main", { worktreePath: "/other" });
			const id = addTerminal({ sessionId: "s11", cwd: "/tmp" });
			// Parked in whatever repo was active, with no owner recorded.
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setRepoPath(id, null);
			terminalsStore.setActive(id);

			gitOps.handleTerminalCwdChange(id, "/other");
			await vi.advanceTimersByTimeAsync(300);

			expect(repositoriesStore.get("/other")?.workspaces["main"]?.terminals).toContain(id);
			expect(terminalsStore.get(id)?.repoPath).toBe("/other");
		});

		it("cancelCwdTracking cancels pending debounce timer", async () => {
			const id = addTerminal({ sessionId: "s8", cwd: "/repo" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", id);
			terminalsStore.setActive(id);

			gitOps.handleTerminalCwdChange(id, "/repo/.worktrees/feature-x");
			gitOps.cancelCwdTracking(id);
			await vi.advanceTimersByTimeAsync(300);

			// Timer was cancelled — terminal should still be on main
			const main = repositoriesStore.get("/repo")?.workspaces["main"];
			expect(main?.terminals).toContain(id);
		});
	});

	describe("handleRemoveRepo — settings cleanup", () => {
		it("removes repo settings (including mcp_upstreams) when repo is removed", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", {});
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { mcpUpstreams: ["alpha", "beta"] });

			// Verify settings exist before removal
			expect(repoSettingsStore.get("/repo")).toBeDefined();
			expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toEqual(["alpha", "beta"]);

			await gitOps.handleRemoveRepo("/repo");

			// Repo settings should be cleaned up
			expect(repoSettingsStore.get("/repo")).toBeUndefined();
		});

		// Terminals were already closed here; the file-backed tabs of the same repo
		// were not, and they point at a tree the user just said they are done with.
		// They survive as tabs nothing can reach: getVisibleIds filters on a branch
		// key that no longer resolves, so they are invisible AND immortal — every
		// removal leaks another set. closeTerminal is the only function that closes
		// a tab completely (store entry, pane slot, next selection), so route them
		// through it rather than re-deriving that cleanup here.
		it("closes the repo's editor, diff and markdown tabs too", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.add({ path: "/other", displayName: "Other" });

			const edit = editorTabsStore.add("/repo", "src/main.ts");
			const diff = diffTabsStore.add("/repo", "src/main.ts", "M");
			const md = mdTabsStore.add("/repo", "README.md");
			const keep = editorTabsStore.add("/other", "src/keep.ts");

			await gitOps.handleRemoveRepo("/repo");

			expect(mockCloseTerminal).toHaveBeenCalledWith(edit);
			expect(mockCloseTerminal).toHaveBeenCalledWith(diff);
			expect(mockCloseTerminal).toHaveBeenCalledWith(md);
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(keep);
		});

		it("forgets the removed repo's remembered focus target", async () => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			recordTerminalRepo("term-1", "/repo");
			expect(getFocusForRepo("/repo")).not.toBeNull();

			await gitOps.handleRemoveRepo("/repo");

			expect(getFocusForRepo("/repo")).toBeNull();
		});
	});

	describe("orphan sweep status tally (critic-1188r5)", () => {
		const live = (path: string) => ({
			path,
			safe: false,
			live_sessions: [{ session_id: `s-${path}`, name: "agent" }],
		});
		const askOps = (confirm: () => Promise<boolean>) =>
			useGitOperations({
				repo: mockRepo,
				pty: mockPty,
				dialogs: { ...mockDialogs, confirmOrphanCleanup: vi.fn(confirm) },
				closeTerminal: mockCloseTerminal,
				createNewTerminal: mockCreateNewTerminal,
				setStatusInfo: mockSetStatusInfo,
				getDefaultFontSize: () => 14,
				getMaxTabNameLength: () => 25,
			});
		const orphanStatuses = () =>
			mockSetStatusInfo.mock.calls.map((c) => c[0]).filter((m) => typeof m === "string" && m.includes("orphaned"));

		beforeEach(() => {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			mockRepo.getRepoStructure.mockResolvedValue({ worktree_paths: wtPaths({ main: "/repo" }), merged_branches: [] });
			mockRepo.getRepoDiffStats.mockResolvedValue({ diff_stats: {}, last_commit_ts: {} });
			repoSettingsStore.getOrCreate("/repo", "Repo");
			repoSettingsStore.update("/repo", { orphanCleanup: "on" });
			mockRepo.removeOrphanWorktree.mockResolvedValue(undefined);
			mockCloseTerminal.mockResolvedValue(undefined);
		});

		// Catches: the Auto sweep and the live review each printing their own line, the second overwriting the first.
		it("auto removal plus confirmed live review report one summed line", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/safe", safe: true }, live("/wt/live")]);
			terminalsStore.add(makeTerminal({ name: "a", cwd: "/wt/safe" }));
			terminalsStore.add(makeTerminal({ name: "b", cwd: "/wt/live" }));
			mockCloseTerminal.mockRejectedValue(new Error("stuck"));
			const ops = askOps(async () => true);
			const p = ops.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			await p;
			expect(orphanStatuses()).toEqual(["Removed 2 orphaned worktree(s); 2 terminal(s) could not be closed"]);
		});

		// Catches: the early `return` on Keep skipping the final status of the already-removed safe orphan.
		it("auto removal then Keep on the live orphan still reports the safe removal once", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/safe", safe: true }, live("/wt/live")]);
			const ops = askOps(async () => false);
			const p = ops.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			await p;
			expect(orphanStatuses()).toEqual(["Removed 1 orphaned worktree(s)"]);
			expect(mockRepo.removeOrphanWorktree).toHaveBeenCalledTimes(1);
		});

		// Catches: the tally being lost when the dialog throws between the Auto phase and the review phase.
		it("auto removal then a throwing confirmation dialog still reports the safe removal", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/safe", safe: true }, live("/wt/live")]);
			const ops = askOps(async () => {
				throw new Error("dialog crashed");
			});
			const p = ops.refreshAllBranchStats().catch(() => undefined);
			await vi.advanceTimersByTimeAsync(0);
			await p;
			expect(orphanStatuses()).toEqual(["Removed 1 orphaned worktree(s)"]);
		});

		// Catches: the duplicate-dialog guard return skipping the status of the Auto phase.
		it("auto removal is reported even when another dialog already holds the review", async () => {
			repositoriesStore.add({ path: "/repo2", displayName: "Repo2" });
			repositoriesStore.setWorkspace("/repo2", "main", { worktreePath: "/repo2" });
			repoSettingsStore.getOrCreate("/repo2", "Repo2");
			repoSettingsStore.update("/repo2", { orphanCleanup: "on" });
			mockRepo.assessOrphanCleanup.mockImplementation(async (r: string) => [
				{ path: `/wt/safe-${r}`, safe: true },
				live(`/wt/live-${r}`),
			]);
			let release!: (v: boolean) => void;
			const ops = askOps(() => new Promise<boolean>((r) => (release = r)));
			const p = ops.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			release(false);
			await p;
			expect(orphanStatuses()).toHaveLength(2);
		});

		// Catches: a live orphan the backend refuses (new session appeared) counted as removed or its terminals closed.
		it("a refused live removal is neither counted nor has its terminals closed", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/safe", safe: true }, live("/wt/live")]);
			mockRepo.removeOrphanWorktree.mockImplementation(async (_r: string, path: string) => {
				if (path === "/wt/live") throw new Error("session appeared");
			});
			const liveTerm = terminalsStore.add(makeTerminal({ name: "b", cwd: "/wt/live" }));
			const ops = askOps(async () => true);
			const p = ops.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			await p;
			expect(orphanStatuses()).toEqual(["Removed 1 orphaned worktree(s)"]);
			expect(mockCloseTerminal).not.toHaveBeenCalledWith(liveTerm, true);
		});

		// Catches: a status line "Removed 0" or stale text when nothing was removed (all fail / Keep only).
		it("no status when every removal fails or the only orphan is kept", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([live("/wt/live")]);
			const keep = askOps(async () => false);
			let p = keep.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			await p;
			mockRepo.assessOrphanCleanup.mockResolvedValue([
				{ path: "/wt/a", safe: true },
				{ path: "/wt/b", safe: true },
			]);
			mockRepo.removeOrphanWorktree.mockRejectedValue(new Error("locked"));
			vi.advanceTimersByTime(5_000);
			p = keep.refreshAllBranchStats();
			await vi.advanceTimersByTimeAsync(0);
			await p;
			expect(orphanStatuses()).toEqual([]);
		});

		// Catches: the tally leaking across sweeps (module-level counter), so refresh 2 reports refresh 1's removals too.
		it("a second sweep reports only its own removals", async () => {
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/one", safe: true }]);
			await gitOps.refreshAllBranchStats();
			mockRepo.assessOrphanCleanup.mockResolvedValue([{ path: "/wt/two", safe: true }]);
			vi.advanceTimersByTime(5_000);
			await gitOps.refreshAllBranchStats();
			expect(orphanStatuses()).toEqual(["Removed 1 orphaned worktree(s)", "Removed 1 orphaned worktree(s)"]);
		});
	});
});
