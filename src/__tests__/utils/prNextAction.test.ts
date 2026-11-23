import { describe, expect, it } from "vitest";
import type { BranchPrStatus } from "../../types";
import { prNextAction } from "../../utils/prNextAction";

const ready: BranchPrStatus = {
	branch: "feature/x",
	number: 42,
	title: "Feature",
	state: "OPEN",
	url: "https://github.com/org/repo/pull/42",
	additions: 10,
	deletions: 5,
	author: "alice",
	commits: 1,
	checks: { passed: 2, failed: 0, pending: 0, total: 2 },
	check_details: [],
	labels: [],
	is_draft: false,
	base_ref_name: "main",
	head_ref_oid: "abc",
	created_at: "",
	updated_at: "",
	merge_state_label: null,
	review_state_label: null,
	review_decision: "APPROVED",
	viewer_did_approve: false,
	mergeable: "MERGEABLE",
	conflict_state: "clear",
	merge_state_status: "CLEAN",
	merge_commit_allowed: true,
	squash_merge_allowed: true,
	rebase_merge_allowed: true,
	unresolved_threads: 0,
	unresolved_threads_truncated: false,
};
const pr = (patch: Partial<BranchPrStatus>): BranchPrStatus => ({ ...ready, ...patch });
const failing = { checks: { passed: 1, failed: 1, pending: 0, total: 2 } };

describe("prNextAction", () => {
	it("asks a reviewer to review someone else's PR even when it is red", () => {
		// catches: Approve hidden behind the author's CI or conflict work
		expect(prNextAction(pr({ review_decision: "REVIEW_REQUIRED", conflict_state: "conflicting" }), "bob")).toBe(
			"review",
		);
	});

	it("never asks the author to review their own PR", () => {
		// catches: offering Approve that GitHub refuses with a 422
		expect(prNextAction(pr({ review_decision: "REVIEW_REQUIRED" }), "alice")).toBe(null);
	});

	it("does not ask for a review again once the viewer approved", () => {
		expect(prNextAction(pr({ review_decision: "REVIEW_REQUIRED", viewer_did_approve: true, ...failing }), "bob")).toBe(
			null,
		);
	});

	it("routes the author to conflicts, then CI, then comments", () => {
		// catches: an order that suggests fixing CI on a branch that cannot merge anyway
		const own = { review_decision: "CHANGES_REQUESTED", unresolved_threads: 2 } satisfies Partial<BranchPrStatus>;
		expect(prNextAction(pr({ ...own, ...failing, conflict_state: "conflicting" }), "alice")).toBe("resolve-conflicts");
		expect(prNextAction(pr({ ...own, ...failing }), "alice")).toBe("fix-ci");
		expect(prNextAction(pr(own), "alice")).toBe("address-comments");
		expect(prNextAction(pr({ unresolved_threads: 0, unresolved_threads_truncated: true }), "alice")).toBe(
			"address-comments",
		);
	});

	it("offers merge only when the shared readiness verdict is ready", () => {
		// catches: Merge next to a badge that reads blocked
		expect(prNextAction(ready, "bob")).toBe("merge");
		expect(prNextAction(pr({ unresolved_threads: 1 }), "bob")).toBe(null);
		expect(prNextAction(pr({ conflict_state: "conflicting" }), "bob")).toBe(null);
	});

	it("suggests waiting for running checks instead of merging", () => {
		expect(prNextAction(pr({ checks: { passed: 1, failed: 0, pending: 1, total: 2 } }), "bob")).toBe("wait-checks");
	});

	it("has no next action for drafts and finished PRs", () => {
		expect(prNextAction(pr({ is_draft: true, review_decision: "REVIEW_REQUIRED" }), "bob")).toBe(null);
		expect(prNextAction(pr({ state: "MERGED" }), "bob")).toBe(null);
		expect(prNextAction(pr({ state: "CLOSED" }), "bob")).toBe(null);
	});

	it("applies no author rule while the viewer is unknown", () => {
		expect(prNextAction(pr({ ...failing }), null)).toBe(null);
	});
});
