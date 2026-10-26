import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChangesTab } from "../../components/GitPanel/ChangesTab";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { mockInvoke } from "../mocks/tauri";

// A plain directory registered as a repo has no git index. ChangesTab used to
// call `get_working_tree_status` on it anyway, so every repo-revision bump
// logged "fatal: not a git repository" — a permanent error flood in the app log.
describe("ChangesTab git gating", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		mockInvoke.mockResolvedValue({ staged: [], unstaged: [], untracked: [] });
	});

	// The store's debounced save and the tab's rAF outlive the test body; drain
	// them so the leak detector does not blame an unrelated later test.
	afterEach(() => {
		vi.runOnlyPendingTimers();
		for (const call of vi.mocked(appLogger.warn).mock.calls) {
			expect(call).toEqual(["store", "Repositories save blocked — hydrate not yet complete"]);
		}
		vi.restoreAllMocks();
		vi.useRealTimers();
	});

	function statusCalls() {
		return mockInvoke.mock.calls.filter((c) => c[0] === "get_working_tree_status");
	}

	it("does not query the working tree for a plain (non-git) directory", () => {
		repositoriesStore.add({ path: "/plain-dir", displayName: "Plain", isGitRepo: false });
		render(() => <ChangesTab repoPath="/plain-dir" onOpenDiff={vi.fn()} />);
		expect(statusCalls()).toHaveLength(0);
	});

	it("queries the working tree for a git repo", () => {
		repositoriesStore.add({ path: "/git-repo", displayName: "Repo" });
		render(() => <ChangesTab repoPath="/git-repo" onOpenDiff={vi.fn()} />);
		expect(statusCalls()).toHaveLength(1);
	});

	// Worktrees pass their own path as repoPath but key store lookups on the
	// parent repo. The gate must follow storeRepoPath, not repoPath.
	it("queries the working tree for a worktree of a git repo", () => {
		repositoriesStore.add({ path: "/git-repo", displayName: "Repo" });
		render(() => <ChangesTab repoPath="/git-repo/../wt" storeRepoPath="/git-repo" onOpenDiff={vi.fn()} />);
		expect(statusCalls()).toHaveLength(1);
	});
});
