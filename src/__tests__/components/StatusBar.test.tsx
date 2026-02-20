import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";

// Use vi.hoisted so these are available to vi.mock factories (which are hoisted)
const {
  mockGitHubStatus,
  mockGitHubRefresh,
  mockGetActive,
  mockGetBranchPrData,
} = vi.hoisted(() => ({
  mockGitHubStatus: vi.fn<() => any>(() => null),
  mockGitHubRefresh: vi.fn(),
  mockGetActive: vi.fn<() => any>(() => null),
  mockGetBranchPrData: vi.fn<() => any>(() => null),
}));

vi.mock("../../hooks/useGitHub", () => ({
  useGitHub: () => ({
    status: mockGitHubStatus,
    loading: () => false,
    error: () => null,
    refresh: mockGitHubRefresh,
    startPolling: vi.fn(),
    stopPolling: vi.fn(),
  }),
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    getActive: mockGetActive,
  },
}));

vi.mock("../../hooks/useRepository", () => ({
  useRepository: () => ({
    renameBranch: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock("../../stores/github", () => ({
  githubStore: {
    getBranchPrData: mockGetBranchPrData,
    getCheckSummary: vi.fn(() => null),
    getCheckDetails: vi.fn(() => []),
  },
}));

const { mockDictationState } = vi.hoisted(() => ({
  mockDictationState: {
    enabled: false,
    recording: false,
    processing: false,
    loading: false,
    hotkey: "F8",
  },
}));

vi.mock("../../stores/dictation", () => ({
  dictationStore: {
    state: mockDictationState,
  },
}));

import { StatusBar } from "../../components/StatusBar/StatusBar";

/** Build a mock BranchPrStatus for testing */
function makePrData(overrides: Record<string, unknown> = {}) {
  return {
    branch: "main",
    number: 42,
    title: "Fix bug",
    state: "OPEN",
    url: "https://github.com/test/pr/42",
    additions: 10,
    deletions: 5,
    checks: { passed: 3, failed: 0, pending: 0, total: 3 },
    check_details: [],
    author: "alice",
    commits: 1,
    mergeable: "MERGEABLE",
    merge_state_status: "CLEAN",
    review_decision: "APPROVED",
    labels: [],
    is_draft: false,
    base_ref_name: "main",
    created_at: "2025-01-01T00:00:00Z",
    updated_at: "2025-01-02T00:00:00Z",
    merge_state_label: null,
    review_state_label: null,
    ...overrides,
  };
}

/** Find the CI badge wrapper span (the one with the onClick handler) */
function findCiBadgeWrapper(container: HTMLElement): HTMLElement {
  const badges = container.querySelectorAll("#github-status .status-badge");
  const ciBadgeEl = Array.from(badges).find((b) => b.textContent?.includes("CI"));
  if (!ciBadgeEl?.parentElement) throw new Error("CI badge wrapper not found");
  return ciBadgeEl.parentElement;
}

describe("StatusBar", () => {
  const defaultProps = {
    fontSize: 14,
    defaultFontSize: 14,
    statusInfo: "Ready",
    onToggleDiff: vi.fn(),
    onToggleMarkdown: vi.fn(),
    onDictationStart: vi.fn(),
    onDictationStop: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockGitHubStatus.mockReturnValue(null);
    mockGetActive.mockReturnValue(null);
    mockGetBranchPrData.mockReturnValue(null);
    mockDictationState.enabled = false;
    mockDictationState.recording = false;
    mockDictationState.processing = false;
    mockDictationState.loading = false;
  });

  it("renders status info text", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const statusInfo = container.querySelector("#status-info");
    expect(statusInfo).not.toBeNull();
    expect(statusInfo!.textContent).toBe("Ready");
  });

  it("renders MD toggle button", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const mdBtn = container.querySelector("#md-toggle");
    expect(mdBtn).not.toBeNull();
    expect(mdBtn!.textContent).toContain("MD");
  });

  it("renders Diff toggle button", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const diffBtn = container.querySelector("#diff-toggle");
    expect(diffBtn).not.toBeNull();
    expect(diffBtn!.textContent).toContain("Diff");
  });

  it("calls onToggleMarkdown when MD button clicked", () => {
    const onToggleMarkdown = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} onToggleMarkdown={onToggleMarkdown} />
    ));
    const mdBtn = container.querySelector("#md-toggle")!;
    fireEvent.click(mdBtn);
    expect(onToggleMarkdown).toHaveBeenCalledOnce();
  });

  it("calls onToggleDiff when Diff button clicked", () => {
    const onToggleDiff = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} onToggleDiff={onToggleDiff} />
    ));
    const diffBtn = container.querySelector("#diff-toggle")!;
    fireEvent.click(diffBtn);
    expect(onToggleDiff).toHaveBeenCalledOnce();
  });

  it("shows zoom indicator", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const statusBar = container.querySelector("#status-bar");
    expect(statusBar).not.toBeNull();
    const statusSection = container.querySelector(".status-section");
    expect(statusSection).not.toBeNull();
  });

  it("does not render github-status when github status is null", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const githubStatus = container.querySelector("#github-status");
    expect(githubStatus).toBeNull();
  });

  it("shows github-status with BranchBadge when github status exists", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "feature/test",
      ahead: 2,
      behind: 1,
    });
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const githubStatus = container.querySelector("#github-status");
    expect(githubStatus).not.toBeNull();
    const badge = githubStatus!.querySelector(".status-badge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("feature/test");
    expect(badge!.textContent).toContain("\u21912");
    expect(badge!.textContent).toContain("\u21931");
  });

  it("shows PrBadge when githubStore has PR data for active branch", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData());
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));
    const githubStatus = container.querySelector("#github-status");
    expect(githubStatus).not.toBeNull();
    const badges = githubStatus!.querySelectorAll(".status-badge");
    const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"));
    expect(prBadge).toBeDefined();
  });

  it("shows CiBadge when githubStore has checks with total > 0", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 3, failed: 0, pending: 0, total: 3 },
    }));
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));
    const githubStatus = container.querySelector("#github-status");
    expect(githubStatus).not.toBeNull();
    const badges = githubStatus!.querySelectorAll(".status-badge");
    const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI passed"));
    expect(ciBadge).toBeDefined();
  });

  it("CI popover opens on CiBadge click", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 3, failed: 0, pending: 0, total: 3 },
    }));
    mockInvoke.mockResolvedValue([
      { name: "Build", status: "completed", conclusion: "success", html_url: "https://example.com/1" },
      { name: "Test", status: "completed", conclusion: "failure", html_url: "https://example.com/2" },
    ]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      const popover = container.querySelector(".ci-popover");
      expect(popover).not.toBeNull();
    });

    const header = container.querySelector(".ci-popover-header h4");
    expect(header).not.toBeNull();
    expect(header!.textContent).toBe("CI Checks");
  });

  it("CI popover shows check items after loading", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 3, failed: 0, pending: 0, total: 3 },
    }));
    mockInvoke.mockResolvedValue([
      { name: "Build", status: "completed", conclusion: "success", html_url: "https://example.com/1" },
      { name: "Test", status: "completed", conclusion: "failure", html_url: "https://example.com/2" },
    ]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      const checkItems = container.querySelectorAll(".ci-check-item");
      expect(checkItems.length).toBe(2);
    });

    const checkNames = container.querySelectorAll(".ci-check-name");
    expect(checkNames[0].textContent).toBe("Build");
    expect(checkNames[1].textContent).toBe("Test");

    const checkIcons = container.querySelectorAll(".ci-check-icon");
    expect(checkIcons[0].textContent).toBe("\u2713");
    expect(checkIcons[1].textContent).toBe("\u2717");

    expect(checkIcons[0].classList.contains("success")).toBe(true);
    expect(checkIcons[1].classList.contains("failure")).toBe(true);
  });

  it("CI popover shows empty state when no checks found", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
    }));
    mockInvoke.mockResolvedValue([]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      const empty = container.querySelector(".ci-popover-empty");
      expect(empty).not.toBeNull();
      expect(empty!.textContent).toBe("No CI checks found");
    });
  });

  it("CI popover closes on close button click", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
    }));
    mockInvoke.mockResolvedValue([]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      expect(container.querySelector(".ci-popover")).not.toBeNull();
    });

    const closeBtn = container.querySelector(".ci-popover-close");
    expect(closeBtn).not.toBeNull();
    fireEvent.click(closeBtn!);

    await waitFor(() => {
      expect(container.querySelector(".ci-popover")).toBeNull();
    });
  });

  it("CI popover closes on overlay click", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
    }));
    mockInvoke.mockResolvedValue([]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      expect(container.querySelector(".ci-popover")).not.toBeNull();
    });

    const overlay = container.querySelector(".ci-popover-overlay");
    expect(overlay).not.toBeNull();
    fireEvent.click(overlay!);

    await waitFor(() => {
      expect(container.querySelector(".ci-popover")).toBeNull();
    });
  });

  it("CI popover handles fetch error gracefully", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 0, failed: 1, pending: 0, total: 1 },
    }));
    mockInvoke.mockRejectedValue(new Error("Network error"));
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      const empty = container.querySelector(".ci-popover-empty");
      expect(empty).not.toBeNull();
      expect(empty!.textContent).toBe("No CI checks found");
    });

    expect(consoleSpy).toHaveBeenCalledWith("Failed to fetch CI checks:", expect.any(Error));
    consoleSpy.mockRestore();
  });

  it("does not render CI badge when no currentRepoPath", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    // Even if githubStore has PR data, without currentRepoPath the activePrData memo returns null
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 1, failed: 0, pending: 0, total: 1 },
    }));
    const { container } = render(() => (
      <StatusBar {...defaultProps} />
    ));

    const badges = container.querySelectorAll("#github-status .status-badge");
    const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI"));
    expect(ciBadge).toBeUndefined();
  });

  it("shows BranchPopover when BranchBadge is clicked", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "feature/test",
      ahead: 0,
      behind: 0,
    });
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const branchBadge = container.querySelector("#github-status .status-badge");
    expect(branchBadge).not.toBeNull();
    fireEvent.click(branchBadge!);

    const popover = container.querySelector(".branch-popover");
    expect(popover).not.toBeNull();
  });

  it("shows CiBadge with failure state", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 2, failed: 1, pending: 0, total: 3 },
    }));
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));
    const badges = container.querySelectorAll(".status-badge");
    const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI failed"));
    expect(ciBadge).toBeDefined();
  });

  it("shows BranchBadge with ahead-only count", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "develop",
      ahead: 3,
      behind: 0,
    });
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const badge = container.querySelector("#github-status .status-badge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain("develop");
    expect(badge!.textContent).toContain("\u21913");
  });

  it("CI check item with pending conclusion shows bullet icon", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData({
      checks: { passed: 0, failed: 0, pending: 1, total: 1 },
    }));
    mockInvoke.mockResolvedValue([
      { name: "Lint", status: "in_progress", conclusion: "pending", html_url: "" },
    ]);
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));

    const ciBadge = findCiBadgeWrapper(container);
    fireEvent.click(ciBadge);

    await waitFor(() => {
      const checkItems = container.querySelectorAll(".ci-check-item");
      expect(checkItems.length).toBe(1);
    });

    const icon = container.querySelector(".ci-check-icon");
    expect(icon!.textContent).toBe("\u25CF"); // bullet for pending
    expect(icon!.classList.contains("pending")).toBe(true);
  });

  it("shows hotkey hints on toggle buttons", () => {
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const hints = container.querySelectorAll(".hotkey-hint");
    expect(hints.length).toBe(4);
  });

  it("opens PR detail popover when PrBadge is clicked", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "main",
      ahead: 0,
      behind: 0,
    });
    mockGetBranchPrData.mockReturnValue(makePrData());
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" />
    ));
    const badges = container.querySelectorAll(".status-badge");
    const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"))!;
    fireEvent.click(prBadge);
    const popover = container.querySelector(".pr-detail-popover");
    expect(popover).not.toBeNull();
  });

  it("calls onBranchRenamed prop after branch rename in BranchPopover", async () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "feature/old",
      ahead: 0,
      behind: 0,
    });
    const onBranchRenamed = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} currentRepoPath="/repo" onBranchRenamed={onBranchRenamed} />
    ));

    const branchBadge = container.querySelector("#github-status .status-badge")!;
    fireEvent.click(branchBadge);

    const popover = container.querySelector(".branch-popover");
    expect(popover).not.toBeNull();

    const input = popover!.querySelector("input")!;
    fireEvent.input(input, { target: { value: "feature/new" } });

    const renameBtn = popover!.querySelector(".branch-popover-rename")!;
    fireEvent.click(renameBtn);

    await waitFor(() => {
      expect(onBranchRenamed).toHaveBeenCalledWith("feature/old", "feature/new");
    });
    expect(mockGitHubRefresh).toHaveBeenCalled();
  });

  it("shows mic button when dictation is enabled", () => {
    mockDictationState.enabled = true;
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const micBtn = container.querySelector("#mic-toggle");
    expect(micBtn).not.toBeNull();
  });

  it("does not show mic button when dictation is disabled", () => {
    mockDictationState.enabled = false;
    const { container } = render(() => <StatusBar {...defaultProps} />);
    const micBtn = container.querySelector("#mic-toggle");
    expect(micBtn).toBeNull();
  });

  it("calls onDictationStart on mouseDown", () => {
    mockDictationState.enabled = true;
    const onDictationStart = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} onDictationStart={onDictationStart} />
    ));
    const micBtn = container.querySelector("#mic-toggle")!;
    fireEvent.mouseDown(micBtn, { button: 0 });
    expect(onDictationStart).toHaveBeenCalled();
  });

  it("calls onDictationStop on mouseUp when recording", () => {
    mockDictationState.enabled = true;
    mockDictationState.recording = true;
    const onDictationStop = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} onDictationStop={onDictationStop} />
    ));
    const micBtn = container.querySelector("#mic-toggle")!;
    fireEvent.mouseUp(micBtn, { button: 0 });
    expect(onDictationStop).toHaveBeenCalled();
  });

  it("calls onDictationStop on mouseLeave when recording", () => {
    mockDictationState.enabled = true;
    mockDictationState.recording = true;
    const onDictationStop = vi.fn();
    const { container } = render(() => (
      <StatusBar {...defaultProps} onDictationStop={onDictationStop} />
    ));
    const micBtn = container.querySelector("#mic-toggle")!;
    fireEvent.mouseLeave(micBtn);
    expect(onDictationStop).toHaveBeenCalled();
  });

  it("BranchPopover uses null when currentRepoPath is undefined", () => {
    mockGitHubStatus.mockReturnValue({
      current_branch: "feature/test",
      ahead: 0,
      behind: 0,
    });
    const { container } = render(() => (
      <StatusBar {...defaultProps} />
    ));
    const branchBadge = container.querySelector("#github-status .status-badge")!;
    fireEvent.click(branchBadge);

    const popover = container.querySelector(".branch-popover");
    expect(popover).not.toBeNull();
  });
});
