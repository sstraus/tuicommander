import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createRepositoryRefreshCoordinator } from "../../hooks/git/createRepositoryRefreshCoordinator";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";

beforeAll(async () => {
	await activityStore.hydrate();
});

interface Row {
	path: string;
	safe: boolean;
	reason?: string;
	dirty_fingerprint?: string;
	live_sessions?: Array<{ session_id: string; name: string }>;
}

describe("orphan Keep memory by path + fingerprint (critic-1367, story 1367-7d6e)", () => {
	const setStatusInfo = vi.fn();
	const removeOrphanWorktree = vi.fn();
	const confirmOrphanCleanup = vi.fn();
	const beginOrphanCleanup = vi.fn();
	const clearOrphanCleanup = vi.fn();
	const pendingOrphanCleanupAnswer = vi.fn();
	let rows: Row[];
	let refresh: (repoPath?: string) => Promise<void>;

	const dirty = (path: string, fingerprint?: string): Row => ({
		path,
		safe: false,
		reason: "untracked files",
		dirty_fingerprint: fingerprint,
	});

	beforeEach(() => {
		vi.useFakeTimers();
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		for (const s of repoSettingsStore.getAll()) repoSettingsStore.remove(s.path);
		setStatusInfo.mockReset();
		removeOrphanWorktree.mockReset().mockResolvedValue(undefined);
		confirmOrphanCleanup.mockReset().mockResolvedValue(false);
		beginOrphanCleanup.mockReset().mockResolvedValue(undefined);
		clearOrphanCleanup.mockReset().mockResolvedValue(undefined);
		pendingOrphanCleanupAnswer.mockReset().mockResolvedValue(null);
		rows = [];
		const repo = {
			getInfo: vi.fn(async () => ({ branch: "main", is_git_repo: true })),
			getRepoStructure: vi.fn(async () => ({
				worktree_paths: { main: { branch: "main", path: "/repo", kind: "main" } },
				merged_branches: [],
			})),
			getRepoDiffStats: vi.fn().mockResolvedValue({ diff_stats: {}, last_commit_ts: {}, workspace_statuses: {} }),
			detectOrphanWorktrees: vi.fn().mockResolvedValue([]),
			assessOrphanCleanup: vi.fn(async () => rows),
			beginOrphanCleanup,
			pendingOrphanCleanupAnswer,
			clearOrphanCleanup,
			removeOrphanWorktree,
			getWorkspaceLifecycle: vi.fn(),
			finalizeMergedWorktree: vi.fn(),
		};
		const c = createRepositoryRefreshCoordinator({
			repo,
			dialogs: { confirmOrphanCleanup },
			closeTerminal: vi.fn(),
			closeTerminalsInWorktree: vi.fn().mockResolvedValue(undefined),
			setStatusInfo,
		} as unknown as Parameters<typeof createRepositoryRefreshCoordinator>[0]);
		refresh = c.refreshAllBranchStats;
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { orphanCleanup: "ask" });
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	// Catches: a failed confirmed removal silently reopening the same dialog.
	it("shows the git failure and does not reprompt for the same checkout", async () => {
		const warning = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		rows = [dirty("/wt/a", "fp-1")];
		confirmOrphanCleanup.mockResolvedValue(true);
		removeOrphanWorktree.mockRejectedValue(new Error("git says untracked files"));
		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		expect(setStatusInfo).toHaveBeenCalledWith(expect.stringContaining("git says untracked files"));
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
		expect(removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/a", false, [], "fp-1");
		expect(warning).toHaveBeenCalledWith("git", expect.stringContaining("/wt/a"), expect.any(Error));
		warning.mockRestore();
	});

	// Catches: the Keep being forgotten between refreshes (or keyed on something that
	// changes per assessment), so the dialog fires on every repo-changed event.
	it("a Keep holds over many refreshes while the fingerprint is unchanged", async () => {
		rows = [dirty("/wt/a", "fp-1")];

		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
		expect(removeOrphanWorktree).not.toHaveBeenCalled();
	});

	// Catches: the Keep staying in force after the orphan was edited again, so new
	// unsaved work is never put in front of the user.
	it("a changed fingerprint puts the kept orphan in front of the user again, with its new fingerprint", async () => {
		rows = [dirty("/wt/a", "fp-1")];
		await refresh("/repo");

		rows = [dirty("/wt/a", "fp-2")];
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
		expect(confirmOrphanCleanup.mock.calls[1][1]).toEqual([dirty("/wt/a", "fp-2")]);
	});

	// Catches: the Keep filter running before the safe-removal step, so an orphan that
	// became safe without its fingerprint moving (a session ended, a branch now holds
	// its HEAD) is never removed and stays listed for ever.
	it("a kept orphan that turns safe with the SAME fingerprint is still removed without asking", async () => {
		rows = [{ path: "/wt/a", safe: false, reason: "live session: A", dirty_fingerprint: "fp-1" }];
		await refresh("/repo");
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);

		rows = [{ path: "/wt/a", safe: true, dirty_fingerprint: "fp-1" }];
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/a", true);
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
	});

	// Catches: ask mode still routing a safe orphan through the dialog, or listing it
	// next to the unsafe one so a Keep of the dialog keeps the clean one alive too.
	it("ask mode removes the safe orphan silently and shows only the unsafe one", async () => {
		rows = [{ path: "/wt/clean", safe: true, dirty_fingerprint: "c" }, dirty("/wt/dirty", "d")];

		await refresh("/repo");

		expect(removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/clean", true);
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
		expect(confirmOrphanCleanup.mock.calls[0][1].map((entry: Row) => entry.path)).toEqual(["/wt/dirty"]);
		expect(beginOrphanCleanup).toHaveBeenCalledWith("/repo", ["/wt/dirty"]);
	});

	// Catches: a Keep recorded for the whole repo (or the whole batch) instead of per
	// orphan path, so a new orphan next to a kept one is never asked about.
	it("a new orphan next to a kept one is asked about alone, and keeping it does not re-open the first", async () => {
		rows = [dirty("/wt/a", "fp-a")];
		await refresh("/repo");

		rows = [dirty("/wt/a", "fp-a"), dirty("/wt/b", "fp-b")];
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
		expect(confirmOrphanCleanup.mock.calls[1][1].map((entry: Row) => entry.path)).toEqual(["/wt/b"]);

		vi.setSystemTime(Date.now() + 5_000);

		await refresh("/repo");
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
	});

	// Catches: `undefined !== undefined`-style comparison treating "no fingerprint" as
	// always changed, so an orphan git cannot inspect repeats its dialog on every refresh.
	it("a Keep on an orphan with no fingerprint holds while it still has none, and lapses once one appears", async () => {
		rows = [dirty("/wt/unreadable", undefined)];
		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);

		rows = [dirty("/wt/unreadable", "now-readable")];
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
	});

	// Catches: only the user's own Keep being remembered; an agent's Keep (read back from
	// the shared backend entry at the end of the countdown) leaves the dialog to come back.
	it("a Keep that arrives from the backend at the end of the countdown is remembered too", async () => {
		rows = [dirty("/wt/a", "fp-1")];
		confirmOrphanCleanup.mockResolvedValue(true);
		pendingOrphanCleanupAnswer.mockResolvedValue(false);

		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(removeOrphanWorktree).not.toHaveBeenCalled();
		expect(clearOrphanCleanup).toHaveBeenCalledWith("/repo", true);
		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(1);
	});

	// Catches: a confirmed removal ending the dialog with kept=true, which would leave a
	// settled entry on the backend instead of dropping it.
	it("a confirmed removal clears the backend entry as not kept, and a Keep clears it as kept", async () => {
		rows = [dirty("/wt/a", "fp-1")];
		confirmOrphanCleanup.mockResolvedValue(true);
		await refresh("/repo");
		expect(removeOrphanWorktree).toHaveBeenCalledWith("/repo", "/wt/a", false, [], "fp-1");
		expect(clearOrphanCleanup).toHaveBeenLastCalledWith("/repo", false);

		rows = [dirty("/wt/b", "fp-b")];
		confirmOrphanCleanup.mockResolvedValue(false);
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");
		expect(clearOrphanCleanup).toHaveBeenLastCalledWith("/repo", true);
	});

	// Catches: a refused silent removal (a session started after the assessment) falling
	// into the dialog or being remembered as a Keep, so it is never retried.
	it("a refused safe removal opens no dialog and is retried on the next refresh", async () => {
		rows = [{ path: "/wt/late", safe: true, dirty_fingerprint: "c" }];
		removeOrphanWorktree.mockRejectedValueOnce(new Error("live session: late"));

		await refresh("/repo");
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(confirmOrphanCleanup).not.toHaveBeenCalled();
		expect(removeOrphanWorktree).toHaveBeenCalledTimes(2);
		expect(setStatusInfo).toHaveBeenCalledTimes(1);
		expect(setStatusInfo).toHaveBeenCalledWith("Removed 1 orphaned worktree(s)");
	});

	// Catches: on-mode review widened by the fingerprint change, so a kept live orphan
	// whose fingerprint moved is reviewed but an unsafe one with no session still is not.
	it("on mode: a changed fingerprint re-opens only the live-session orphan, never an unattended dirty one", async () => {
		repoSettingsStore.update("/repo", { orphanCleanup: "on" });
		const live = (fp: string): Row => ({
			path: "/wt/live",
			safe: false,
			reason: "live session: A",
			dirty_fingerprint: fp,
			live_sessions: [{ session_id: "s1", name: "A" }],
		});
		rows = [live("fp-1"), dirty("/wt/dirty", "d-1")];
		await refresh("/repo");
		rows = [live("fp-2"), dirty("/wt/dirty", "d-2")];
		vi.setSystemTime(Date.now() + 5_000);
		await refresh("/repo");

		expect(confirmOrphanCleanup).toHaveBeenCalledTimes(2);
		for (const call of confirmOrphanCleanup.mock.calls) {
			expect(call[1].map((entry: Row) => entry.path)).toEqual(["/wt/live"]);
		}
		expect(removeOrphanWorktree).not.toHaveBeenCalled();
	});
});
