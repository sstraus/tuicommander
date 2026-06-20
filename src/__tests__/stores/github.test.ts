import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type { BranchPrStatus } from "../../types";
import { testInScope, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

const mockListen = tauriListen as ReturnType<typeof vi.fn>;

const listenHandlers = new Map<string, ((event: { payload: unknown }) => void)[]>();

function emitEvent(event: string, payload: unknown): void {
	const handlers = listenHandlers.get(event);
	if (handlers) {
		for (const h of handlers) h({ payload });
	}
}

describe("githubStore", () => {
	let store: typeof import("../../stores/github").githubStore;

	const mockGetPaths = vi.fn<() => string[]>(() => ["/repo1"]);
	const mockSetIssueFilter = vi.fn();

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		listenHandlers.clear();
		mockInvoke.mockReset();
		mockGetPaths.mockReturnValue(["/repo1"]);

		mockListen.mockImplementation((event: string, handler: (event: { payload: unknown }) => void) => {
			if (!listenHandlers.has(event)) listenHandlers.set(event, []);
			listenHandlers.get(event)!.push(handler);
			return Promise.resolve(vi.fn());
		});

		vi.doMock("../../stores/repositories", () => ({
			repositoriesStore: {
				getPaths: mockGetPaths,
				getActivePaths: mockGetPaths,
			},
		}));

		mockSetIssueFilter.mockReset();
		vi.doMock("../../stores/settings", () => ({
			settingsStore: {
				state: { issueFilter: "assigned" },
				setIssueFilter: mockSetIssueFilter,
			},
		}));

		mockInvoke.mockResolvedValue(undefined);
		store = (await import("../../stores/github")).githubStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	function makePrStatus(overrides: Partial<BranchPrStatus> = {}): BranchPrStatus {
		return {
			branch: "feature/x",
			number: 42,
			title: "Add feature",
			state: "OPEN",
			url: "https://github.com/org/repo/pull/42",
			additions: 10,
			deletions: 5,
			checks: { passed: 2, failed: 0, pending: 0, total: 2 },
			check_details: [
				{ context: "build", state: "SUCCESS" },
				{ context: "test", state: "SUCCESS" },
			],
			author: "alice",
			commits: 3,
			mergeable: "MERGEABLE",
			merge_state_status: "CLEAN",
			review_decision: "",
			viewer_did_approve: false,
			labels: [],
			is_draft: false,
			base_ref_name: "main",
			head_ref_oid: "abc1234",
			created_at: "2026-01-15T10:00:00Z",
			updated_at: "2026-01-15T12:00:00Z",
			merge_state_label: { label: "Ready to merge", css_class: "clean" },
			review_state_label: null,
			merge_commit_allowed: true,
			squash_merge_allowed: true,
			rebase_merge_allowed: true,
			...overrides,
		};
	}

	describe("initialization", () => {
		it("starts with empty repos state", () => {
			testInScope(() => {
				expect(store.state.repos).toEqual({});
			});
		});
	});

	describe("updateRepoData()", () => {
		it("sets branch PR data for a repo", () => {
			testInScope(() => {
				const prData = [makePrStatus()];
				store.updateRepoData("/repo1", prData);

				expect(store.state.repos["/repo1"]).toBeDefined();
				expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
				expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
			});
		});

		it("handles multiple branches in one repo", () => {
			testInScope(() => {
				const prData = [
					makePrStatus({ branch: "feature/x", number: 42 }),
					makePrStatus({ branch: "fix/y", number: 43 }),
				];
				store.updateRepoData("/repo1", prData);

				expect(Object.keys(store.state.repos["/repo1"].branches)).toHaveLength(2);
				expect(store.state.repos["/repo1"].branches["feature/x"].number).toBe(42);
				expect(store.state.repos["/repo1"].branches["fix/y"].number).toBe(43);
			});
		});

		it("removes stale branches no longer in poll results", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [makePrStatus({ branch: "feature/x" }), makePrStatus({ branch: "feature/y" })]);
				expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
				expect(store.state.repos["/repo1"].branches["feature/y"]).toBeDefined();

				store.updateRepoData("/repo1", [makePrStatus({ branch: "feature/x" })]);
				expect(store.state.repos["/repo1"].branches["feature/x"]).toBeDefined();
				expect(store.state.repos["/repo1"].branches["feature/y"]).toBeUndefined();
			});
		});

		it("updates lastPolled timestamp", () => {
			testInScope(() => {
				const before = Date.now();
				store.updateRepoData("/repo1", [makePrStatus()]);
				const after = Date.now();

				expect(store.state.repos["/repo1"].lastPolled).toBeGreaterThanOrEqual(before);
				expect(store.state.repos["/repo1"].lastPolled).toBeLessThanOrEqual(after);
			});
		});
	});

	describe("getCheckSummary()", () => {
		it("returns check summary for known branch", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [makePrStatus({ checks: { passed: 3, failed: 1, pending: 2, total: 6 } })]);

				const summary = store.getCheckSummary("/repo1", "feature/x");
				expect(summary).toEqual({ passed: 3, failed: 1, pending: 2, total: 6 });
			});
		});

		it("returns null for unknown repo", () => {
			testInScope(() => {
				expect(store.getCheckSummary("/unknown", "feature/x")).toBeNull();
			});
		});

		it("returns null for unknown branch", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [makePrStatus()]);
				expect(store.getCheckSummary("/repo1", "nonexistent")).toBeNull();
			});
		});
	});

	describe("getPrStatus()", () => {
		it("returns PR status for known branch", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [makePrStatus()]);

				const pr = store.getPrStatus("/repo1", "feature/x");
				expect(pr).not.toBeNull();
				expect(pr!.number).toBe(42);
				expect(pr!.title).toBe("Add feature");
				expect(pr!.state).toBe("OPEN");
				expect(pr!.url).toBe("https://github.com/org/repo/pull/42");
			});
		});

		it("returns null for unknown branches", () => {
			testInScope(() => {
				expect(store.getPrStatus("/unknown", "feature/x")).toBeNull();
			});
		});
	});

	describe("getCheckDetails()", () => {
		it("returns check details for known branch", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [makePrStatus()]);

				const details = store.getCheckDetails("/repo1", "feature/x");
				expect(details).toHaveLength(2);
				expect(details[0].context).toBe("build");
				expect(details[1].context).toBe("test");
			});
		});

		it("returns empty array for unknown branch", () => {
			testInScope(() => {
				expect(store.getCheckDetails("/unknown", "feature/x")).toEqual([]);
			});
		});
	});

	describe("getBranchPrData()", () => {
		it("returns full BranchPrStatus for known branch", () => {
			testInScope(() => {
				const pr = makePrStatus();
				store.updateRepoData("/repo1", [pr]);

				const data = store.getBranchPrData("/repo1", "feature/x");
				expect(data).not.toBeNull();
				expect(data!.author).toBe("alice");
				expect(data!.commits).toBe(3);
				expect(data!.additions).toBe(10);
				expect(data!.deletions).toBe(5);
			});
		});

		it("returns null for unknown branch", () => {
			testInScope(() => {
				expect(store.getBranchPrData("/repo1", "no-branch")).toBeNull();
			});
		});
	});

	describe("event-driven polling", () => {
		it("startPolling invokes github_start_polling with paths and issue filter", async () => {
			await testInScopeAsync(async () => {
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				expect(mockInvoke).toHaveBeenCalledWith("github_start_polling", {
					paths: ["/repo1"],
					issueFilter: "assigned",
				});
				store.stopPolling();
			});
		});

		it("stopPolling invokes github_stop_polling", async () => {
			await testInScopeAsync(async () => {
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);
				store.stopPolling();

				expect(mockInvoke).toHaveBeenCalledWith("github_stop_polling");
			});
		});

		it("pollRepo invokes github_poll_repo with path", async () => {
			await testInScopeAsync(async () => {
				store.pollRepo("/repo1");
				await vi.advanceTimersByTimeAsync(0);

				expect(mockInvoke).toHaveBeenCalledWith("github_poll_repo", { path: "/repo1" });
			});
		});

		it("updates store when github-pr-update event fires", async () => {
			await testInScopeAsync(async () => {
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				const prStatus = makePrStatus({ branch: "main", number: 7 });
				emitEvent("github-pr-update", { repo_path: "/repo1", statuses: [prStatus] });

				const data = store.getBranchPrData("/repo1", "main");
				expect(data).not.toBeNull();
				expect(data!.number).toBe(7);

				store.stopPolling();
			});
		});

		it("updates issues when github-issues-update event fires", async () => {
			await testInScopeAsync(async () => {
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				emitEvent("github-issues-update", {
					repo_path: "/repo1",
					issues: [{ number: 1, title: "Bug", state: "OPEN" }],
				});

				const issues = store.getRepoIssues("/repo1");
				expect(issues).toHaveLength(1);
				expect(issues[0].title).toBe("Bug");

				store.stopPolling();
			});
		});

		it("fires prTerminal callback on merged transition event", async () => {
			await testInScopeAsync(async () => {
				const cb = vi.fn();
				store.setOnPrTerminal(cb);
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				emitEvent("github-transition", {
					type: "merged",
					repo_path: "/repo1",
					branch: "feature/x",
					pr_number: 42,
					title: "Add feature",
				});

				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42, "merged");
				store.setOnPrTerminal(null);
				store.stopPolling();
			});
		});

		it("fires ciFailed callback on ci_failed transition event", async () => {
			await testInScopeAsync(async () => {
				const cb = vi.fn();
				store.setOnCiFailed(cb);
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				emitEvent("github-transition", {
					type: "ci_failed",
					repo_path: "/repo1",
					branch: "feature/x",
					pr_number: 42,
					title: "Add feature",
				});

				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42);
				store.setOnCiFailed(null);
				store.stopPolling();
			});
		});

		it("fires ciRecovered callback on ci_recovered transition event", async () => {
			await testInScopeAsync(async () => {
				const cb = vi.fn();
				store.setOnCiRecovered(cb);
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				emitEvent("github-transition", {
					type: "ci_recovered",
					repo_path: "/repo1",
					branch: "feature/x",
					pr_number: 42,
					title: "Add feature",
				});

				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42);
				store.setOnCiRecovered(null);
				store.stopPolling();
			});
		});

		it("fires conflict callback on blocked transition event", async () => {
			await testInScopeAsync(async () => {
				const cb = vi.fn();
				store.setOnConflict(cb);
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				emitEvent("github-transition", {
					type: "blocked",
					repo_path: "/repo1",
					branch: "feature/x",
					pr_number: 42,
					title: "Add feature",
				});

				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42);
				store.setOnConflict(null);
				store.stopPolling();
			});
		});

		it("triggerConflictHeal fires the registered conflict callback on demand", () => {
			testInScope(() => {
				const cb = vi.fn();
				store.setOnConflict(cb);
				store.triggerConflictHeal("/repo1", "feature/x", 42);
				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42);
				store.setOnConflict(null);
			});
		});

		it("triggerConflictHeal is a no-op when no conflict callback is registered", () => {
			testInScope(() => {
				store.setOnConflict(null);
				expect(() => store.triggerConflictHeal("/repo1", "feature/x", 42)).not.toThrow();
			});
		});

		it("triggerCiHeal fires the registered ciFailed callback on demand", () => {
			testInScope(() => {
				const cb = vi.fn();
				store.setOnCiFailed(cb);
				store.triggerCiHeal("/repo1", "feature/x", 42);
				expect(cb).toHaveBeenCalledWith("/repo1", "feature/x", 42);
				store.setOnCiFailed(null);
			});
		});

		it("triggerCiHeal is a no-op when no ciFailed callback is registered", () => {
			testInScope(() => {
				store.setOnCiFailed(null);
				expect(() => store.triggerCiHeal("/repo1", "feature/x", 42)).not.toThrow();
			});
		});

		it("forwards visibility changes to Rust poller", async () => {
			await testInScopeAsync(async () => {
				store.startPolling();
				await vi.advanceTimersByTimeAsync(0);

				Object.defineProperty(document, "hidden", { value: true, writable: true, configurable: true });
				document.dispatchEvent(new Event("visibilitychange"));
				await vi.advanceTimersByTimeAsync(0);

				expect(mockInvoke).toHaveBeenCalledWith("github_set_visibility", { visible: false });

				Object.defineProperty(document, "hidden", { value: false, writable: true, configurable: true });
				document.dispatchEvent(new Event("visibilitychange"));
				await vi.advanceTimersByTimeAsync(0);

				expect(mockInvoke).toHaveBeenCalledWith("github_set_visibility", { visible: true });

				store.stopPolling();
			});
		});
	});

	describe("getRemoteOnlyPrs()", () => {
		it("returns open PRs whose branch is not in the provided local branches set", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [
					makePrStatus({ branch: "local-branch", state: "OPEN" }),
					makePrStatus({ branch: "remote-only-a", state: "OPEN", number: 1 }),
					makePrStatus({ branch: "remote-only-b", state: "OPEN", number: 2 }),
				]);

				const result = store.getRemoteOnlyPrs("/repo1", new Set(["local-branch"]));

				expect(result).toHaveLength(2);
				expect(result.map((p) => p.branch)).toEqual(expect.arrayContaining(["remote-only-a", "remote-only-b"]));
			});
		});

		it("excludes merged and closed PRs", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [
					makePrStatus({ branch: "merged-remote", state: "MERGED", number: 1 }),
					makePrStatus({ branch: "closed-remote", state: "CLOSED", number: 2 }),
					makePrStatus({ branch: "open-remote", state: "OPEN", number: 3 }),
				]);

				const result = store.getRemoteOnlyPrs("/repo1", new Set([]));

				expect(result).toHaveLength(1);
				expect(result[0].branch).toBe("open-remote");
			});
		});

		it("returns empty array when all PRs have local branches", () => {
			testInScope(() => {
				store.updateRepoData("/repo1", [
					makePrStatus({ branch: "branch-a", state: "OPEN" }),
					makePrStatus({ branch: "branch-b", state: "OPEN", number: 2 }),
				]);

				const result = store.getRemoteOnlyPrs("/repo1", new Set(["branch-a", "branch-b"]));

				expect(result).toHaveLength(0);
			});
		});

		it("returns empty array for unknown repo", () => {
			testInScope(() => {
				const result = store.getRemoteOnlyPrs("/unknown-repo", new Set([]));
				expect(result).toHaveLength(0);
			});
		});
	});

	describe("setIssueFilter()", () => {
		it("delegates to settingsStore.setIssueFilter and invokes Rust command", async () => {
			await testInScopeAsync(async () => {
				store.setIssueFilter("created");
				await vi.advanceTimersByTimeAsync(0);

				expect(mockSetIssueFilter).toHaveBeenCalledWith("created");
				expect(mockInvoke).toHaveBeenCalledWith("github_set_issue_filter", { filter: "created" });
			});
		});
	});
});
