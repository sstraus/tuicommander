import { describe, expect, it } from "vitest";
import { canMergePr, effectiveMergeMethod } from "../../components/Sidebar/RepoSection";
import type { BranchPrStatus } from "../../types";
import { canApprovePr } from "../../utils/prMerge";

/** Build a BranchPrStatus with sensible defaults for a merge-eligible PR */
function makePr(overrides: Partial<BranchPrStatus> = {}): BranchPrStatus {
	return {
		branch: "feature/x",
		number: 42,
		title: "Add feature X",
		state: "OPEN",
		url: "https://github.com/o/r/pull/42",
		additions: 10,
		deletions: 2,
		checks: { passed: 3, failed: 0, pending: 0, total: 3 },
		check_details: [],
		author: "dev",
		commits: 1,
		mergeable: "MERGEABLE",
		merge_state_status: "CLEAN",
		review_decision: "APPROVED",
		viewer_did_approve: false,
		labels: [],
		is_draft: false,
		base_ref_name: "main",
		head_ref_oid: "abc123",
		created_at: "2026-01-01T00:00:00Z",
		updated_at: "2026-01-01T00:00:00Z",
		merge_state_label: { label: "Ready", css_class: "clean" },
		review_state_label: { label: "Approved", css_class: "approved" },
		merge_commit_allowed: true,
		squash_merge_allowed: true,
		rebase_merge_allowed: true,
		...overrides,
	};
}

describe("canMergePr", () => {
	it("returns true for an approved, open PR with all checks passed", () => {
		expect(canMergePr(makePr())).toBe(true);
	});

	it("returns false when PR is not approved", () => {
		expect(canMergePr(makePr({ review_decision: "CHANGES_REQUESTED" }))).toBe(false);
		expect(canMergePr(makePr({ review_decision: "REVIEW_REQUIRED" }))).toBe(false);
		expect(canMergePr(makePr({ review_decision: "" }))).toBe(false);
	});

	it("returns false when PR is a draft", () => {
		expect(canMergePr(makePr({ is_draft: true }))).toBe(false);
	});

	it("returns false when PR is closed", () => {
		expect(canMergePr(makePr({ state: "CLOSED" }))).toBe(false);
	});

	it("returns false when PR is merged", () => {
		expect(canMergePr(makePr({ state: "MERGED" }))).toBe(false);
	});

	it("returns false when CI has failures", () => {
		expect(
			canMergePr(
				makePr({
					checks: { passed: 2, failed: 1, pending: 0, total: 3 },
				}),
			),
		).toBe(false);
	});

	it("returns true when CI has pending checks (let GitHub gate the actual merge)", () => {
		expect(
			canMergePr(
				makePr({
					checks: { passed: 2, failed: 0, pending: 1, total: 3 },
				}),
			),
		).toBe(true);
	});

	it("returns true when no CI checks exist (0 total)", () => {
		expect(
			canMergePr(
				makePr({
					checks: { passed: 0, failed: 0, pending: 0, total: 0 },
				}),
			),
		).toBe(true);
	});

	it("handles lowercase state string", () => {
		expect(canMergePr(makePr({ state: "open" }))).toBe(true);
		expect(canMergePr(makePr({ state: "closed" }))).toBe(false);
	});
});

describe("effectiveMergeMethod", () => {
	it("returns preferred method when allowed", () => {
		expect(effectiveMergeMethod(makePr(), "squash")).toBe("squash");
		expect(effectiveMergeMethod(makePr(), "merge")).toBe("merge");
		expect(effectiveMergeMethod(makePr(), "rebase")).toBe("rebase");
	});

	it("falls back when preferred method is not allowed", () => {
		const pr = makePr({ merge_commit_allowed: false });
		expect(effectiveMergeMethod(pr, "merge")).toBe("squash");
	});

	it("picks first available when preferred is disallowed", () => {
		const pr = makePr({ merge_commit_allowed: false, squash_merge_allowed: false });
		expect(effectiveMergeMethod(pr, "merge")).toBe("rebase");
	});

	it("returns squash for squash-only repo", () => {
		const pr = makePr({
			merge_commit_allowed: false,
			squash_merge_allowed: true,
			rebase_merge_allowed: false,
		});
		expect(effectiveMergeMethod(pr, "merge")).toBe("squash");
		expect(effectiveMergeMethod(pr, "rebase")).toBe("squash");
	});

	it("returns preferred as last resort when all are false", () => {
		const pr = makePr({
			merge_commit_allowed: false,
			squash_merge_allowed: false,
			rebase_merge_allowed: false,
		});
		expect(effectiveMergeMethod(pr, "squash")).toBe("squash");
	});
});

describe("canApprovePr", () => {
	/** A PR that needs review from "me" (not author, not yet approved). */
	const reviewable = (overrides: Partial<BranchPrStatus> = {}) =>
		makePr({ author: "alice", review_decision: "REVIEW_REQUIRED", ...overrides });

	it("shows Approve for a normal open PR the viewer did not author or approve", () => {
		expect(canApprovePr(reviewable(), "bob")).toBe(true);
	});

	it("hides Approve on the viewer's own PR (self-approve would 422)", () => {
		expect(canApprovePr(reviewable({ author: "bob" }), "bob")).toBe(false);
	});

	it("hides Approve once the viewer already approved (review_decision still REVIEW_REQUIRED)", () => {
		expect(canApprovePr(reviewable({ viewer_did_approve: true }), "bob")).toBe(false);
	});

	it("hides Approve when the PR is already approved overall", () => {
		expect(canApprovePr(reviewable({ review_decision: "APPROVED" }), "bob")).toBe(false);
	});

	it("hides Approve on draft PRs", () => {
		expect(canApprovePr(reviewable({ is_draft: true }), "bob")).toBe(false);
	});

	it("hides Approve on closed/merged PRs", () => {
		expect(canApprovePr(reviewable({ state: "CLOSED" }), "bob")).toBe(false);
		expect(canApprovePr(reviewable({ state: "MERGED" }), "bob")).toBe(false);
	});

	it("hides Approve while viewerLogin is unknown (avoids leaking the button onto the viewer's own PR)", () => {
		// null identity can't be compared to author, so we suppress until it loads.
		expect(canApprovePr(reviewable(), null)).toBe(false);
	});

	it("hides Approve when pr.author is null and viewerLogin is unknown", () => {
		expect(canApprovePr(reviewable({ author: null as unknown as string }), null)).toBe(false);
	});
});
