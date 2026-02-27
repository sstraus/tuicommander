import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";
import { repositoriesStore } from "./repositories";
import { prNotificationsStore, type PrNotificationType } from "./prNotifications";
import type { BranchPrStatus, CheckSummary, CheckDetail, GitHubStatus } from "../types";

const PR_STATE_STORAGE_KEY = "github:pr_state";

const BASE_INTERVAL = 30000; // 30 seconds
const HIDDEN_INTERVAL = 120000; // 2 minutes when tab not visible
const MAX_INTERVAL = 300000; // 5 minutes (backoff cap)

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
}

/** GitHub store state */
interface GitHubStoreState {
  repos: Record<string, RepoGitHubData>;
}

function createGitHubStore() {
  const [state, setState] = createStore<GitHubStoreState>({
    repos: {},
  });

  let intervalId: number | null = null;
  let currentInterval = BASE_INTERVAL;
  let pollingActive = false;
  /** True until the first poll completes — startup poll includes MERGED state for offline transition detection */
  let isStartupPoll = true;
  /** Pending per-repo immediate polls (debounced to 2s to coalesce rapid git events) */
  const pendingRepoPollTimers = new Map<string, number>();

  /** Detect significant PR state transitions and emit notifications */
  function detectTransitions(repoPath: string, oldPr: BranchPrStatus, newPr: BranchPrStatus): void {
    const oldState = oldPr.state?.toUpperCase();
    const newState = newPr.state?.toUpperCase();

    let type: PrNotificationType | null = null;

    // Terminal state transitions
    if (oldState !== "MERGED" && newState === "MERGED") {
      type = "merged";
    } else if (oldState !== "CLOSED" && newState === "CLOSED") {
      type = "closed";
    }
    // Actionable state transitions (only for open PRs)
    else if (newState === "OPEN") {
      // Became blocked (conflicts)
      if (oldPr.mergeable !== "CONFLICTING" && newPr.mergeable === "CONFLICTING") {
        type = "blocked";
      }
      // CI failed
      else if ((oldPr.checks?.failed ?? 0) === 0 && (newPr.checks?.failed ?? 0) > 0) {
        type = "ci_failed";
      }
      // Changes requested
      else if (oldPr.review_decision !== "CHANGES_REQUESTED" && newPr.review_decision === "CHANGES_REQUESTED") {
        type = "changes_requested";
      }
      // Became ready to merge
      else if (
        (oldPr.mergeable !== "MERGEABLE" || oldPr.review_decision !== "APPROVED" || (oldPr.checks?.failed ?? 0) > 0) &&
        newPr.mergeable === "MERGEABLE" && newPr.review_decision === "APPROVED" && (newPr.checks?.failed ?? 0) === 0
      ) {
        type = "ready";
      }
    }

    if (type) {
      prNotificationsStore.add({
        repoPath,
        branch: newPr.branch,
        prNumber: newPr.number,
        title: newPr.title,
        type,
      });
    }
  }

  /** Update repo data from a batch poll result (only updates changed branches) */
  function updateRepoData(repoPath: string, prStatuses: BranchPrStatus[]): void {
    const branches: Record<string, BranchPrStatus> = {};
    for (const pr of prStatuses) {
      branches[pr.branch] = pr;
    }

    // Initialize repo entry if it doesn't exist yet
    if (!state.repos[repoPath]) {
      setState("repos", repoPath, { branches, remoteStatus: null, lastPolled: Date.now() });
      return;
    }

    // Detect state transitions before updating
    const existing = state.repos[repoPath]?.branches;
    if (existing) {
      for (const pr of prStatuses) {
        const oldPr = existing[pr.branch];
        if (oldPr) {
          detectTransitions(repoPath, oldPr, pr);
        }
      }
    }

    // Update lastPolled separately so branch data comparisons are granular
    setState("repos", repoPath, "lastPolled", Date.now());

    // Update each branch individually so SolidJS can diff unchanged values
    for (const pr of prStatuses) {
      setState("repos", repoPath, "branches", pr.branch, pr);
    }

    // Remove branches no longer present in poll results
    if (existing) {
      for (const key of Object.keys(existing)) {
        if (!(key in branches)) {
          setState("repos", repoPath, "branches", key, undefined!);
        }
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

  /** Persist current PR state to localStorage for offline transition detection on next startup */
  function persistPrState(): void {
    try {
      localStorage.setItem(PR_STATE_STORAGE_KEY, JSON.stringify(state.repos));
    } catch {
      // localStorage can throw in private browsing or when storage is full — ignore
    }
  }

  /** Seed PR store from persisted state (without emitting transition notifications).
   *  Called before the first poll so startup can detect offline transitions. */
  function loadPersistedPrState(): void {
    try {
      const raw = localStorage.getItem(PR_STATE_STORAGE_KEY);
      if (!raw) return;
      const repos = JSON.parse(raw) as Record<string, RepoGitHubData>;
      for (const [repoPath, repoData] of Object.entries(repos)) {
        if (repoData.branches && typeof repoData.branches === "object") {
          setState("repos", repoPath, { branches: repoData.branches, remoteStatus: null, lastPolled: 0 });
        }
      }
    } catch {
      // Corrupted or missing persisted state — ignore
    }
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

  /** Poll all repos using a single batched GraphQL call.
   *  Falls back to per-repo individual calls if the batch fails. */
  async function pollAll(): Promise<void> {
    const paths = repositoriesStore.getPaths();
    if (paths.length === 0) return;

    const includeMerged = isStartupPoll;
    let hitRateLimit = false;
    let batchSucceeded = false;

    // Attempt a single batched GraphQL call for all repos
    try {
      const allStatuses = await invoke<Record<string, BranchPrStatus[]>>("get_all_pr_statuses", {
        paths,
        includeMerged,
      });
      for (const path of paths) {
        const statuses = allStatuses[path] ?? [];
        updateRepoData(path, statuses);
      }
      batchSucceeded = true;
    } catch (err) {
      const errStr = String(err);
      if (errStr.startsWith("rate-limit:")) {
        hitRateLimit = true;
        appLogger.warn("github", `Rate limited on batch poll: ${errStr}`);
      } else {
        appLogger.warn("github", "Batch PR poll failed, falling back to per-repo calls", err);
      }
    }

    // Fall back to per-repo calls if batch failed (and wasn't rate-limited)
    if (!batchSucceeded && !hitRateLimit) {
      await Promise.all(
        paths.map(async (path) => {
          try {
            const statuses = await invoke<BranchPrStatus[]>("get_repo_pr_statuses", { path });
            updateRepoData(path, statuses);
          } catch (err) {
            const errStr = String(err);
            if (errStr.startsWith("rate-limit:")) {
              hitRateLimit = true;
              appLogger.warn("github", `Rate limited: ${errStr}`);
            } else {
              appLogger.error("github", `Failed to poll PR statuses for ${path}`, err);
            }
          }
        })
      );
    }

    // Fetch remote tracking status (ahead/behind) for all repos in parallel — cached in Rust
    await Promise.all(paths.map(pollRemoteStatus));

    if (hitRateLimit) {
      currentInterval = MAX_INTERVAL;
      scheduleNext();
    } else {
      currentInterval = document.hidden ? HIDDEN_INTERVAL : BASE_INTERVAL;
      persistPrState();
    }

    isStartupPoll = false;
  }

  /** Immediately poll a single repo for PR status (debounced: coalesces rapid git events) */
  function pollRepo(path: string): void {
    // Cancel any pending timer for this repo
    const existing = pendingRepoPollTimers.get(path);
    if (existing) window.clearTimeout(existing);

    const timerId = window.setTimeout(async () => {
      pendingRepoPollTimers.delete(path);
      try {
        const statuses = await invoke<BranchPrStatus[]>("get_repo_pr_statuses", { path });
        updateRepoData(path, statuses);
        await pollRemoteStatus(path);
      } catch (err) {
        appLogger.debug("github", `Immediate poll failed for ${path}`, err);
      }
    }, 2000);

    pendingRepoPollTimers.set(path, timerId);
  }

  /** Lazy-load CI check details for a PR and populate the store.
   *  Called when PrDetailPopover opens to avoid fetching check details on every poll. */
  async function loadCheckDetails(repoPath: string, branch: string, prNumber: number): Promise<void> {
    try {
      const rawChecks = await invoke<{ name: string; status: string; conclusion: string }[]>(
        "get_ci_checks",
        { path: repoPath, prNumber },
      );
      const details: CheckDetail[] = rawChecks.map((c) => ({
        context: c.name,
        state: c.conclusion || c.status,
      }));
      setState("repos", repoPath, "branches", branch, "check_details", details);
    } catch (err) {
      appLogger.debug("github", `Failed to load check details for ${repoPath}:${branch}`, err);
    }
  }

  /** Handle visibility changes to pause/resume polling */
  function onVisibilityChange(): void {
    if (!pollingActive) return;
    if (document.hidden) {
      clearScheduled();
    } else {
      pollAll().catch((err) => appLogger.warn("github", "Poll failed on visibility change", err));
      scheduleNext();
    }
  }

  /** Start background polling */
  function startPolling(): void {
    pollingActive = true;
    isStartupPoll = true;
    loadPersistedPrState();
    document.addEventListener("visibilitychange", onVisibilityChange);
    pollAll().catch((err) => appLogger.warn("github", "Initial poll failed", err));
    scheduleNext();
  }

  /** Stop background polling */
  function stopPolling(): void {
    pollingActive = false;
    clearScheduled();
    document.removeEventListener("visibilitychange", onVisibilityChange);
  }

  /** Clear scheduled interval */
  function clearScheduled(): void {
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = null;
    }
  }

  /** Schedule next poll */
  function scheduleNext(): void {
    clearScheduled();
    intervalId = window.setInterval(pollAll, currentInterval);
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
    getRemoteStatus,
    setRemoteStatus,
    loadCheckDetails,
    pollRepo,
    startPolling,
    stopPolling,
  };
}

export const githubStore = createGitHubStore();
