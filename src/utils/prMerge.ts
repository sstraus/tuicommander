import { invoke } from "../invoke";
import type { MergeStrategy } from "../stores/repoDefaults";
import type { BranchPrStatus } from "../types";

const MERGE_METHODS = ["merge", "squash", "rebase"] as const;

/** Pick the merge method for a PR based on repo-allowed methods and user preference.
 *  Falls back to the first allowed method if preferred is not permitted.
 *  Note: merge_commit_allowed/squash_merge_allowed/rebase_merge_allowed reflect
 *  repo-level settings but may not account for branch protection rulesets —
 *  callers should handle 405 errors from GitHub as a signal to update the strategy. */
export function effectiveMergeMethod(pr: BranchPrStatus, preferred: MergeStrategy): MergeStrategy {
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

/** Whether the Approve button should be shown for a PR.
 *  Hidden when: not open, draft, already approved overall, the viewer is the PR
 *  author (GitHub rejects self-approval with 422), or the viewer already approved
 *  (overall review_decision may still be REVIEW_REQUIRED for other reviewers). */
export function canApprovePr(pr: BranchPrStatus, viewerLogin: string | null): boolean {
	// Identity not yet resolved: hide the button rather than risk offering it on
	// the viewer's own PR (which GitHub rejects with a 422). `author !== null`
	// would otherwise be true and leak the button until viewerLogin loads.
	if (viewerLogin === null) return false;
	return (
		pr.state?.toUpperCase() === "OPEN" &&
		!pr.is_draft &&
		pr.review_decision !== "APPROVED" &&
		!pr.viewer_did_approve &&
		pr.author !== viewerLogin
	);
}

/** Whether a GitHub merge error is a 405 method-not-allowed response. */
export function isMergeMethodNotAllowed(error: unknown): boolean {
	return String(error).includes("405");
}

/** Whether the PR was already merged (GitHub returns 409 or "not mergeable"). */
export function isAlreadyMerged(error: unknown): boolean {
	const msg = String(error);
	return msg.includes("not mergeable") || msg.includes("already been merged") || msg.includes("409");
}

/** Try to merge a PR, automatically falling back through all merge methods on 405.
 *  Returns the method that succeeded. Throws on non-405 errors or if all methods fail. */
export async function mergeWithFallback(
	repoPath: string,
	prNumber: number,
	preferred: MergeStrategy,
): Promise<MergeStrategy> {
	const methodOrder = [preferred, ...MERGE_METHODS.filter((m) => m !== preferred)];
	let lastError: unknown;
	for (const method of methodOrder) {
		try {
			await invoke("merge_pr_via_github", {
				repoPath,
				prNumber,
				mergeMethod: method,
			});
			return method;
		} catch (e) {
			if (isMergeMethodNotAllowed(e)) {
				lastError = e;
				continue;
			}
			throw e;
		}
	}
	throw lastError;
}
