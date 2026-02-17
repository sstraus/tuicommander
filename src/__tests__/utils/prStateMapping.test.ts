import { describe, it, expect } from "vitest";
import { classifyMergeState, classifyReviewState } from "../../utils/prStateMapping";

describe("classifyMergeState", () => {
  it("returns Conflicts when mergeable is CONFLICTING regardless of status", () => {
    expect(classifyMergeState("CONFLICTING", "CLEAN")).toEqual({
      label: "Conflicts",
      cssClass: "conflicting",
    });
    expect(classifyMergeState("CONFLICTING", null)).toEqual({
      label: "Conflicts",
      cssClass: "conflicting",
    });
  });

  it("returns Ready to merge for CLEAN status", () => {
    expect(classifyMergeState("MERGEABLE", "CLEAN")).toEqual({
      label: "Ready to merge",
      cssClass: "clean",
    });
  });

  it("returns Behind base for BEHIND status", () => {
    expect(classifyMergeState("MERGEABLE", "BEHIND")).toEqual({
      label: "Behind base",
      cssClass: "behind",
    });
  });

  it("returns Blocked for BLOCKED status", () => {
    expect(classifyMergeState("MERGEABLE", "BLOCKED")).toEqual({
      label: "Blocked",
      cssClass: "blocked",
    });
  });

  it("returns Unstable for UNSTABLE status", () => {
    expect(classifyMergeState("MERGEABLE", "UNSTABLE")).toEqual({
      label: "Unstable",
      cssClass: "blocked",
    });
  });

  it("returns Draft for DRAFT status", () => {
    expect(classifyMergeState("MERGEABLE", "DRAFT")).toEqual({
      label: "Draft",
      cssClass: "behind",
    });
  });

  it("returns Conflicts for DIRTY status", () => {
    expect(classifyMergeState("MERGEABLE", "DIRTY")).toEqual({
      label: "Conflicts",
      cssClass: "conflicting",
    });
  });

  it("returns null for UNKNOWN status", () => {
    expect(classifyMergeState("MERGEABLE", "UNKNOWN")).toBeNull();
  });

  it("returns null for HAS_HOOKS status", () => {
    expect(classifyMergeState("MERGEABLE", "HAS_HOOKS")).toBeNull();
  });

  it("returns null when both args are null", () => {
    expect(classifyMergeState(null, null)).toBeNull();
  });

  it("returns null for unrecognized status", () => {
    expect(classifyMergeState("MERGEABLE", "SOMETHING_NEW")).toBeNull();
  });
});

describe("classifyReviewState", () => {
  it("returns Approved for APPROVED", () => {
    expect(classifyReviewState("APPROVED")).toEqual({
      label: "Approved",
      cssClass: "approved",
    });
  });

  it("returns Changes requested for CHANGES_REQUESTED", () => {
    expect(classifyReviewState("CHANGES_REQUESTED")).toEqual({
      label: "Changes requested",
      cssClass: "changes-requested",
    });
  });

  it("returns Review required for REVIEW_REQUIRED", () => {
    expect(classifyReviewState("REVIEW_REQUIRED")).toEqual({
      label: "Review required",
      cssClass: "review-required",
    });
  });

  it("returns null for empty string", () => {
    expect(classifyReviewState("")).toBeNull();
  });

  it("returns null for null", () => {
    expect(classifyReviewState(null)).toBeNull();
  });

  it("returns null for unrecognized value", () => {
    expect(classifyReviewState("DISMISSED")).toBeNull();
  });
});
