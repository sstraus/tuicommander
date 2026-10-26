import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRepositoryRefreshCoordinator } from "../../hooks/git/createRepositoryRefreshCoordinator";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";

describe("repository refresh interval (#1491)", () => {
	let additions = 0;
	const structure = vi.fn(async (path: string) => ({
		worktree_paths: { main: { branch: "main", path, kind: "worktree" as const } },
		merged_branches: [],
	}));
	const stats = vi.fn(async (path: string) => ({
		diff_stats: { [path]: { additions, deletions: 0 } },
		last_commit_ts: {},
		workspace_statuses: {},
	}));
	let refresh: ReturnType<typeof createRepositoryRefreshCoordinator>["refreshAllBranchStats"];

	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-10-07T12:00:00Z"));
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		additions = 0;
		structure.mockClear();
		stats.mockClear();
		refresh = createRepositoryRefreshCoordinator({
			repo: {
				getInfo: vi.fn(async () => ({ branch: "main", is_git_repo: true })),
				getRepoStructure: structure,
				getRepoDiffStats: stats,
				detectOrphanWorktrees: vi.fn(async () => []),
				assessOrphanCleanup: vi.fn(async () => []),
				beginOrphanCleanup: vi.fn(async () => {}),
				pendingOrphanCleanupAnswer: vi.fn(async () => null),
				clearOrphanCleanup: vi.fn(async () => {}),
				removeOrphanWorktree: vi.fn(async () => {}),
				getWorkspaceLifecycle: vi.fn(),
				finalizeMergedWorktree: vi.fn(),
			},
			dialogs: {},
			closeTerminal: vi.fn(async () => {}),
			closeTerminalsInWorktree: vi.fn(async () => {}),
			setStatusInfo: vi.fn(),
		}).refreshAllBranchStats;
		for (const path of ["/repo", "/other"]) {
			repositoriesStore.add({ path, displayName: path });
			repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
		}
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	it("coalesces a burst into leading and trailing reads instead of spawning git per request", async () => {
		await refresh("/repo");
		const pending: Promise<void>[] = [];
		for (let i = 0; i < 10; i++) {
			await vi.advanceTimersByTimeAsync(400);
			pending.push(refresh("/repo"));
		}
		expect(structure).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(999);
		expect(structure).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1);
		await Promise.all(pending);
		expect(structure).toHaveBeenCalledTimes(2);
		expect(stats).toHaveBeenCalledTimes(2);
	});

	it("reads the last state at the trailing deadline instead of dropping the final update", async () => {
		await refresh("/repo");
		await vi.advanceTimersByTimeAsync(1_000);
		const pending = refresh("/repo");
		await vi.advanceTimersByTimeAsync(3_999);
		additions = 42;
		expect(repositoriesStore.get("/repo")?.workspaces.main.additions).toBe(0);
		await vi.advanceTimersByTimeAsync(1);
		await pending;
		expect(repositoriesStore.get("/repo")?.workspaces.main.additions).toBe(42);
		expect(structure).toHaveBeenCalledTimes(2);
	});

	it("starts another repo immediately instead of applying a global throttle", async () => {
		await refresh("/repo");
		const pending = refresh("/repo");
		await refresh("/other");
		expect(structure.mock.calls.map(([path]) => path)).toEqual(["/repo", "/other"]);
		await vi.advanceTimersByTimeAsync(5_000);
		await pending;
		expect(structure.mock.calls.map(([path]) => path)).toEqual(["/repo", "/other", "/repo"]);
	});

	it("keeps single-flight through a slow read instead of overlapping at the deadline", async () => {
		let release: (() => void) | undefined;
		structure.mockImplementationOnce(async (path) => {
			await new Promise<void>((resolve) => {
				release = resolve;
			});
			return { worktree_paths: { main: { branch: "main", path, kind: "worktree" as const } }, merged_branches: [] };
		});
		const first = refresh("/repo");
		const queued = refresh("/repo");
		await vi.advanceTimersByTimeAsync(6_000);
		expect(structure).toHaveBeenCalledTimes(1);
		release?.();
		await Promise.all([first, queued]);
		expect(structure).toHaveBeenCalledTimes(2);
	});

	it("refreshes at once when the clock went back instead of waiting out the skew", async () => {
		// catches: remaining = 5s - (negative elapsed), stalling the repo for the whole skew
		await refresh("/repo");
		vi.setSystemTime(new Date("2026-10-06T12:00:00Z"));
		await refresh("/repo");
		expect(structure).toHaveBeenCalledTimes(2);
	});

	it("skips a repo parked during the wait instead of waking a dormant repo", async () => {
		await refresh("/repo");
		const pending = refresh("/repo");
		repositoriesStore.setPark("/repo", true);
		await vi.advanceTimersByTimeAsync(5_000);
		await pending;
		expect(structure).toHaveBeenCalledTimes(1);
	});
	it("counts a failed pass as a start instead of retrying git inside the window", async () => {
		const failure = new Error("structure read failed");
		const warning = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		try {
			structure.mockRejectedValueOnce(failure);
			await refresh("/repo");
			expect(warning).toHaveBeenCalledWith("git", "Repository refresh failed for /repo", failure);
			const pending = refresh("/repo");
			await vi.advanceTimersByTimeAsync(4_999);
			expect(structure).toHaveBeenCalledTimes(1);
			await vi.advanceTimersByTimeAsync(1);
			await pending;
			expect(structure).toHaveBeenCalledTimes(2);
			expect(stats).toHaveBeenCalledTimes(1);
		} finally {
			warning.mockRestore();
		}
	});

	it("wakes an existing trailing wait instead of blocking an explicit UI refresh", async () => {
		await refresh("/repo");
		const queued = refresh("/repo");
		additions = 42;
		await refresh("/repo", { immediate: true });
		await queued;
		expect(structure).toHaveBeenCalledTimes(2);
		expect(repositoriesStore.get("/repo")?.workspaces.main.additions).toBe(42);
		// The bypass also starts a new automatic window; the cancelled timer must not run again.
		const next = refresh("/repo");
		await vi.advanceTimersByTimeAsync(4_999);
		expect(structure).toHaveBeenCalledTimes(2);
		await vi.advanceTimersByTimeAsync(1);
		await next;
		expect(structure).toHaveBeenCalledTimes(3);
	});

	it("queues an explicit UI refresh behind an active read instead of overlapping git", async () => {
		let release: (() => void) | undefined;
		structure.mockImplementationOnce(async (path) => {
			await new Promise<void>((resolve) => {
				release = resolve;
			});
			return { worktree_paths: { main: { branch: "main", path, kind: "worktree" as const } }, merged_branches: [] };
		});
		const active = refresh("/repo");
		const explicit = refresh("/repo", { immediate: true });
		expect(structure).toHaveBeenCalledTimes(1);
		release?.();
		await Promise.all([active, explicit]);
		expect(structure).toHaveBeenCalledTimes(2);
	});
});
