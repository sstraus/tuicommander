import { githubStore } from "../stores/github";
import { userActivityStore } from "../stores/userActivity";
import type { BranchPrStatus } from "../types";

/** Accumulated activity time (ms) per merged PR, keyed by `repoPath:branch:prNumber`.
 *  Tracks how long the user has been active since the merged PR was first seen. */
const mergedActivityAccum = new Map<string, { ms: number; lastCheck: number }>();

/** Activity-based grace period (ms) before hiding merged PRs */
export const MERGED_GRACE_MS = 5 * 60 * 1000; // 5 minutes

/** Get PR status with lifecycle rules:
 *  - CLOSED: hidden immediately
 *  - MERGED: hidden after 5 min of accumulated user activity
 *  - OPEN: shown as-is */
export function activePrStatus(repoPath: string, branch: string): BranchPrStatus | null {
  const pr = githubStore.getPrStatus(repoPath, branch);
  if (!pr) return null;

  const state = pr.state?.toUpperCase();

  // CLOSED: never show
  if (state === "CLOSED") return null;

  // MERGED: activity-based grace period
  if (state === "MERGED") {
    const prKey = `${repoPath}:${branch}:${pr.number}`;
    const now = Date.now();
    const lastActivity = userActivityStore.lastActivityAt();

    let entry = mergedActivityAccum.get(prKey);
    if (!entry) {
      entry = { ms: 0, lastCheck: now };
      mergedActivityAccum.set(prKey, entry);
    }

    // Accumulate: if user was active within the last 2s, add elapsed since last check
    if (lastActivity > 0 && now - lastActivity < 2000) {
      const elapsed = now - entry.lastCheck;
      if (elapsed > 0 && elapsed < 60_000) { // cap at 60s to avoid jumps
        entry.ms += elapsed;
      }
    }
    entry.lastCheck = now;

    if (entry.ms >= MERGED_GRACE_MS) {
      return null; // Keep entry so PR stays hidden on subsequent ticks
    }
  }

  return pr;
}

/** Reset merged activity accumulators (for testing) */
export function _resetMergedActivityAccum(): void {
  mergedActivityAccum.clear();
}
