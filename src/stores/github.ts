import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import { repositoriesStore } from "./repositories";
import { prNotificationsStore, type PrNotificationType } from "./prNotifications";
import type { BranchPrStatus, CheckSummary, CheckDetail } from "../types";

const BASE_INTERVAL = 30000; // 30 seconds
const HIDDEN_INTERVAL = 120000; // 2 minutes when tab not visible
const MAX_INTERVAL = 300000; // 5 minutes (backoff cap)

/** Per-repo PR/CI data */
interface RepoGitHubData {
  branches: Record<string, BranchPrStatus>;
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
  let consecutiveErrors = 0;
  let pollingActive = false;

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
      setState("repos", repoPath, { branches, lastPolled: Date.now() });
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

  /** Poll all repos for PR status */
  async function pollAll(): Promise<void> {
    const paths = repositoriesStore.getPaths();
    if (paths.length === 0) return;

    let hitRateLimit = false;

    try {
      await Promise.all(
        paths.map(async (path) => {
          try {
            const statuses = await invoke<BranchPrStatus[]>("get_repo_pr_statuses", { path });
            updateRepoData(path, statuses);
          } catch (err) {
            const errStr = String(err);
            if (errStr.startsWith("rate-limit:")) {
              hitRateLimit = true;
              console.warn(`[GitHub] Rate limited: ${errStr}`);
            } else {
              console.error(`Failed to poll PR statuses for ${path}:`, err);
            }
          }
        })
      );

      if (hitRateLimit) {
        currentInterval = MAX_INTERVAL;
        scheduleNext();
      } else {
        consecutiveErrors = 0;
        currentInterval = document.hidden ? HIDDEN_INTERVAL : BASE_INTERVAL;
      }
    } catch {
      consecutiveErrors++;
      currentInterval = Math.min(BASE_INTERVAL * Math.pow(2, consecutiveErrors), MAX_INTERVAL);
    }
  }

  /** Handle visibility changes to pause/resume polling */
  function onVisibilityChange(): void {
    if (!pollingActive) return;
    if (document.hidden) {
      clearScheduled();
    } else {
      pollAll().catch((err) => console.error("[GitHub] Poll failed on visibility change:", err));
      scheduleNext();
    }
  }

  /** Start background polling */
  function startPolling(): void {
    pollingActive = true;
    document.addEventListener("visibilitychange", onVisibilityChange);
    pollAll().catch((err) => console.error("[GitHub] Initial poll failed:", err));
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

  return {
    state,
    updateRepoData,
    getCheckSummary,
    getPrStatus,
    getCheckDetails,
    getBranchPrData,
    startPolling,
    stopPolling,
  };
}

export const githubStore = createGitHubStore();
