import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, bump } = vi.hoisted(() => ({ invokeMock: vi.fn(), bump: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: { bumpGitRevision: bump } }));

import { WorktreeDiagnosticRow } from "../../components/Sidebar/WorktreeDiagnosticRow";
import type { WorktreeDiagnostic } from "../../hooks/useRepository";

const diag = (over: Partial<WorktreeDiagnostic>): WorktreeDiagnostic => ({
	path: "/wt/smartmem-fix",
	branch: "fix",
	state: "missing",
	guidance: "Checkout unavailable.",
	checkout_present: true,
	...over,
});

describe("WorktreeDiagnosticRow", () => {
	beforeEach(() => {
		invokeMock.mockReset();
		bump.mockReset();
	});

	// Catches: repair sent with the wrong action/path, or the sidebar not refreshing afterwards.
	it("repairs a present checkout and refreshes the repo", async () => {
		invokeMock.mockResolvedValue(undefined);
		const view = render(() => <WorktreeDiagnosticRow repoPath="/repo" diagnostic={diag({})} />);
		expect(view.getByTestId("worktree-diagnostic")).toHaveTextContent("smartmem-fix");
		expect(view.getByTestId("worktree-diagnostic").textContent).not.toContain("/wt/");
		fireEvent.click(view.getByTestId("worktree-diagnostic-repair"));
		await waitFor(() => expect(bump).toHaveBeenCalledWith("/repo"));
		expect(invokeMock).toHaveBeenCalledWith("resolve_worktree_diagnostic_cmd", {
			repoPath: "/repo",
			worktreePath: "/wt/smartmem-fix",
			action: "repair",
		});
	});

	// Catches: prune firing on the first click, or offered while the folder still exists.
	it("prunes a missing folder only after a second confirming click", async () => {
		invokeMock.mockResolvedValue(undefined);
		const view = render(() => (
			<WorktreeDiagnosticRow repoPath="/repo" diagnostic={diag({ checkout_present: false })} />
		));
		expect(view.queryByTestId("worktree-diagnostic-repair")).toBeNull();
		fireEvent.click(view.getByTestId("worktree-diagnostic-prune"));
		expect(invokeMock).not.toHaveBeenCalled();
		fireEvent.click(view.getByTestId("worktree-diagnostic-prune"));
		await waitFor(() => expect(invokeMock).toHaveBeenCalledOnce());
		expect(invokeMock.mock.calls[0][1]).toMatchObject({ action: "prune" });
	});

	// Catches: a failed repair that is silent or leaves the row stuck busy.
	it("shows a failed repair once, compactly", async () => {
		invokeMock.mockRejectedValue("git worktree repair failed: boom");
		const view = render(() => <WorktreeDiagnosticRow repoPath="/repo" diagnostic={diag({})} />);
		fireEvent.click(view.getByTestId("worktree-diagnostic-repair"));
		await waitFor(() => expect(view.getByTestId("worktree-diagnostic-error")).toHaveTextContent("boom"));
		expect(bump).not.toHaveBeenCalled();
	});

	it("offers no action for a duplicate-branch diagnostic", () => {
		const view = render(() => (
			<WorktreeDiagnosticRow repoPath="/repo" diagnostic={diag({ state: "duplicate_branch" })} />
		));
		expect(view.container.querySelector("button")).toBeNull();
	});
});
