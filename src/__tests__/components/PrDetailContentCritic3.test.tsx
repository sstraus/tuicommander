import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockGithubStore = vi.hoisted(() => ({
	getBranchPrData: vi.fn(),
	getCheckSummary: vi.fn(() => null),
	getCheckDetails: vi.fn(() => []),
	loadCheckDetails: vi.fn(() => Promise.resolve()),
	state: { viewerLogin: null as string | null },
}));
const mockRpc = vi.hoisted(() => vi.fn(() => Promise.resolve({ bot: 0, human: 0 })));

vi.mock("../../stores/github", () => ({ githubStore: mockGithubStore }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { PrDetailContent } from "../../components/PrDetailPopover/PrDetailContent";

const conflicting = {
	number: 7,
	head_ref_oid: "b".repeat(40),
	state: "OPEN",
	branch: "feature",
	base_ref_name: "main",
	author: "boss",
	commits: 1,
	additions: 1,
	deletions: 1,
	labels: [],
	mergeable: "CONFLICTING",
	conflict_state: "conflicting",
	merge_state_status: "DIRTY",
	merge_state_label: { label: "Conflicts", css_class: "conflicting" },
	review_state_label: null,
	is_draft: false,
	checks: { passed: 0, failed: 0, pending: 0, total: 0 },
	unresolved_threads: 0,
	unresolved_threads_truncated: false,
	created_at: "2026-01-01T00:00:00Z",
	updated_at: "2026-01-01T00:00:00Z",
};

describe("PrDetailContent (critic round 3)", () => {
	beforeEach(() => vi.clearAllMocks());

	// Catches: the conflict-assist action keyed on the readiness verdict, where "draft" outranks
	// "conflict" — a draft PR with merge conflicts loses its Resolve conflicts button.
	it("a conflicting DRAFT PR still offers Resolve conflicts", () => {
		mockGithubStore.getBranchPrData.mockReturnValue({ ...conflicting, is_draft: true });
		const { queryByText } = render(() => (
			<PrDetailContent repoPath="/repo-c3-draft" branch="feature" onConflictAssist={() => {}} />
		));
		expect(queryByText("Resolve conflicts")).toBeTruthy();
	});

	// Control: the same PR not in draft shows it (guards the test above against a broken fixture).
	it("a conflicting non-draft PR offers Resolve conflicts", () => {
		mockGithubStore.getBranchPrData.mockReturnValue(conflicting);
		const { queryByText } = render(() => (
			<PrDetailContent repoPath="/repo-c3-open" branch="feature" onConflictAssist={() => {}} />
		));
		expect(queryByText("Resolve conflicts")).toBeTruthy();
	});

	// Catches: a merged PR rendering an empty status row (all pills hidden for terminal states)
	// that still takes layout space.
	it("a merged PR renders no empty status row", () => {
		mockGithubStore.getBranchPrData.mockReturnValue({ ...conflicting, state: "MERGED", conflict_state: "clear" });
		const { container } = render(() => <PrDetailContent repoPath="/repo-c3-merged" branch="feature" />);
		const rows = [...container.querySelectorAll("[class*='statusRow']")];
		expect(rows.filter((r) => r.children.length === 0)).toHaveLength(0);
		expect(container.querySelector("[data-readiness]")).toBeNull();
	});
});
