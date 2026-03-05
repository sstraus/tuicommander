import { describe, it, expect, vi, beforeEach } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { repoSettingsStore } from "../../stores/repoSettings";
import { RemoteOnlyPrPopover } from "../../components/Sidebar/RepoSection";
import type { BranchPrStatus } from "../../types";

const {
  mockPollRepo,
} = vi.hoisted(() => ({
  mockPollRepo: vi.fn(),
}));

vi.mock("../../stores/github", () => ({
  githubStore: {
    pollRepo: mockPollRepo,
    getCheckSummary: vi.fn(() => null),
    getCheckDetails: vi.fn(() => []),
    loadCheckDetails: vi.fn(() => Promise.resolve()),
    getBranchPrData: vi.fn(() => mergeablePr),
  },
}));

vi.mock("../../stores/mdTabs", () => ({
  mdTabsStore: {
    addPrDiff: vi.fn(),
  },
}));

vi.mock("../../stores/repoDefaults", () => ({
  repoDefaultsStore: {
    state: { prMergeStrategy: "merge" },
  },
}));

const mergeablePr: BranchPrStatus = {
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

const defaultProps = {
  prs: [mergeablePr],
  repoPath: "/repo",
  onClose: vi.fn(),
  onCheckout: vi.fn(),
};

describe("RemoteOnlyPrPopover — 405 merge method not allowed dialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    repoSettingsStore.getOrCreate("/repo", "Repo");
    repoSettingsStore.update("/repo", { prMergeStrategy: null }); // reset to global default
  });

  it("shows 405 dialog when merge is rejected with method not allowed", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed on this repository."));

    const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
    // Expand the PR row first to reveal action buttons
    const row = container.querySelector(".remoteOnlyRow");
    expect(row).not.toBeNull();
    fireEvent.click(row!);
    const mergeBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    ) as HTMLButtonElement | undefined;
    expect(mergeBtn).not.toBeUndefined();
    fireEvent.click(mergeBtn!);

    await waitFor(() => {
      expect(container.textContent).toContain("squash");
    });
  });

  it("updates prMergeStrategy to squash and retries on dialog confirm", async () => {
    mockInvoke
      .mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed on this repository."))
      .mockResolvedValueOnce("abc123sha");

    const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
    const row = container.querySelector(".remoteOnlyRow");
    expect(row).not.toBeNull();
    fireEvent.click(row!);
    const mergeBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    ) as HTMLButtonElement | undefined;
    expect(mergeBtn).not.toBeUndefined();
    fireEvent.click(mergeBtn!);

    await waitFor(() => expect(container.textContent).toContain("squash"));

    const confirmBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.toLowerCase().includes("squash") || b.textContent?.toLowerCase().includes("switch"),
    );
    expect(confirmBtn).not.toBeNull();
    fireEvent.click(confirmBtn!);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenLastCalledWith("merge_pr_via_github", expect.objectContaining({ mergeMethod: "squash" }));
    });
    expect(repoSettingsStore.getEffective("/repo")?.prMergeStrategy).toBe("squash");
  });

  it("shows original error when dialog is cancelled", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed on this repository."));

    const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
    const row = container.querySelector(".remoteOnlyRow");
    expect(row).not.toBeNull();
    fireEvent.click(row!);
    const mergeBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    ) as HTMLButtonElement | undefined;
    expect(mergeBtn).not.toBeUndefined();
    fireEvent.click(mergeBtn!);

    await waitFor(() => expect(container.textContent).toContain("squash"));

    const cancelBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.toLowerCase().includes("cancel"),
    );
    expect(cancelBtn).not.toBeNull();
    fireEvent.click(cancelBtn!);

    await waitFor(() => {
      expect(container.textContent).toContain("405");
    });
  });
});

describe("RemoteOnlyPrPopover — merge button label reflects effective method", () => {
  const makePr = (overrides = {}): BranchPrStatus => ({
    ...mergeablePr, ...overrides,
  });

  const renderAndExpand = (pr: BranchPrStatus) => {
    repoSettingsStore.getOrCreate("/repo", "Repo");
    repoSettingsStore.update("/repo", { prMergeStrategy: null });
    const { container } = render(() => (
      <RemoteOnlyPrPopover prs={[pr]} repoPath="/repo" onClose={vi.fn()} onCheckout={vi.fn()} />
    ));
    const row = container.querySelector(".remoteOnlyRow")!;
    fireEvent.click(row);
    return container;
  };

  it("shows Merge when merge commits are allowed", () => {
    const container = renderAndExpand(makePr({ merge_commit_allowed: true }));
    const btn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    );
    expect(btn?.textContent).toBe("Merge");
  });

  it("shows Squash & Merge when only squash is allowed", () => {
    const container = renderAndExpand(makePr({ merge_commit_allowed: false, squash_merge_allowed: true, rebase_merge_allowed: false }));
    const btn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    );
    expect(btn?.textContent).toBe("Squash & Merge");
  });

  it("shows Rebase & Merge when only rebase is allowed", () => {
    const container = renderAndExpand(makePr({ merge_commit_allowed: false, squash_merge_allowed: false, rebase_merge_allowed: true }));
    const btn = Array.from(container.querySelectorAll("button")).find(
      (b) => ["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
    );
    expect(btn?.textContent).toBe("Rebase & Merge");
  });
});
