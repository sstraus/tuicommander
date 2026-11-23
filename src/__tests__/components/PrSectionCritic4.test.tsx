import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchPrStatus } from "../../types";
import { menuItem, rowsOfferingMenuItem, selectMenuItem } from "../helpers/panelToolbarMenu";
import { mockInvoke } from "../mocks/tauri";

const { pollRepo } = vi.hoisted(() => ({ pollRepo: vi.fn() }));

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
vi.mock("../../utils/clipboard", () => ({ writeClipboard: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../components/PrDetailPopover/PrDetailContent", () => ({
	// The real panel shows Merge as the next action; this stand-in keeps the caller's wiring testable.
	PrDetailContent: (p: { children?: unknown; onMerge?: () => void; merging?: boolean }) => (
		<>
			{p.onMerge && (
				<button type="button" disabled={p.merging} onClick={() => p.onMerge?.()}>
					{p.merging ? "Merging..." : "Merge"}
				</button>
			)}
			{p.children}
		</>
	),
}));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));

import { PrSection } from "../../components/Sidebar/PrSection";

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
		created_at: new Date().toISOString(),
		...o,
	}) as BranchPrStatus;

const renderSection = (prs: () => BranchPrStatus[], expandedKey: string) =>
	render(() => (
		<PrSection
			title="PRs"
			prs={prs()}
			repoPath="/repo"
			collapsed={false}
			onToggleCollapsed={vi.fn()}
			expandedKey={expandedKey}
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
const buttons = (container: HTMLElement, label: string) =>
	Array.from(container.querySelectorAll("button")).filter(
		(b) => b.textContent?.trim() === label,
	) as HTMLButtonElement[];

describe("PrSection critic r4", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});

	it("an accepted update on one PR does not hide Update branch on another PR with the same head SHA", async () => {
		// Catches: the hold keyed on the head SHA alone — two PRs from one branch (to main and to a
		// release branch) share a head, so updating one makes the other's button vanish for 60 s.
		const a = pr({ number: 12 });
		const b = pr({ number: 13, url: "https://github.com/acme/api/pull/13" });
		const { container } = renderSection(() => [a, b], a.branch);
		expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([0, 1]);
		selectMenuItem(container, UPDATE_BRANCH, 0);
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("update_pr_branch", expect.anything()));
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([1]));
		expect(container.textContent).toContain("#13");
		// the remaining item must belong to PR 13
		selectMenuItem(container, UPDATE_BRANCH, 1);
		expect(mockInvoke).toHaveBeenLastCalledWith("update_pr_branch", expect.objectContaining({ prNumber: 13 }));
	});

	it("shows Update branch again at once when a poll brings a new head while the hold is active", async () => {
		// Catches: the hold keyed on the PR number (or never released on head change), so a base
		// that moved again leaves the row without its button for the rest of the interval.
		const [prs, setPrs] = createSignal([pr()]);
		const { container } = renderSection(prs, "feat/login");
		selectMenuItem(container, UPDATE_BRANCH);
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([]));
		setPrs([pr({ head_ref_oid: "headsha2" })]);
		await vi.waitFor(() => expect(rowsOfferingMenuItem(container, UPDATE_BRANCH)).toEqual([0]));
	});

	it("a rejected update request keeps the button, enabled, and starts no hold", async () => {
		// Catches: the hold armed before the request outcome, hiding the button after a failure.
		mockInvoke.mockRejectedValueOnce("422 head moved");
		const { container } = renderSection(() => [pr()], "feat/login");
		selectMenuItem(container, UPDATE_BRANCH);
		await vi.waitFor(() => expect(container.textContent).toContain("422 head moved"));
		const item = menuItem(container, UPDATE_BRANCH);
		expect(item).toBeDefined();
		expect(item?.getAttribute("aria-disabled")).toBeNull();
		expect(pollRepo).not.toHaveBeenCalled();
	});

	it("a merge in flight on one PR stays locked when another PR's merge finishes", async () => {
		// Catches: a single `mergingPr` slot — the first merge finishing clears it and re-enables
		// the Merge button of the PR whose merge is still running (a double merge request).
		const resolvers = new Map<number, () => void>();
		mockInvoke.mockImplementation((cmd: string, args?: { prNumber?: number }) => {
			if (cmd === "merge_pr_via_github") {
				return new Promise<void>((r) => resolvers.set(args?.prNumber ?? -1, () => r()));
			}
			return Promise.resolve(undefined);
		});
		const base = { review_decision: "APPROVED" as const, merge_state_status: "CLEAN" as const };
		const a = pr({ number: 12, ...base });
		const b = pr({ number: 13, url: "https://github.com/acme/api/pull/13", ...base });
		const { container } = renderSection(() => [a, b], a.branch);
		const merges = buttons(container, "Merge");
		expect(merges).toHaveLength(2);
		fireEvent.click(merges[0]);
		fireEvent.click(merges[1]);
		await vi.waitFor(() => expect(resolvers.size).toBe(2));
		resolvers.get(12)?.();
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("run_git_command", expect.anything()));
		const second = Array.from(container.querySelectorAll("button")).filter((x) =>
			/^Merg(e|ing\.\.\.)$/.test(x.textContent?.trim() ?? ""),
		);
		expect(second.some((x) => x.textContent?.trim() === "Merging..." && x.disabled)).toBe(true);
		resolvers.get(13)?.();
		await vi.waitFor(() => expect(buttons(container, "Merging...")).toHaveLength(0));
	});
});
