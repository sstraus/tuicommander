import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchPrStatus } from "../../types";
import { selectMenuItem } from "../helpers/panelToolbarMenu";
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
	PrDetailContent: (p: { children?: unknown }) => p.children,
}));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));

import { PrSection } from "../../components/Sidebar/PrSection";

const pr = (o: Partial<BranchPrStatus> = {}): BranchPrStatus =>
	({
		number: 12,
		title: "Fix login",
		branch: "feat/a",
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

describe("PrSection critic r5", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});

	it("an action that fails while its row is collapsed is still shown when the row is expanded again", async () => {
		// Catches: the error map wiped on every expand change, so a failure that lands while the
		// user looks at another PR is lost with no trace in the UI (only the log).
		const a = pr({ number: 12, branch: "feat/a" });
		const b = pr({ number: 13, branch: "feat/b", url: "https://github.com/acme/api/pull/13" });
		const [expanded, setExpanded] = createSignal<string>("feat/a");
		let rejectUpdate: (e: unknown) => void = () => {};
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "update_pr_branch"
				? new Promise((_, reject) => {
						rejectUpdate = reject;
					})
				: Promise.resolve(undefined),
		);
		const { container } = render(() => (
			<PrSection
				title="PRs"
				prs={[a, b]}
				repoPath="/repo"
				collapsed={false}
				onToggleCollapsed={vi.fn()}
				expandedKey={expanded()}
				onToggleExpanded={vi.fn()}
				activeKey={null}
				dismissedCount={0}
				onDismiss={vi.fn()}
				onShowDismissed={vi.fn()}
				onCheckout={vi.fn()}
				onMerged={vi.fn()}
			/>
		));
		selectMenuItem(container, "Merge the base branch into this PR branch");
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("update_pr_branch", expect.anything()));
		setExpanded("feat/b");
		rejectUpdate("branch protection: boom-12");
		await new Promise((r) => setTimeout(r, 0));
		setExpanded("feat/a");
		await new Promise((r) => setTimeout(r, 0));
		expect(container.textContent).toContain("boom-12");
	});
});
