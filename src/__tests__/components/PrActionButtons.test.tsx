import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { failedRunIds, type PrAction, PrActionButtons } from "../../components/PrDetailPopover/PrActionButtons";

const ALL_ACTIONS: readonly PrAction[] = ["rerun", "auto-merge", "request-changes"];

import { appLogger } from "../../stores/appLogger";
import type { BranchPrStatus, CheckDetail } from "../../types";
import { mockInvoke } from "../mocks/tauri";

const HEAD = "a".repeat(40);
const pr = (over: Partial<BranchPrStatus> = {}) =>
	({
		number: 42,
		state: "OPEN",
		is_draft: false,
		head_ref_oid: HEAD,
		merge_commit_allowed: false,
		squash_merge_allowed: true,
		rebase_merge_allowed: false,
		...over,
	}) as BranchPrStatus;
const failed = (run: number, job = 1): CheckDetail => ({
	context: `job ${job}`,
	state: "failure",
	html_url: `https://github.com/acme/widget/actions/runs/${run}/job/${job}`,
});

/** Backend stub: get_pr_action_state answers `state`; every other command is recorded and succeeds. */
function stubBackend(state: Record<string, unknown>) {
	mockInvoke.mockImplementation(async (cmd: string) => (cmd === "get_pr_action_state" ? state : { accepted: true }));
}
const writes = () => mockInvoke.mock.calls.filter(([cmd]) => cmd !== "get_pr_action_state");

describe("PrActionButtons", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.restoreAllMocks();
	});

	it("derives one run id per failed Actions run and ignores other providers", () => {
		const circle: CheckDetail = { context: "ci", state: "failure", html_url: "https://circleci.com/gh/a/b/9" };
		const passing: CheckDetail = { ...failed(7), state: "success" };
		expect(failedRunIds([failed(5, 1), failed(5, 2), failed(6), circle, passing])).toEqual([5, 6]);
	});

	// Catches: the button re-running something other than the failed runs of this PR's checks.
	it("rerun calls rerun_failed_pr_jobs once per failed run and shows the outcome", async () => {
		stubBackend({});
		render(() => (
			<PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[failed(5, 1), failed(5, 2), failed(6)]} />
		));
		fireEvent.click(await screen.findByRole("button", { name: "Re-run failed jobs" }));
		await screen.findByRole("status");
		expect(writes()).toEqual([
			["rerun_failed_pr_jobs", { repoPath: "/r", runId: 5 }],
			["rerun_failed_pr_jobs", { repoPath: "/r", runId: 6 }],
		]);
	});

	// Catches: a rerun button offered when no failed Actions run can be targeted.
	it("offers no rerun without a failed Actions run", () => {
		stubBackend({});
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		expect(screen.queryByRole("button", { name: "Re-run failed jobs" })).toBeNull();
	});

	// Catches: "Enable" still shown while auto-merge is already on, or the head not pinned.
	it("auto-merge button follows the backend state and pins the reviewed head", async () => {
		stubBackend({ autoMergeRequest: null, mergeQueueEntry: null, queueRequired: false });
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		const enable = await screen.findByRole("button", { name: "Enable auto-merge" });
		await waitFor(() => expect(enable).not.toBeDisabled());
		stubBackend({ autoMergeRequest: { enabledAt: "now" }, mergeQueueEntry: null, queueRequired: false });
		fireEvent.click(enable);
		expect(await screen.findByRole("button", { name: "Disable auto-merge" })).toBeInTheDocument();
		expect(writes()).toEqual([
			[
				"set_pr_auto_merge",
				{ repoPath: "/r", prNumber: 42, enabled: true, mergeMethod: "squash", expectedHeadSha: HEAD },
			],
		]);
	});

	// Catches: queue button on a repo without a queue, or "Add" shown for a PR already queued.
	it("queue button appears only when the queue is required and toggles by entry", async () => {
		stubBackend({ queueRequired: false });
		const { unmount } = render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		await screen.findByRole("button", { name: "Enable auto-merge" });
		expect(screen.queryByRole("button", { name: /merge queue/ })).toBeNull();
		unmount();

		stubBackend({ queueRequired: true, mergeQueueEntry: null });
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		fireEvent.click(await screen.findByRole("button", { name: "Add to merge queue" }));
		await screen.findByRole("status");
		expect(writes()).toEqual([
			["merge_or_enqueue_pr", { repoPath: "/r", prNumber: 42, mergeMethod: "squash", expectedHeadSha: HEAD }],
		]);
	});

	it("a queued PR offers removal through set_pr_merge_queue", async () => {
		stubBackend({ queueRequired: true, mergeQueueEntry: { id: "q1" } });
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		fireEvent.click(await screen.findByRole("button", { name: "Remove from merge queue" }));
		await screen.findByRole("status");
		expect(writes()).toEqual([
			["set_pr_merge_queue", { repoPath: "/r", prNumber: 42, enqueued: false, expectedHeadSha: HEAD }],
		]);
	});

	// Catches: an empty review body sent to GitHub, which answers 422.
	it("request changes needs a non-blank body and sends REQUEST_CHANGES", async () => {
		stubBackend({});
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		fireEvent.click(await screen.findByRole("button", { name: "Request changes" }));
		const submit = screen.getByRole("button", { name: "Submit" });
		const box = screen.getByLabelText("Review summary");
		fireEvent.input(box, { target: { value: "   " } });
		expect(submit).toBeDisabled();
		fireEvent.submit(box.closest("form") as HTMLFormElement);
		expect(writes()).toEqual([]);
		fireEvent.input(box, { target: { value: " fix the race " } });
		fireEvent.click(submit);
		await screen.findByRole("status");
		expect(writes()).toEqual([
			[
				"post_pr_review",
				{ repoPath: "/r", prNumber: 42, body: "fix the race", event: "REQUEST_CHANGES", comments: [] },
			],
		]);
	});

	// Catches: a rejected write surfacing as silent success.
	it("shows the backend error and no success notice when a write fails", async () => {
		mockInvoke.mockImplementation(async (cmd: string) => {
			if (cmd !== "set_pr_auto_merge") return {};
			throw new Error("PR head changed: Refresh and review before merging");
		});
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr()} checks={[]} />);
		const enable = await screen.findByRole("button", { name: "Enable auto-merge" });
		await waitFor(() => expect(enable).not.toBeDisabled());
		fireEvent.click(enable);
		expect(await screen.findByRole("alert")).toHaveTextContent("PR head changed");
		expect(screen.queryByRole("status")).toBeNull();
		expect(warn).toHaveBeenCalledWith("github", "PR action failed", expect.anything());
	});

	// Catches: the panel's next-action block repeating every PR action instead of only the ones
	// its state asks for (Request changes next to Merge, Auto-merge next to a review).
	it("offers only the actions the caller asks for", async () => {
		stubBackend({ autoMergeRequest: null, mergeQueueEntry: null, queueRequired: false });
		render(() => <PrActionButtons actions={["request-changes"]} repoPath="/r" pr={pr()} checks={[failed(5)]} />);
		expect(await screen.findByRole("button", { name: "Request changes" })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: /auto-merge/ })).toBeNull();
		expect(screen.queryByRole("button", { name: "Re-run failed jobs" })).toBeNull();
	});

	// Catches: merge-type actions offered on a draft, and any action on a closed PR.
	it("hides merge actions on a draft and everything on a merged PR", () => {
		stubBackend({ queueRequired: true });
		const draft = render(() => (
			<PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr({ is_draft: true })} checks={[]} />
		));
		expect(screen.queryByRole("button", { name: /auto-merge|merge queue/ })).toBeNull();
		expect(screen.getByRole("button", { name: "Request changes" })).toBeInTheDocument();
		draft.unmount();
		render(() => (
			<PrActionButtons actions={ALL_ACTIONS} repoPath="/r" pr={pr({ state: "MERGED" })} checks={[failed(5)]} />
		));
		expect(screen.queryByRole("button")).toBeNull();
	});
});

/** Serialized shape of Rust `PrActionState` (`#[serde(rename_all = "camelCase")]`, github_pr_actions.rs). */
const wire = (over: Record<string, unknown>) => ({
	id: "PR_1",
	headSha: HEAD,
	baseRefName: "main",
	state: "OPEN",
	isDraft: false,
	autoMergeRequest: null,
	mergeQueueEntry: null,
	queueRequired: false,
	...over,
});

describe("PrActionButtons against the real backend wire shape", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("shows Disable auto-merge when the backend reports autoMergeRequest — catches: reading snake_case keys the backend never sends", async () => {
		stubBackend(wire({ autoMergeRequest: { enabledAt: "2026-10-11" } }));
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/repo" pr={pr()} checks={[]} />);
		expect(await screen.findByText("Disable auto-merge")).toBeTruthy();
	});

	it("offers the merge queue when the backend reports queueRequired — catches: queue button unreachable", async () => {
		stubBackend(wire({ queueRequired: true }));
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/repo" pr={pr()} checks={[]} />);
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_pr_action_state", expect.anything()));
		expect(await screen.findByText("Add to merge queue")).toBeTruthy();
	});

	it("offers Remove from merge queue when the backend reports mergeQueueEntry — catches: cannot leave a queue", async () => {
		stubBackend(wire({ mergeQueueEntry: { position: 1 } }));
		render(() => <PrActionButtons actions={ALL_ACTIONS} repoPath="/repo" pr={pr()} checks={[]} />);
		expect(await screen.findByText("Remove from merge queue")).toBeTruthy();
	});
});
