import { createStore, produce, reconcile } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { groupGitHubPollingPaths } from "../services/githubPollingPaths";
import { notifyPrTransition } from "../services/prNativeNotifications";
import { rpc } from "../transport";
import { getRemoteBaseUrl, getRepoConnection } from "../transportRuntime";
import type { BranchPrStatus, CheckDetail, CheckSummary, GitHubIssue, GitHubStatus } from "../types";
import { type RemoteEventPayload, remoteEventOrigin } from "../utils/remoteEventOrigin";
import { appLogger } from "./appLogger";
import { isNotificationType, prNotificationsStore } from "./prNotifications";
import { repositoriesStore } from "./repositories";
import { settingsStore } from "./settings";
import { workItemLinksStore } from "./workItemLinks";

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
	/** Ahead/behind of checkouts that are not a polled repo root (linked worktrees), by path */
	checkoutStatus: Record<string, RepoRemoteStatus>;
	issuesLoading: boolean;
	circuitBreakerOpen: boolean;
	viewerLogin: string | null;
}

function createGitHubStore() {
	const [state, setState] = createStore<GitHubStoreState>({
		repos: {},
		checkoutStatus: {},
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
	/** Callback fired when a PR becomes blocked by merge conflicts (mergeable → CONFLICTING) */
	let conflictCallback: ((repoPath: string, branch: string, prNumber: number) => void) | null = null;

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

		// The poller re-sends every branch every cycle, and each PR arrives as a
		// fresh object from IPC deserialization. Installing it wholesale replaced
		// the nested values too — `check_details` above all — so every consumer
		// reading them woke on every poll even when the PR was byte-identical.
		// `reconcile` walks the two and touches only what actually differs.
		// `key: null` because the nested arrays hold plain records with no id.
		for (const pr of prStatuses) {
			const existing = state.repos[repoPath]?.branches?.[pr.branch];
			setState("repos", repoPath, "branches", pr.branch, existing ? reconcile(pr, { key: null }) : pr);
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
		return pr.check_details ?? [];
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

	/** Get remote tracking status (ahead/behind) for a repo root or a worktree checkout */
	function getRemoteStatus(path: string): GitHubStatus | null {
		return state.repos[path]?.remoteStatus ?? state.checkoutStatus[path] ?? null;
	}

	/** Milliseconds of the last remote poll of a repo; 0 when it was never polled */
	function getLastPolled(repoPath: string): number {
		return state.repos[repoPath]?.lastPolled ?? 0;
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
				// A path without a repo entry is a worktree checkout: writing it under `repos`
				// would create a phantom repo without `branches`.
				if (state.repos[path]) setState("repos", path, "remoteStatus", remoteStatus);
				else setState("checkoutStatus", path, remoteStatus);
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
			const rawChecks = await invoke<{ name: string; status: string; conclusion: string; html_url: string }[]>(
				"get_ci_checks",
				{
					path: repoPath,
					prNumber,
				},
			);
			const details: CheckDetail[] = rawChecks.map((c) => ({
				context: c.name,
				state: c.conclusion || c.status,
				html_url: c.html_url ?? "",
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
	function ownsNotice(payload: RemoteEventPayload & { repo_path: string }): boolean {
		const origin = remoteEventOrigin(payload);
		if (payload.__tuic_origin !== undefined && !origin) return false;
		return (
			getRepoConnection(payload.repo_path) === origin?.connection && (!origin || !!getRemoteBaseUrl(origin.connection))
		);
	}

	async function handleTransition(
		t: RemoteEventPayload & {
			type: string;
			repo_path: string;
			branch: string;
			pr_number: number;
			title: string;
		},
	): Promise<void> {
		if (!ownsNotice(t)) return;
		const origin = remoteEventOrigin(t);
		// Watcher-only transitions (pushed/opened) have no popover label — skip them.
		if (!isNotificationType(t.type)) return;

		if (origin) {
			const duplicate = prNotificationsStore.state.notifications.some(
				(n) =>
					!n.dismissed &&
					n.connectionId === origin.connection &&
					n.repoPath === t.repo_path &&
					n.prNumber === t.pr_number &&
					n.type === t.type,
			);
			if (duplicate) return;
			// The event precedes the snapshot; fetch from its owner rather than local cached URLs.
			try {
				const statuses = await rpc<BranchPrStatus[]>("get_repo_pr_statuses", { path: t.repo_path }, origin.connection);
				if (!ownsNotice(t)) return;
				updateRepoData(t.repo_path, statuses);
			} catch (error) {
				appLogger.debug("github", "Remote PR snapshot refresh failed", error);
				return;
			}
			if (
				prNotificationsStore.state.notifications.some(
					(n) =>
						!n.dismissed &&
						n.connectionId === origin.connection &&
						n.repoPath === t.repo_path &&
						n.prNumber === t.pr_number &&
						n.type === t.type,
				)
			)
				return;
		}
		const pr = getPrStatus(t.repo_path, t.branch);
		const url = pr?.url?.replace(/\/pull\/\d+(?:[?#].*)?$/, `/pull/${t.pr_number}`);
		prNotificationsStore.add({
			repoPath: t.repo_path,
			branch: t.branch,
			prNumber: t.pr_number,
			url,
			title: origin ? `[${origin.name}] ${t.title}` : t.title,
			...(origin ? { connectionId: origin.connection } : {}),
			type: t.type,
		});

		// Transitions are emitted before `github-pr-update`, so the store can still hold the branch's
		// previous PR: take only the repo base from its URL and address the transitioning PR number.
		if (url) {
			notifyPrTransition({
				repoName: `${origin ? `[${origin.name}] ` : ""}${repositoriesStore.get(t.repo_path)?.displayName ?? t.repo_path}`,
				prNumber: t.pr_number,
				title: t.title,
				type: t.type,
				linkedItem: origin ? undefined : workItemLinksStore.get(t.repo_path, t.branch),
				url,
			});
		}

		// Remote transitions may notify but must never trigger local repository automation.
		if (origin) return;
		if ((t.type === "merged" || t.type === "closed") && prTerminalCallback) {
			prTerminalCallback(t.repo_path, t.branch, t.pr_number, t.type);
		}
		if (t.type === "ci_failed" && ciFailedCallback) {
			ciFailedCallback(t.repo_path, t.branch, t.pr_number);
		}
		if (t.type === "ci_recovered" && ciRecoveredCallback) {
			ciRecoveredCallback(t.repo_path, t.branch, t.pr_number);
		}
		if (t.type === "blocked" && conflictCallback) {
			conflictCallback(t.repo_path, t.branch, t.pr_number);
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
		const grouped = groupGitHubPollingPaths(paths);
		for (const [owner, ownedPaths] of grouped) {
			if (owner && !getRemoteBaseUrl(owner)) continue;
			const args = { paths: ownedPaths, issueFilter, prHideDrafts };
			const start = owner ? rpc("github_start_polling", args, owner) : invoke("github_start_polling", args);
			start.catch((err) => appLogger.warn("github", "Failed to start GitHub poller", err));
		}
		fetchViewerLogin();

		listen<RemoteEventPayload & { repo_path: string; statuses: BranchPrStatus[] }>("github-pr-update", (event) => {
			if (!ownsNotice(event.payload)) return;
			updateRepoData(event.payload.repo_path, event.payload.statuses);
			pollRemoteStatus(event.payload.repo_path);
			for (const checkout of Object.keys(state.checkoutStatus)) pollRemoteStatus(checkout);
		}).then((unsub) => unlisteners.push(unsub));

		listen<RemoteEventPayload & { type: string; repo_path: string; branch: string; pr_number: number; title: string }>(
			"github-transition",
			(event) => void handleTransition(event.payload),
		).then((unsub) => unlisteners.push(unsub));

		listen<RemoteEventPayload & { repo_path: string; issues: GitHubIssue[] }>("github-issues-update", (event) => {
			if (!ownsNotice(event.payload)) return;
			updateRepoIssues(event.payload.repo_path, event.payload.issues);
		}).then((unsub) => unlisteners.push(unsub));

		listen<RemoteEventPayload & { id: string; status: string }>("remote-connection-status", (event) => {
			const notice = event.payload;
			if (notice.__tuic_origin !== undefined || notice.status !== "connected" || !getRemoteBaseUrl(notice.id)) return;
			const ownedPaths = groupGitHubPollingPaths(repositoriesStore.getActivePaths()).get(notice.id) ?? [];
			if (!ownedPaths.length) return;
			rpc(
				"github_start_polling",
				{
					paths: ownedPaths,
					issueFilter: settingsStore.state.issueFilter ?? "disabled",
					prHideDrafts: settingsStore.state.prHideDrafts,
				},
				notice.id,
			).catch((error) => appLogger.debug("github", "Remote GitHub poller restart failed", error));
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
		getLastPolled,
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
		pollRemoteStatus,
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
		/** Register a callback for merge-conflict blocks (mergeable → CONFLICTING) */
		setOnConflict(cb: ((repoPath: string, branch: string, prNumber: number) => void) | null): void {
			conflictCallback = cb;
		},
		/** Fire the CI-failed handler on demand (e.g. when auto-heal is enabled while CI
		 *  is already red) — the transition event only fires once on green→red. */
		triggerCiHeal(repoPath: string, branch: string, prNumber: number): void {
			ciFailedCallback?.(repoPath, branch, prNumber);
		},
		/** Fire the conflict handler on demand (e.g. when auto-heal is enabled while the
		 *  PR is already conflicting) — the transition event only fires once on the edge. */
		triggerConflictHeal(repoPath: string, branch: string, prNumber: number): void {
			conflictCallback?.(repoPath, branch, prNumber);
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
