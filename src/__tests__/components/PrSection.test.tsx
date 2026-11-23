import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchPrStatus } from "../../types";
import { menuItem, rowsOfferingMenuItem, selectMenuItem } from "../helpers/panelToolbarMenu";
import { mockInvoke } from "../mocks/tauri";

const { pollRepo, writeClipboard } = vi.hoisted(() => ({
	pollRepo: vi.fn(),
	writeClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../stores/github", () => ({
	githubStore: { state: { viewerLogin: "me" }, pollRepo, getCheckSummary: () => null },
}));
vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: { getEffectiveField: () => undefined, getOrCreate: vi.fn(), update: vi.fn() },
}));
vi.mock("../../stores/repoDefaults", () => ({ repoDefaultsStore: { state: { prMergeStrategy: "merge" } } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { get: () => undefined, workspaceIdOnBranch: () => null, state: { repositories: {} } },
}));
vi.mock("../../stores/mdTabs", () => ({ mdTabsStore: { addPrDiff: vi.fn() } }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard }));
vi.mock("../../components/PrDetailPopover/PrDetailContent", () => ({
	PrDetailContent: (p: { children?: unknown }) => p.children,
}));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));

import { PrSection, UPDATE_BRANCH_HOLD_MS } from "../../components/Sidebar/PrSection";

const pr = (o: Partial<BranchPrStatus> = {}): BranchPrStatus =>
	({
		number: 12,
		title: "Fix login",
		branch: "feat/login",
		state: "OPEN",
		url: "https://github.com/acme/api/pull/12",
		is_draft: false,
		merge_state_status: "BEHIND",
		head_ref_oid: "headsha1",
		review_decision: "",
		checks: { passed: 0, failed: 0, pending: 0, total: 0 },
		created_at: new Date(Date.now() - 100 * 86_400_000).toISOString(),
		...o,
	}) as BranchPrStatus;

const renderSection = (p: BranchPrStatus) =>
	render(() => (
		<PrSection
			title="PRs"
			prs={[p]}
			repoPath="/repo"
			collapsed={false}
			onToggleCollapsed={vi.fn()}
			expandedKey={p.branch}
			onToggleExpanded={vi.fn()}
			activeKey={null}
			dismissedCount={0}
			onDismiss={vi.fn()}
			onShowDismissed={vi.fn()}
			onCheckout={vi.fn()}
			onMerged={vi.fn()}
		/>
	));

const UPDATE_BRANCH = "Merge the base branch into this PR branch";

describe("PrSection row actions", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		// jsdom ships no window.confirm; define one so it can be stubbed per test.
		window.confirm = vi.fn();
	});
	afterEach(() => vi.restoreAllMocks());

	it("update branch is pinned to the head the row showed", async () => {
		// Catches: updating against a head that moved after the user looked.
		const { container } = renderSection(pr());
		selectMenuItem(container, UPDATE_BRANCH);
		expect(mockInvoke).toHaveBeenCalledWith("update_pr_branch", {
			repoPath: "/repo",
			prNumber: 12,
			expectedHeadSha: "headsha1",
		});
	});

	it("close asks for confirmation and closes nothing when declined", () => {
		// Catches: close without confirmation.
		const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
		const { container } = renderSection(pr());
		selectMenuItem(container, "Close this pull request without merging");
		expect(confirm).toHaveBeenCalledOnce();
		expect(mockInvoke).not.toHaveBeenCalledWith("close_pr", expect.anything());
	});

	it("close sends only this PR once confirmed", () => {
		vi.spyOn(window, "confirm").mockReturnValue(true);
		const { container } = renderSection(pr());
		selectMenuItem(container, "Close this pull request without merging");
		expect(mockInvoke).toHaveBeenCalledWith("close_pr", { repoPath: "/repo", prNumber: 12 });
	});

	it("hides the PR from the list through the row menu", () => {
		// Catches: Hide from list dismissing another PR, or no way left to dismiss once the × is gone.
		const onDismiss = vi.fn();
		const { container } = render(() => (
			<PrSection
				title="PRs"
				prs={[pr()]}
				repoPath="/repo"
				collapsed={false}
				onToggleCollapsed={vi.fn()}
				expandedKey="feat/login"
				onToggleExpanded={vi.fn()}
				activeKey={null}
				dismissedCount={0}
				onDismiss={onDismiss}
				onShowDismissed={vi.fn()}
				onCheckout={vi.fn()}
				onMerged={vi.fn()}
			/>
		));
		selectMenuItem(container, "Hide from list");
		expect(onDismiss).toHaveBeenCalledWith(12);
	});

	it("copy ref writes owner/repo#N", () => {
		// Catches: copying the URL or the bare number.
		const { container } = renderSection(pr());
		selectMenuItem(container, "Copy owner/repo#number");
		expect(writeClipboard).toHaveBeenCalledWith("acme/api#12");
	});

	it("shows the stale-age marker and hides Update branch for a non-BEHIND PR", () => {
		const { container } = renderSection(pr({ merge_state_status: "CLEAN" }));
		expect(container.querySelector(".ghAgeMarker")?.textContent).toBe("3m");
		expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([]);
	});

	it("keeps Update branch out of reach after the request was accepted, until the head changes", async () => {
		// Catches: 202 clearing the busy flag while the row still reads BEHIND, so a second click
		// is sent against a stale head and answered with a misleading "PR head changed".
		const { container } = renderSection(pr());
		selectMenuItem(container, UPDATE_BRANCH);
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([]));
		expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "update_pr_branch")).toHaveLength(1);
	});

	it("shows a failed row action only on its own row and keeps it when another row opens", async () => {
		// Catches: one shared error slot printing PR #12's failure under every expanded PR.
		mockInvoke.mockRejectedValueOnce(new Error("boom"));
		const first = pr();
		const other = pr({ number: 13, branch: "feat/other", head_ref_oid: "headsha2", merge_state_status: "CLEAN" });
		const [key, setKey] = createSignal<string | null>(first.branch);
		const { container } = render(() => (
			<PrSection
				title="PRs"
				prs={[first, other]}
				repoPath="/repo"
				collapsed={false}
				onToggleCollapsed={vi.fn()}
				expandedKey={key()}
				onToggleExpanded={vi.fn()}
				activeKey={null}
				dismissedCount={0}
				onDismiss={vi.fn()}
				onShowDismissed={vi.fn()}
				onCheckout={vi.fn()}
				onMerged={vi.fn()}
			/>
		));
		selectMenuItem(container, UPDATE_BRANCH);
		await vi.waitFor(() => expect(container.textContent).toContain("boom"));
		setKey(other.branch);
		await vi.waitFor(() => expect(container.textContent).not.toContain("boom"));
		setKey(first.branch);
		await vi.waitFor(() => expect(container.textContent).toContain("boom"));
	});

	it("shows a fallback text when the rejection message is empty", async () => {
		// Catches: an empty rejection rendering an empty error line.
		mockInvoke.mockRejectedValueOnce("");
		const { container } = renderSection(pr());
		selectMenuItem(container, UPDATE_BRANCH);
		await vi.waitFor(() => expect(container.textContent).toContain("Update branch failed"));
	});

	it("brings Update branch back after one poll interval when the head did not change", async () => {
		// Catches: a silently failed async update leaving the button hidden until remount.
		vi.useFakeTimers();
		try {
			const { container } = renderSection(pr());
			selectMenuItem(container, UPDATE_BRANCH);
			await vi.advanceTimersByTimeAsync(0);
			expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([]);
			await vi.advanceTimersByTimeAsync(UPDATE_BRANCH_HOLD_MS);
			expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([0]);
		} finally {
			vi.useRealTimers();
		}
	});

	it("keeps each PR busy on its own while two row actions are in flight", async () => {
		// Catches: one busy slot, so the first action finishing re-enables the second PR's buttons.
		let finishFirst: (v?: unknown) => void = () => {};
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((r) => {
					finishFirst = r;
				}),
		);
		let finishSecond!: () => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					finishSecond = resolve;
				}),
		);
		const first = pr();
		const second = pr({ number: 13, branch: "feat/other", head_ref_oid: "headsha2" });
		const [key, setKey] = createSignal<string | null>(first.branch);
		const { container } = render(() => (
			<PrSection
				title="PRs"
				prs={[first, second]}
				repoPath="/repo"
				collapsed={false}
				onToggleCollapsed={vi.fn()}
				expandedKey={key()}
				onToggleExpanded={vi.fn()}
				activeKey={null}
				dismissedCount={0}
				onDismiss={vi.fn()}
				onShowDismissed={vi.fn()}
				onCheckout={vi.fn()}
				onMerged={vi.fn()}
			/>
		));
		selectMenuItem(container, UPDATE_BRANCH);
		setKey(second.branch);
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([0]));
		selectMenuItem(container, UPDATE_BRANCH);
		finishFirst();
		await vi.waitFor(() => expect(menuItem(container, UPDATE_BRANCH)?.getAttribute("aria-disabled")).toBe("true"));
		finishSecond();
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([]));
	});
});
