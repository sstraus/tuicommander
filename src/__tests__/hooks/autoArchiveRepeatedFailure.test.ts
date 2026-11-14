import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { keptReason } from "../../hooks/git/workspaceCleanupDecision";
import { useGitOperations } from "../../hooks/useGitOperations";
import { appLogger } from "../../stores/appLogger";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { mockInvoke } from "../mocks/tauri";

const REINSPECT_MS = 10 * 60_000;

const lifecycle = () => ({
	dirtyFingerprint: "fp",
	dirtyFiles: 0,
	commitStatus: "merged",
	proofSource: "ancestry",
	tip: "tip-1",
	base: "base-1",
	removalSafety: "safe",
});

const wtPaths = (byBranch: Record<string, string>) =>
	Object.fromEntries(Object.entries(byBranch).map(([branch, path]) => [branch, { branch, path, kind: "worktree" }]));

describe("auto-archive of merged worktrees under repeated failure", () => {
	const getWorkspaceLifecycle = vi.fn();
	const finalizeMergedWorktree = vi.fn();
	const getRepoStructure = vi.fn();
	const repo = new Proxy(
		{
			getRepoStructure,
			getRepoDiffStats: vi.fn().mockResolvedValue({ diff_stats: {}, last_commit_ts: {} }),
			getInfo: vi.fn().mockResolvedValue({ branch: "", is_git_repo: false }),
			getWorkspaceLifecycle,
			finalizeMergedWorktree,
			sweepArchiveRetention: vi.fn().mockResolvedValue({ deleted: [], kept: [] }),
			detectOrphanWorktrees: vi.fn().mockResolvedValue([]),
			assessOrphanCleanup: vi.fn().mockResolvedValue([]),
			getMergedBranches: vi.fn().mockResolvedValue([]),
			listLocalBranches: vi.fn().mockResolvedValue(["main"]),
		} as Record<string, unknown>,
		{ get: (target, key: string) => target[key] ?? vi.fn().mockResolvedValue(undefined) },
	);
	let gitOps: ReturnType<typeof useGitOperations>;
	const setStatusInfo = vi.fn();

	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		paneLayoutStore.reset();
		resetGroupCounter();
		mockInvoke.mockResolvedValue(undefined);
		getWorkspaceLifecycle.mockReset().mockResolvedValue(lifecycle());
		finalizeMergedWorktree.mockReset().mockResolvedValue({ merged: true, action: "archived", archive_path: null });
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		getRepoStructure.mockResolvedValue({
			worktree_paths: wtPaths({ main: "/repo", "feature/x": "/repo/.worktrees/x", "feature/y": "/repo/.worktrees/y" }),
			merged_branches: ["feature/x", "feature/y"],
		});
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { autoArchiveMerged: true });
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		onTestFinished(() => warn.mockRestore());
		gitOps = useGitOperations({
			repo: repo as never,
			pty: {
				canSpawn: vi.fn().mockResolvedValue(true),
				write: vi.fn(),
				getWorktreesDir: vi.fn().mockResolvedValue("/w"),
			} as never,
			dialogs: {} as never,
			closeTerminal: vi.fn(),
			createNewTerminal: vi.fn(),
			setStatusInfo,
			getDefaultFontSize: () => 14,
			getMaxTabNameLength: () => 25,
		});
	});

	afterEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		vi.useRealTimers();
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	const refresh = () => gitOps.refreshAllBranchStats(undefined, { immediate: true });
	const callsFor = (id: string) => finalizeMergedWorktree.mock.calls.filter((call) => call[1] === id).length;

	// Catches: a failure on one workspace throttling (or masking) a healthy sibling, or the
	// failing one skipping the interval and being hit again on the very next refresh.
	it("throttles only the workspace that failed; its sibling keeps being archived", async () => {
		finalizeMergedWorktree.mockImplementation(async (_repo: string, id: string) => {
			if (id === "feature/x") throw new Error("Failed to repair archived worktree");
			return { merged: true, action: "archived", archive_path: null };
		});

		await refresh();
		await refresh();

		expect(callsFor("feature/x")).toBe(2);
		expect(callsFor("feature/y")).toBe(2);
		expect(keptReason(repositoriesStore.get("/repo")?.workspaces["feature/y"]?.cleanup)).toBeNull();
	});

	// Catches: the failure record outliving the interval, so a repaired checkout is never archived.
	it("archives after the interval once the cause is gone", async () => {
		finalizeMergedWorktree.mockRejectedValue(new Error("Failed to repair archived worktree"));
		await refresh();
		finalizeMergedWorktree.mockResolvedValue({ merged: true, action: "archived", archive_path: null });
		vi.setSystemTime(Date.now() + REINSPECT_MS - 1_000);
		await refresh();
		expect(callsFor("feature/x")).toBe(2);

		vi.setSystemTime(Date.now() + 2_000);
		await refresh();

		expect(callsFor("feature/x")).toBe(3);
		expect(setStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Auto-archived 2 merged worktree(s)"));
	});

	// Catches: the same every-refresh loop through the OTHER failure shape: the fresh inspection
	// before the retry throws (so no record is written) after an archive that failed once.
	it("waits the interval when the inspection before the retry keeps failing", async () => {
		getWorkspaceLifecycle.mockReset();
		getWorkspaceLifecycle.mockResolvedValueOnce(lifecycle()).mockRejectedValue(new Error("git status failed"));
		finalizeMergedWorktree.mockRejectedValue(new Error("Failed to repair archived worktree"));
		getRepoStructure.mockResolvedValue({
			worktree_paths: wtPaths({ main: "/repo", "feature/x": "/repo/.worktrees/x" }),
			merged_branches: ["feature/x"],
		});

		await refresh();
		const afterFirst = getWorkspaceLifecycle.mock.calls.length;
		await refresh();
		await refresh();

		expect(getWorkspaceLifecycle.mock.calls.length).toBe(afterFirst);
	});

	// Catches: an inspection that throws on every refresh being repeated on every refresh, forever
	// (the preflight walks the whole checkout), because only archive failures are recorded.
	it("waits the interval when the inspection itself keeps failing", async () => {
		getWorkspaceLifecycle.mockRejectedValue(new Error("git status failed"));
		getRepoStructure.mockResolvedValue({
			worktree_paths: wtPaths({ main: "/repo", "feature/x": "/repo/.worktrees/x" }),
			merged_branches: ["feature/x"],
		});

		await refresh();
		await refresh();
		await refresh();

		expect(getWorkspaceLifecycle).toHaveBeenCalledTimes(1);
	});
});
