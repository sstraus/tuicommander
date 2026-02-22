import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

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
});
