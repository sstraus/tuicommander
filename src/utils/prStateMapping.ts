/** PR state classification — maps raw GitHub merge/review state to display labels.
 *
 * This is the frontend fallback; the canonical implementation lives in Rust
 * (github.rs classify_merge_state / classify_review_state). Keep in sync.
 */

export interface StateLabel {
  label: string;
  cssClass: string;
}

/** Classify merge readiness from mergeable + mergeStateStatus fields */
export function classifyMergeState(
  mergeable: string | null,
  mergeStateStatus: string | null,
): StateLabel | null {
  // CONFLICTING takes priority (merge would fail)
  if (mergeable === "CONFLICTING") {
    return { label: "Conflicts", cssClass: "conflicting" };
  }

  switch (mergeStateStatus) {
    case "CLEAN":
      return { label: "Ready to merge", cssClass: "clean" };
    case "BEHIND":
      return { label: "Behind base", cssClass: "behind" };
    case "BLOCKED":
      return { label: "Blocked", cssClass: "blocked" };
    case "UNSTABLE":
      return { label: "Unstable", cssClass: "blocked" };
    case "DRAFT":
      return { label: "Draft", cssClass: "behind" };
    case "DIRTY":
      return { label: "Conflicts", cssClass: "conflicting" };
    default:
      return null; // UNKNOWN, HAS_HOOKS — don't show
  }
}

/** Classify review decision into display label */
export function classifyReviewState(
  reviewDecision: string | null,
): StateLabel | null {
  switch (reviewDecision) {
    case "APPROVED":
      return { label: "Approved", cssClass: "approved" };
    case "CHANGES_REQUESTED":
      return { label: "Changes requested", cssClass: "changes-requested" };
    case "REVIEW_REQUIRED":
      return { label: "Review required", cssClass: "review-required" };
    default:
      return null;
  }
}
