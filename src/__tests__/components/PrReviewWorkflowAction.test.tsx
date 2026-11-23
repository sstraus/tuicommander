import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { PrReviewWorkflowAction } from "../../components/PrReviewWorkflow/PrReviewWorkflowAction";
import { invoke } from "../../invoke";
import { workflowsUi } from "../../stores/workflowsUi";

const URL = "https://github.com/o/r/pull/7";
const target = (id: string, name: string) => ({
	binding_id: id,
	enabled: true,
	publication: { project: "/repo", id: `${id}-pub`, revision: 1 },
	name,
});

function backend(targets: unknown[], start: () => Promise<unknown> = async () => ({})) {
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action?: { action: string } } | undefined)?.action;
		if (action?.action === "pr_review_targets") return { type: "pr_review_targets", value: targets };
		if (action?.action === "start_pr_review") return start();
		return undefined;
	});
}

const button = () => screen.getByRole("button", { name: "Review with workflow" }) as HTMLButtonElement;

describe("PrReviewWorkflowAction", () => {
	beforeEach(() => vi.mocked(invoke).mockReset());

	it("is disabled with the reason when the project has no PR review workflow", async () => {
		// Catches: an enabled button that can only fail, or a disabled one with no explanation.
		backend([]);
		render(() => <PrReviewWorkflowAction project="/repo" pullRequestUrl={URL} />);
		await screen.findByText(/No PR review workflow/);
		expect(button().disabled).toBe(true);
	});

	it("starts the single workflow and opens the started run", async () => {
		// Catches: the start omitting the PR URL or binding, and Open run losing the run id.
		backend([target("b1", "PR review")], async () => ({
			type: "inspection",
			value: { snapshot: { run: { id: "run-1234567890" }, status: { type: "running" } } },
		}));
		render(() => <PrReviewWorkflowAction project="/repo" pullRequestUrl={URL} />);
		await waitFor(() => expect(button().disabled).toBe(false));
		fireEvent.click(button());
		await screen.findByText(/run-1234 · running/);
		expect(invoke).toHaveBeenCalledWith("workflow_neutral_action", {
			project: "/repo",
			action: { action: "start_pr_review", pull_request: URL, binding: "b1" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Open run" }));
		expect(workflowsUi.project()).toBe("/repo");
		expect(workflowsUi.takeRun()).toBe("run-1234567890");
		workflowsUi.close();
	});

	it("requires a choice when several workflows review the repository", async () => {
		// Catches: the UI guessing the first binding when the backend would refuse the ambiguity.
		backend([target("b1", "Fast"), target("b2", "Thorough")]);
		render(() => <PrReviewWorkflowAction project="/repo" pullRequestUrl={URL} />);
		const select = (await screen.findByLabelText("PR review workflow")) as HTMLSelectElement;
		expect(button().disabled).toBe(true);
		fireEvent.change(select, { target: { value: "b2" } });
		await waitFor(() => expect(button().disabled).toBe(false));
	});

	it("shows a backend refusal inline", async () => {
		// Catches: a refused start (closed PR, wrong repository) failing silently.
		backend([target("b1", "PR review")], async () => {
			throw new Error("pull request is not open: #7");
		});
		render(() => <PrReviewWorkflowAction project="/repo" pullRequestUrl={URL} />);
		await waitFor(() => expect(button().disabled).toBe(false));
		fireEvent.click(button());
		expect((await screen.findByRole("alert")).textContent).toContain("pull request is not open");
	});
});
