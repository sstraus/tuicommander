import { createStore, produce } from "solid-js/store";
import { invoke, listen } from "../invoke";
import type { BranchPrStatus, CheckDetail, CheckSummary, GitHubIssue, GitHubStatus } from "../types";
import { appLogger } from "./appLogger";
import { isNotificationType, prNotificationsStore } from "./prNotifications";
import { repositoriesStore } from "./repositories";
import { settingsStore } from "./settings";

/** Per-repo remote tracking data (ahead/behind from local git) */
interface RepoRemoteStatus {
	has_remote: boolean;
	current_branch: string;
	ahead: number;
	behind: number;
}

/** Per-repo PR/CI data */
interface RepoGitHubData {
	branches: Record<string, BranchPrStatus>;
	remoteStatus: RepoRemoteStatus | null;
	lastPolled: number;
	issues: GitHubIssue[];
	issuesLastPolled: number;
}

/** GitHub store state */
interface GitHubStoreState {
	repos: Record<string, RepoGitHubData>;
	issuesLoading: boolean;
	circuitBreakerOpen: boolean;
	viewerLogin: string | null;
}

function createGitHubStore() {
	const [state, setState] = createStore<GitHubStoreState>({
		repos: {},
		issuesLoading: false,
		circuitBreakerOpen: false,
		viewerLogin: null,
	});

	const unlisteners: (() => void)[] = [];

	/** Callback fired when a PR reaches a terminal state (merged/closed) */
	let prTerminalCallback:
		| ((repoPath: string, branch: string, prNumber: number, type: "merged" | "closed") => void)
		| null = null;
	/** Callback fired when CI checks transition to failed for a PR */
	let ciFailedCallback: ((repoPath: string, branch: string, prNumber: number) => void) | null = null;
	/** Callback fired when CI checks recover (failed → all passing) for a PR */
	let ciRecoveredCallback: ((repoPath: string, branch: string, prNumber: number) => void) | null = null;

	/** Update repo data from Rust poller event (transitions handled by separate event) */
	function updateRepoData(repoPath: string, prStatuses: BranchPrStatus[]): void {
		const branches: Record<string, BranchPrStatus> = {};
		for (const pr of prStatuses) {
			branches[pr.branch] = pr;
		}

		if (!state.repos[repoPath]) {
			setState("repos", repoPath, {
				branches,
				remoteStatus: null,
				lastPolled: Date.now(),
				issues: [],
				issuesLastPolled: 0,
			});
			return;
		}

		setState("repos", repoPath, "lastPolled", Date.now());

		for (const pr of prStatuses) {
			setState("repos", repoPath, "branches", pr.branch, pr);
		}

		const existing = state.repos[repoPath]?.branches;
		if (existing) {
			const staleKeys = Object.keys(existing).filter((key) => !(key in branches));
			if (staleKeys.length > 0) {
				setState(
					"repos",
					repoPath,
					"branches",
					produce((b) => {
						for (const key of staleKeys) {
							delete b[key];
						}
					}),
				);
			}
		}
	}

	/** Get check summary for a specific branch */
	function getCheckSummary(repoPath: string, branch: string): CheckSummary | null {
		const repo = state.repos[repoPath];
		if (!repo) return null;
		const pr = repo.branches[branch];
		if (!pr) return null;
		return pr.checks;
	}

	/** Get PR status for a specific branch */
	function getPrStatus(repoPath: string, branch: string): BranchPrStatus | null {
		const repo = state.repos[repoPath];
		if (!repo) return null;
		return repo.branches[branch] ?? null;
	}

	/** Get check details for a specific branch */
	function getCheckDetails(repoPath: string, branch: string): CheckDetail[] {
		const repo = state.repos[repoPath];
		if (!repo) return [];
		const pr = repo.branches[branch];
		if (!pr) return [];
		return pr.check_details;
	}

	/** Get open PRs whose branch has no matching local branch/worktree */
	function getRemoteOnlyPrs(repoPath: string, localBranches: Set<string>): BranchPrStatus[] {
		const repo = state.repos[repoPath];
		if (!repo) return [];
		return Object.values(repo.branches).filter(
			(pr) => pr.state?.toUpperCase() === "OPEN" && !localBranches.has(pr.branch),
		);
	}

	/** Get all open PRs regardless of local branch presence */
	function getAllOpenPrs(repoPath: string): BranchPrStatus[] {
		const repo = state.repos[repoPath];
		if (!repo) return [];
		return Object.values(repo.branches).filter((pr) => pr.state?.toUpperCase() === "OPEN");
	}

	/** Get full branch PR data */
	function getBranchPrData(repoPath: string, branch: string): BranchPrStatus | null {
		const repo = state.repos[repoPath];
		if (!repo) return null;
		return repo.branches[branch] ?? null;
	}

	/** Get remote tracking status (ahead/behind) for a repo */
	function getRemoteStatus(repoPath: string): GitHubStatus | null {
		return state.repos[repoPath]?.remoteStatus ?? null;
	}

	/** Get issues for a repo */
	function getRepoIssues(repoPath: string): GitHubIssue[] {
		return state.repos[repoPath]?.issues ?? [];
	}

	/** Update issues for a repo from poll results */
	function updateRepoIssues(repoPath: string, issues: GitHubIssue[]): void {
		if (!state.repos[repoPath]) {
			setState("repos", repoPath, {
				branches: {},
				remoteStatus: null,
				lastPolled: 0,
				issues,
				issuesLastPolled: Date.now(),
			});
			return;
		}
		setState("repos", repoPath, "issues", issues);
		setState("repos", repoPath, "issuesLastPolled", Date.now());
	}

	/** Set issue filter mode — persists to Rust config via settings store.
	 *  Reads from settingsStore as single source of truth for the filter value. */
	function setIssueFilter(filter: import("../types").IssueFilterMode): void {
		settingsStore.setIssueFilter(filter);
		invoke("github_set_issue_filter", { filter }).catch((err) =>
			appLogger.warn("github", "Failed to update issue filter in poller", err),
		);
	}

	/** Poll a single repo's remote tracking status (ahead/behind) */
	async function pollRemoteStatus(path: string): Promise<void> {
		try {
			const remoteStatus = await invoke<GitHubStatus>("get_github_status", { path });
			if (remoteStatus) {
				setState("repos", path, "remoteStatus", remoteStatus);
			}
		} catch {
			// Remote status is best-effort — ignore failures
		}
	}

	/** Tell Rust poller to immediately re-poll a single repo (debounced in Rust) */
	function pollRepo(path: string): void {
		invoke("github_poll_repo", { path }).catch((err) =>
			appLogger.debug("github", `Immediate poll failed for ${path}`, err),
		);
	}

	/** Lazy-load CI check details for a PR and populate the store.
	 *  Called when PrDetailPopover opens to avoid fetching check details on every poll.
	 *  Also recomputes CheckSummary from the fresh data so the badge/ring stay in sync. */
	async function loadCheckDetails(repoPath: string, branch: string, prNumber: number): Promise<void> {
		try {
			const rawChecks = await invoke<{ name: string; status: string; conclusion: string }[]>("get_ci_checks", {
				path: repoPath,
				prNumber,
			});
			const details: CheckDetail[] = rawChecks.map((c) => ({
				context: c.name,
				state: c.conclusion || c.status,
			}));

			let passed = 0,
				failed = 0,
				pending = 0;
			for (const c of rawChecks) {
				switch (c.conclusion) {
					case "success":
					case "neutral":
					case "skipped":
						passed++;
						break;
					case "failure":
					case "cancelled":
					case "timed_out":
					case "action_required":
					case "stale":
						failed++;
						break;
					default:
						pending++;
						break;
				}
			}

			setState("repos", repoPath, "branches", branch, "check_details", details);
			setState("repos", repoPath, "branches", branch, "checks", {
				passed,
				failed,
				pending,
				total: passed + failed + pending,
			});
		} catch (err) {
			appLogger.debug("github", `Failed to load check details for ${repoPath}:${branch}`, err);
		}
	}

	/** Forward visibility changes to Rust poller (controls poll interval) */
	function onVisibilityChange(): void {
		invoke("github_set_visibility", { visible: !document.hidden }).catch((err) =>
			appLogger.debug("github", "Failed to set poller visibility", err),
		);
	}

	/** Handle transition events from Rust poller. `type` is the raw poller tag — a
	 *  superset of the renderable notification types (it also carries watcher-only
	 *  `pushed`/`opened`), so gate the notification add behind `isNotificationType`. */
	function handleTransition(t: {
		type: string;
		repo_path: string;
		branch: string;
		pr_number: number;
		title: string;
	}): void {
		// Watcher-only transitions (pushed/opened) have no popover label — skip them.
		if (!isNotificationType(t.type)) return;

		prNotificationsStore.add({
			repoPath: t.repo_path,
			branch: t.branch,
			prNumber: t.pr_number,
			title: t.title,
			type: t.type,
		});

		if ((t.type === "merged" || t.type === "closed") && prTerminalCallback) {
			prTerminalCallback(t.repo_path, t.branch, t.pr_number, t.type);
		}
		if (t.type === "ci_failed" && ciFailedCallback) {
			ciFailedCallback(t.repo_path, t.branch, t.pr_number);
		}
		if (t.type === "ci_recovered" && ciRecoveredCallback) {
			ciRecoveredCallback(t.repo_path, t.branch, t.pr_number);
		}
	}

	function fetchViewerLogin(): void {
		invoke<string>("get_github_viewer_login")
			.then((login) => setState("viewerLogin", login))
			.catch((err) => appLogger.debug("github", "Failed to fetch viewer login", err));
	}

	/** Start Rust poller and set up event listeners */
	function startPolling(): void {
		const paths = repositoriesStore.getActivePaths();
		const issueFilter = settingsStore.state.issueFilter ?? "disabled";

		const prHideDrafts = settingsStore.state.prHideDrafts;
		invoke("github_start_polling", { paths, issueFilter, prHideDrafts }).catch((err) =>
			appLogger.warn("github", "Failed to start GitHub poller", err),
		);
		fetchViewerLogin();

		listen<{ repo_path: string; statuses: BranchPrStatus[] }>("github-pr-update", (event) => {
			updateRepoData(event.payload.repo_path, event.payload.statuses);
			pollRemoteStatus(event.payload.repo_path);
		}).then((unsub) => unlisteners.push(unsub));

		listen<{ type: string; repo_path: string; branch: string; pr_number: number; title: string }>(
			"github-transition",
			(event) => handleTransition(event.payload),
		).then((unsub) => unlisteners.push(unsub));

		listen<{ repo_path: string; issues: GitHubIssue[] }>("github-issues-update", (event) => {
			updateRepoIssues(event.payload.repo_path, event.payload.issues);
		}).then((unsub) => unlisteners.push(unsub));

		document.addEventListener("visibilitychange", onVisibilityChange);
	}

	/** Stop Rust poller and tear down event listeners */
	function stopPolling(): void {
		invoke("github_stop_polling").catch((err) => appLogger.debug("github", "Failed to stop GitHub poller", err));
		for (const unsub of unlisteners) unsub();
		unlisteners.length = 0;
		document.removeEventListener("visibilitychange", onVisibilityChange);
	}

	/** Directly set remote status for a repo (used by simulator) */
	function setRemoteStatus(repoPath: string, remote: RepoRemoteStatus): void {
		if (!state.repos[repoPath]) {
			setState("repos", repoPath, { branches: {}, remoteStatus: remote, lastPolled: Date.now() });
		} else {
			setState("repos", repoPath, "remoteStatus", remote);
		}
	}

	return {
		state,
		updateRepoData,
		getCheckSummary,
		getPrStatus,
		getCheckDetails,
		getBranchPrData,
		getRemoteOnlyPrs,
		getAllOpenPrs,
		getRemoteStatus,
		setRemoteStatus,
		getRepoIssues,
		setIssueFilter,
		pollIssues(): void {
			const filter = settingsStore.state.issueFilter ?? "disabled";
			invoke("github_set_issue_filter", { filter }).catch((err) =>
				appLogger.debug("github", "Failed to trigger issues re-poll", err),
			);
		},
		setPrHideDrafts(hide: boolean): void {
			invoke("github_set_pr_hide_drafts", { hide }).catch((err) =>
				appLogger.debug("github", "Failed to set pr_hide_drafts", err),
			);
		},
		loadCheckDetails,
		pollRepo,
		startPolling,
		stopPolling,
		/** Register a callback for PR terminal state transitions (merged/closed) */
		setOnPrTerminal(
			cb: ((repoPath: string, branch: string, prNumber: number, type: "merged" | "closed") => void) | null,
		): void {
			prTerminalCallback = cb;
		},
		/** Register a callback for CI failure transitions */
		setOnCiFailed(cb: ((repoPath: string, branch: string, prNumber: number) => void) | null): void {
			ciFailedCallback = cb;
		},
		/** Register a callback for CI recovery (failed → all passing) */
		setOnCiRecovered(cb: ((repoPath: string, branch: string, prNumber: number) => void) | null): void {
			ciRecoveredCallback = cb;
		},
		/** Fire the CI-failed handler on demand (e.g. when auto-heal is enabled while CI
		 *  is already red) — the transition event only fires once on green→red. */
		triggerCiHeal(repoPath: string, branch: string, prNumber: number): void {
			ciFailedCallback?.(repoPath, branch, prNumber);
		},
	};
}

export const githubStore = createGitHubStore();

// Debug registry — expose GitHub PR/CI state for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("github", () => {
	const s = githubStore.state;
	return {
		repos: Object.fromEntries(
			Object.entries(s.repos).map(([path, data]) => [
				path,
				{
					lastPolled: data.lastPolled,
					remoteStatus: data.remoteStatus,
					branches: Object.fromEntries(
						Object.entries(data.branches).map(([name, pr]) => [
							name,
							{
								number: pr.number,
								state: pr.state,
								checks: pr.checks,
								url: pr.url,
							},
						]),
					),
					issuesCount: data.issues?.length ?? 0,
					issuesLastPolled: data.issuesLastPolled,
				},
			]),
		),
	};
});
