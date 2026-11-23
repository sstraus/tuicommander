import type { BranchPrStatus } from "../types";
import { canApprovePr } from "./prMerge";
import { prReadinessOf } from "./prReadiness";

/** The action a UI v3 PR panel puts first (docs/frontend/ui-v3.md). */
export type PrNextAction = "review" | "address-comments" | "resolve-conflicts" | "fix-ci" | "merge" | "wait-checks";

/** Picks the next action for the viewer. It follows the shared readiness verdict, so the
 *  panel never suggests merging a PR whose badge reads blocked. `viewerLogin` null means
 *  the identity is not known yet, so no author-specific action applies. */
export function prNextAction(pr: BranchPrStatus, viewerLogin: string | null): PrNextAction | null {
	const readiness = prReadinessOf(pr);
	if (readiness === "draft" || readiness === "merged" || readiness === "closed") return null;
	if (canApprovePr(pr, viewerLogin)) return "review";
	if (viewerLogin !== null && pr.author === viewerLogin) {
		if (readiness === "conflict") return "resolve-conflicts";
		if (readiness === "ci-failed") return "fix-ci";
		if (readiness === "changes-requested" || readiness === "unresolved-comments") return "address-comments";
	}
	if (readiness === "ready") return "merge";
	if (readiness === "ci-pending") return "wait-checks";
	return null;
}
