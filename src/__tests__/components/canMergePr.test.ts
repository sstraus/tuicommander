import { describe, it, expect } from "vitest";
import { canMergePr } from "../../components/Sidebar/RepoSection";
import type { BranchPrStatus } from "../../types";

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
    labels: [],
    is_draft: false,
    base_ref_name: "main",
    head_ref_oid: "abc123",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    merge_state_label: { label: "Ready", css_class: "clean" },
    review_state_label: { label: "Approved", css_class: "approved" },
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
    expect(canMergePr(makePr({
      checks: { passed: 2, failed: 1, pending: 0, total: 3 },
    }))).toBe(false);
  });

  it("returns false when CI has pending checks", () => {
    expect(canMergePr(makePr({
      checks: { passed: 2, failed: 0, pending: 1, total: 3 },
    }))).toBe(false);
  });

  it("returns true when no CI checks exist (0 total)", () => {
    expect(canMergePr(makePr({
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
    }))).toBe(true);
  });

  it("handles lowercase state string", () => {
    expect(canMergePr(makePr({ state: "open" }))).toBe(true);
    expect(canMergePr(makePr({ state: "closed" }))).toBe(false);
  });
});
