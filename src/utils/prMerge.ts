import type { BranchPrStatus } from "../types";

/** Pick the merge method for a PR based on repo-allowed methods and user preference.
 *  Falls back to the first allowed method if preferred is not permitted.
 *  Note: merge_commit_allowed/squash_merge_allowed/rebase_merge_allowed reflect
 *  repo-level settings but may not account for branch protection rulesets —
 *  callers should handle 405 errors from GitHub as a signal to update the strategy. */
export function effectiveMergeMethod(pr: BranchPrStatus, preferred: string): string {
  const allowed: Record<string, boolean> = {
    merge: pr.merge_commit_allowed,
    squash: pr.squash_merge_allowed,
    rebase: pr.rebase_merge_allowed,
  };
  if (allowed[preferred]) return preferred;
  for (const method of ["squash", "rebase", "merge"] as const) {
    if (allowed[method]) return method;
  }
  return preferred; // all false shouldn't happen — send preferred anyway
}

/** Whether a GitHub merge error is a 405 method-not-allowed response. */
export function isMergeMethodNotAllowed(error: unknown): boolean {
  return String(error).includes("405");
}
