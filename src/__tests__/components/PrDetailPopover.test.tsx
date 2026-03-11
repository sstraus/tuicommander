import { describe, it, expect, vi, beforeEach } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { repoSettingsStore } from "../../stores/repoSettings";

const {
  mockGetBranchPrData,
  mockGetCheckSummary,
  mockGetCheckDetails,
} = vi.hoisted(() => ({
  mockGetBranchPrData: vi.fn<() => any>(() => null),
  mockGetCheckSummary: vi.fn<() => any>(() => null),
  mockGetCheckDetails: vi.fn<() => any[]>(() => []),
}));

vi.mock("../../stores/github", () => ({
  githubStore: {
    getBranchPrData: mockGetBranchPrData,
    getCheckSummary: mockGetCheckSummary,
    getCheckDetails: mockGetCheckDetails,
    loadCheckDetails: vi.fn(() => Promise.resolve()),
    pollRepo: vi.fn(),
  },
}));

import { PrDetailPopover } from "../../components/PrDetailPopover/PrDetailPopover";

describe("PrDetailPopover", () => {
  const defaultProps = {
    repoPath: "/repo",
    branch: "feature/x",
    onClose: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockGetBranchPrData.mockReturnValue(null);
    mockGetCheckSummary.mockReturnValue(null);
    mockGetCheckDetails.mockReturnValue([]);
  });

  it("renders PR metadata correctly", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Add new feature",
      state: "OPEN",
      url: "https://github.com/org/repo/pull/42",
      additions: 150,
      deletions: 30,
      author: "alice",
      commits: 5,
      checks: { passed: 2, failed: 0, pending: 0, total: 2 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const popover = container.querySelector(".popover");
    expect(popover).not.toBeNull();

    // Title and number
    expect(popover!.textContent).toContain("Add new feature");
    expect(popover!.textContent).toContain("#42");

    // State badge
    expect(popover!.textContent).toContain("OPEN");

    // Author
    expect(popover!.textContent).toContain("alice");
  });

  it("shows diff stats with correct signs", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Add feature",
      state: "OPEN",
      url: "https://github.com/org/repo/pull/42",
      additions: 150,
      deletions: 30,
      author: "alice",
      commits: 5,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    expect(container.textContent).toContain("+150");
    expect(container.textContent).toContain("-30");
  });

  it("renders check list items", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Add feature",
      state: "OPEN",
      url: "https://github.com/org/repo/pull/42",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 1, failed: 1, pending: 0, total: 2 },
      check_details: [
        { context: "build", state: "SUCCESS" },
        { context: "test", state: "FAILURE" },
      ],
    });
    mockGetCheckDetails.mockReturnValue([
      { context: "build", state: "SUCCESS" },
      { context: "test", state: "FAILURE" },
    ]);

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const checkItems = container.querySelectorAll(".checkItem");
    expect(checkItems.length).toBe(2);
    expect(checkItems[0].textContent).toContain("build");
    expect(checkItems[1].textContent).toContain("test");
  });

  it("calls onClose on Escape", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 1,
      title: "Test",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "bob",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });
    const onClose = vi.fn();
    render(() => <PrDetailPopover {...defaultProps} onClose={onClose} />);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("calls onClose on overlay click", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 1,
      title: "Test",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "bob",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });
    const onClose = vi.fn();
    const { container } = render(() => <PrDetailPopover {...defaultProps} onClose={onClose} />);

    const overlay = container.querySelector(".overlay");
    expect(overlay).not.toBeNull();
    fireEvent.click(overlay!);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("handles no-data gracefully", () => {
    mockGetBranchPrData.mockReturnValue(null);

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const popover = container.querySelector(".popover");
    expect(popover).not.toBeNull();
    expect(popover!.textContent).toContain("No PR data");
  });

  it("shows commit count", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Feature",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 7,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    expect(container.textContent).toContain("7 commit");
  });

  it("shows CI summary when checks exist", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Feature",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 3, failed: 1, pending: 2, total: 6 },
      check_details: [],
    });
    mockGetCheckSummary.mockReturnValue({ passed: 3, failed: 1, pending: 2, total: 6 });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    expect(container.textContent).toContain("1 failed");
    expect(container.textContent).toContain("3 passed");
  });

  it("shows merge readiness indicator for CLEAN state", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Ready PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 2, failed: 0, pending: 0, total: 2 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".mergeStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Ready to merge");
    expect(badge!.classList.contains("clean")).toBe(true);
  });

  it("shows merge readiness indicator for BEHIND state", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Behind PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "BEHIND",
      merge_state_label: { label: "Behind base", css_class: "behind" },
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".mergeStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Behind base");
    expect(badge!.classList.contains("behind")).toBe(true);
  });

  it("shows merge readiness indicator for CONFLICTING state", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Conflict PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      mergeable: "CONFLICTING",
      merge_state_status: "DIRTY",
      merge_state_label: { label: "Conflicts", css_class: "conflicting" },
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".mergeStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Conflicts");
    expect(badge!.classList.contains("conflicting")).toBe(true);
  });

  it("shows merge readiness indicator for BLOCKED state", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Blocked PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 1, pending: 0, total: 1 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "BLOCKED",
      merge_state_label: { label: "Blocked", css_class: "blocked" },
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".mergeStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Blocked");
    expect(badge!.classList.contains("blocked")).toBe(true);
  });

  it("shows review status badge for APPROVED", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Approved PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      review_decision: "APPROVED",
      merge_state_label: null,
      review_state_label: { label: "Approved", css_class: "approved" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".reviewStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Approved");
    expect(badge!.classList.contains("approved")).toBe(true);
  });

  it("shows review status badge for CHANGES_REQUESTED", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Changes PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      review_decision: "CHANGES_REQUESTED",
      merge_state_label: null,
      review_state_label: { label: "Changes requested", css_class: "changes-requested" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".reviewStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Changes requested");
    expect(badge!.classList.contains("changesRequested")).toBe(true);
  });

  it("shows review status badge for REVIEW_REQUIRED", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Needs review",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      review_decision: "REVIEW_REQUIRED",
      merge_state_label: null,
      review_state_label: { label: "Review required", css_class: "review-required" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".reviewStateBadge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("Review required");
    expect(badge!.classList.contains("reviewRequired")).toBe(true);
  });

  it("does not show review status badge when review_decision is empty", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "No review",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      review_decision: "",
      merge_state_label: null,
      review_state_label: null,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const badge = container.querySelector(".reviewStateBadge");
    expect(badge).toBeNull();
  });

  it("displays PR labels as colored pills", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Labeled PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [
        { name: "bug", color: "d73a4a", text_color: "#e5e5e5", background_color: "rgba(215, 58, 74, 0.3)" },
        { name: "enhancement", color: "a2eeef", text_color: "#1e1e1e", background_color: "rgba(162, 238, 239, 0.3)" },
      ],
      is_draft: false,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const labels = container.querySelectorAll(".label");
    expect(labels.length).toBe(2);
    expect(labels[0].textContent).toBe("bug");
    expect(labels[1].textContent).toBe("enhancement");
    // Label should have background color from Rust backend
    const style = (labels[0] as HTMLElement).style;
    expect(style.backgroundColor).toBeTruthy();
  });

  it("applies correct rgba background and text color for dark label (d73a4a)", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Dark label PR",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [{ name: "bug", color: "d73a4a", text_color: "#e5e5e5", background_color: "rgba(215, 58, 74, 0.3)" }],
      is_draft: false,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const label = container.querySelector(".label") as HTMLElement;
    expect(label).not.toBeNull();
    // Pre-computed by Rust: rgba(215, 58, 74, 0.3)
    expect(label.style.backgroundColor).toBe("rgba(215, 58, 74, 0.3)");
    expect(label.style.borderColor).toBe("#d73a4a");
    // Pre-computed by Rust: dark color => light text (#e5e5e5)
    expect(label.style.color).toBe("#e5e5e5");
  });

  it("applies correct rgba background and text color for light label (a2eeef)", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Light label PR",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [{ name: "enhancement", color: "a2eeef", text_color: "#1e1e1e", background_color: "rgba(162, 238, 239, 0.3)" }],
      is_draft: false,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const label = container.querySelector(".label") as HTMLElement;
    expect(label).not.toBeNull();
    // Pre-computed by Rust: rgba(162, 238, 239, 0.3)
    expect(label.style.backgroundColor).toBe("rgba(162, 238, 239, 0.3)");
    expect(label.style.borderColor).toBe("#a2eeef");
    // Pre-computed by Rust: light color => dark text (#1e1e1e)
    expect(label.style.color).toBe("#1e1e1e");
  });

  it("applies correct colors for pure black (000000) and pure white (ffffff)", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "B&W labels PR",
      state: "OPEN",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [
        { name: "black", color: "000000", text_color: "#e5e5e5", background_color: "rgba(0, 0, 0, 0.3)" },
        { name: "white", color: "ffffff", text_color: "#1e1e1e", background_color: "rgba(255, 255, 255, 0.3)" },
      ],
      is_draft: false,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const labels = container.querySelectorAll(".label") as NodeListOf<HTMLElement>;
    expect(labels.length).toBe(2);
    // Black: pre-computed by Rust, luminance=0 < 128 => light text
    expect(labels[0].style.backgroundColor).toBe("rgba(0, 0, 0, 0.3)");
    expect(labels[0].style.color).toBe("#e5e5e5");
    // White: pre-computed by Rust, luminance=255 > 128 => dark text
    expect(labels[1].style.backgroundColor).toBe("rgba(255, 255, 255, 0.3)");
    expect(labels[1].style.color).toBe("#1e1e1e");
  });

  it("does not show labels section when labels array is empty", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "No labels",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [],
      is_draft: false,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const labelsContainer = container.querySelector(".labels");
    expect(labelsContainer).toBeNull();
  });

  it("shows Draft text in state badge for draft PRs", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Draft PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [],
      is_draft: true,
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const stateBadge = container.querySelector(".stateBadge");
    expect(stateBadge).not.toBeNull();
    expect(stateBadge!.textContent).toBe("Draft");
    expect(stateBadge!.classList.contains("draft")).toBe(true);
  });

  it("shows merge direction (head -> base)", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Feature PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [],
      is_draft: false,
      base_ref_name: "main",
      created_at: "2026-01-15T10:00:00Z",
      updated_at: "2026-01-15T12:00:00Z",
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const mergeDir = container.querySelector(".mergeDirection");
    expect(mergeDir).not.toBeNull();
    expect(mergeDir!.textContent).toContain("feature/x");
    expect(mergeDir!.textContent).toContain("main");
  });

  it("shows relative timestamps for creation and update", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-15T15:00:00Z"));

    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Timestamped PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [],
      is_draft: false,
      base_ref_name: "main",
      created_at: "2026-01-15T10:00:00Z",
      updated_at: "2026-01-15T12:00:00Z",
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const timestamps = container.querySelector(".timestamps");
    expect(timestamps).not.toBeNull();
    expect(timestamps!.textContent).toContain("5h ago");
    expect(timestamps!.textContent).toContain("3h ago");

    vi.useRealTimers();
  });

  it("does not show merge direction when base_ref_name is empty", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "No base",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      labels: [],
      is_draft: false,
      base_ref_name: "",
      created_at: "",
      updated_at: "",
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const mergeDir = container.querySelector(".mergeDirection");
    expect(mergeDir).toBeNull();
  });

  it("shows state badge with MERGED state", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Merged PR",
      state: "MERGED",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const stateBadge = container.querySelector(".stateBadge");
    expect(stateBadge).not.toBeNull();
    expect(stateBadge!.textContent).toBe("MERGED");
    expect(stateBadge!.classList.contains("merged")).toBe(true);
  });

  it("shows closed state badge with 'closed' CSS class", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Closed PR",
      state: "CLOSED",
      url: "",
      additions: 0,
      deletions: 0,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const stateBadge = container.querySelector(".stateBadge");
    expect(stateBadge).not.toBeNull();
    expect(stateBadge!.textContent).toBe("CLOSED");
    expect(stateBadge!.classList.contains("closed")).toBe(true);
  });

  it("suppresses merge state badge for MERGED PR", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Merged PR",
      state: "MERGED",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: { label: "Approved", css_class: "approved" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const mergeStateBadge = container.querySelector(".mergeStateBadge");
    const reviewStateBadge = container.querySelector(".reviewStateBadge");
    expect(mergeStateBadge).toBeNull();
    expect(reviewStateBadge).toBeNull();
  });

  it("suppresses merge and review state badges for CLOSED PR", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Closed PR",
      state: "CLOSED",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      mergeable: "CONFLICTING",
      merge_state_status: "DIRTY",
      merge_state_label: { label: "Conflicts", css_class: "conflicting" },
      review_state_label: { label: "Review required", css_class: "review-required" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const mergeStateBadge = container.querySelector(".mergeStateBadge");
    const reviewStateBadge = container.querySelector(".reviewStateBadge");
    expect(mergeStateBadge).toBeNull();
    expect(reviewStateBadge).toBeNull();
  });

  it("still shows merge and review badges for OPEN PR", () => {
    mockGetBranchPrData.mockReturnValue({
      branch: "feature/x",
      number: 42,
      title: "Open PR",
      state: "OPEN",
      url: "",
      additions: 10,
      deletions: 5,
      author: "alice",
      commits: 1,
      checks: { passed: 0, failed: 0, pending: 0, total: 0 },
      check_details: [],
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_state_label: { label: "Ready to merge", css_class: "clean" },
      review_state_label: { label: "Approved", css_class: "approved" },
    });

    const { container } = render(() => <PrDetailPopover {...defaultProps} />);
    const mergeStateBadge = container.querySelector(".mergeStateBadge");
    const reviewStateBadge = container.querySelector(".reviewStateBadge");
    expect(mergeStateBadge).not.toBeNull();
    expect(reviewStateBadge).not.toBeNull();
  });

  describe("405 merge method auto-fallback", () => {
    const mergeablePr = {
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
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_commit_allowed: true,
      squash_merge_allowed: true,
      rebase_merge_allowed: true,
    };

    beforeEach(() => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { prMergeStrategy: null }); // reset to global default
      mockGetBranchPrData.mockReturnValue(mergeablePr);
    });

    it("falls back to squash when merge is rejected with 405", async () => {
      mockInvoke
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
        .mockResolvedValueOnce("abc123sha");

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("merge_pr_via_github", expect.objectContaining({ mergeMethod: "squash" }));
      });
      // Persists the working method for future merges
      expect(repoSettingsStore.getEffective("/repo")?.prMergeStrategy).toBe("squash");
    });

    it("falls back to rebase when both merge and squash are rejected with 405", async () => {
      mockInvoke
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Squash merging is not allowed."))
        .mockResolvedValueOnce("abc123sha");

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("merge_pr_via_github", expect.objectContaining({ mergeMethod: "rebase" }));
      });
      expect(repoSettingsStore.getEffective("/repo")?.prMergeStrategy).toBe("rebase");
    });

    it("shows error when all merge methods are rejected", async () => {
      mockInvoke
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Squash merging is not allowed."))
        .mockRejectedValueOnce(new Error("GitHub merge failed (405): Rebase merging is not allowed."));

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(container.textContent).toContain("405");
      });
    });

    it("does not retry on non-405 errors", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("Network error"));

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(container.textContent).toContain("Network error");
      });
      // Only one merge call — no retry (other invoke calls may be from store persistence)
      const mergeCalls = mockInvoke.mock.calls.filter(
        (args: unknown[]) => args[0] === "merge_pr_via_github",
      );
      expect(mergeCalls).toHaveLength(1);
    });
  });

  describe("post-merge cleanup dialog", () => {
    const mergeablePr = {
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
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      merge_commit_allowed: true,
      squash_merge_allowed: true,
      rebase_merge_allowed: true,
    };

    beforeEach(() => {
      repoSettingsStore.getOrCreate("/repo", "Repo");
      repoSettingsStore.update("/repo", { prMergeStrategy: null });
      mockGetBranchPrData.mockReturnValue(mergeablePr);
    });

    it("shows cleanup dialog after successful merge", async () => {
      // merge succeeds, then git status returns clean
      mockInvoke
        .mockResolvedValueOnce("sha123") // merge_pr_via_github
        .mockResolvedValueOnce({ stdout: "" }); // run_git_command (git status --porcelain)

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(container.textContent).toContain("Post-merge cleanup");
        expect(container.textContent).toContain("feature/x");
        expect(container.textContent).toContain("main");
      });
    });

    it("calls pollRepo AFTER setCleanupCtx (cleanup dialog is mounted first)", async () => {
      const { githubStore } = await import("../../stores/github");
      const pollSpy = vi.mocked(githubStore.pollRepo);
      pollSpy.mockClear();

      // Track when pollRepo is called relative to DOM update
      let cleanupDialogExistedWhenPollCalled = false;
      let containerRef: HTMLElement | null = null;
      pollSpy.mockImplementation(() => {
        // At the moment pollRepo fires, check if cleanup dialog is in the DOM
        if (containerRef) {
          cleanupDialogExistedWhenPollCalled =
            containerRef.textContent?.includes("Post-merge cleanup") ?? false;
        }
      });

      mockInvoke
        .mockResolvedValueOnce("sha123") // merge_pr_via_github
        .mockResolvedValueOnce({ stdout: "" }); // run_git_command

      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      containerRef = container;
      const mergeBtn = container.querySelector(".mergeBtn") as HTMLButtonElement;
      fireEvent.click(mergeBtn);

      await waitFor(() => {
        expect(pollSpy).toHaveBeenCalledWith("/repo");
      });
      expect(cleanupDialogExistedWhenPollCalled).toBe(true);
    });
  });

  describe("merge button label reflects effective merge method", () => {
    const makePr = (overrides = {}) => ({
      branch: "feature/x", number: 42, title: "Feature", state: "OPEN",
      url: "", additions: 10, deletions: 5, author: "alice", commits: 1,
      checks: { passed: 2, failed: 0, pending: 0, total: 2 }, check_details: [],
      labels: [], is_draft: false, base_ref_name: "main", head_ref_oid: "abc",
      created_at: "", updated_at: "", merge_state_label: null, review_state_label: null,
      review_decision: "APPROVED", mergeable: "MERGEABLE", merge_state_status: "CLEAN",
      merge_commit_allowed: true, squash_merge_allowed: true, rebase_merge_allowed: true,
      ...overrides,
    });

    it("shows Merge when preferred method is merge and repo allows it", () => {
      mockGetBranchPrData.mockReturnValue(makePr());
      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const btn = container.querySelector(".mergeBtn");
      expect(btn?.textContent).toBe("Merge");
    });

    it("shows Squash & Merge when only squash is allowed", () => {
      mockGetBranchPrData.mockReturnValue(makePr({ merge_commit_allowed: false, squash_merge_allowed: true, rebase_merge_allowed: false }));
      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const btn = container.querySelector(".mergeBtn");
      expect(btn?.textContent).toBe("Squash & Merge");
    });

    it("shows Rebase & Merge when only rebase is allowed", () => {
      mockGetBranchPrData.mockReturnValue(makePr({ merge_commit_allowed: false, squash_merge_allowed: false, rebase_merge_allowed: true }));
      const { container } = render(() => <PrDetailPopover {...defaultProps} />);
      const btn = container.querySelector(".mergeBtn");
      expect(btn?.textContent).toBe("Rebase & Merge");
    });
  });
});
