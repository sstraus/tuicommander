import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { GithubOpsDashboard } from "../../components/GithubOpsDashboard/GithubOpsDashboard";
import { githubOpsStore } from "../../stores/githubOps";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

const repoPath = "/geometry-dashboard";
const proposal = {
	title: "Improve coverage",
	summary: "Cover the boundary",
	rationale: "Regression protection",
	issue_title: "Add coverage",
	issue_body: "Cover the boundary",
	labels: ["testing"],
	impact: "medium",
	effort: "small",
};
beforeEach(() => {
	vi.clearAllMocks();
	mockInvoke.mockResolvedValue(undefined);
	githubOpsStore.handleEvent("proposals-ready", { repo_path: repoPath, payload: { proposals: [proposal] } });
});
afterEach(cleanup);

it.each(["refactor", "testing", "perf"])(
	"catches %s scan losing its focus or pending disabled state",
	async (focus) => {
		let settle!: () => void;
		mockInvoke.mockImplementation((command) =>
			command === "run_improvement_scan"
				? new Promise<void>((resolve) => {
						settle = resolve;
					})
				: Promise.resolve(undefined),
		);
		const view = render(() => <GithubOpsDashboard repoPath={repoPath} />);
		fireEvent.click(view.getByRole("button", { name: focus }));
		expect(mockInvoke).toHaveBeenCalledWith("run_improvement_scan", { repoPath, focus });
		for (const name of ["refactor", "testing", "perf"]) expect(view.getByRole("button", { name })).toBeDisabled();
		settle();
		await waitFor(() => expect(view.getByRole("button", { name: focus })).not.toBeDisabled());
	},
);

it("catches issue creation dropping its proposal or allowing a duplicate pending submission", async () => {
	let settle!: () => void;
	mockInvoke.mockImplementation((command) =>
		command === "create_issue_from_proposal"
			? new Promise<void>((resolve) => {
					settle = resolve;
				})
			: Promise.resolve(undefined),
	);
	const view = render(() => <GithubOpsDashboard repoPath={repoPath} />);
	fireEvent.click(view.getByRole("button", { name: "Create issue" }));
	expect(mockInvoke).toHaveBeenCalledWith("create_issue_from_proposal", { repoPath, proposal });
	expect(view.getByRole("button", { name: "Creating" })).toBeDisabled();
	settle();
	await waitFor(() => expect(view.getByRole("button", { name: "Create issue" })).not.toBeDisabled());
});

it("catches a failed issue submission hiding the error or leaving retry disabled", async () => {
	mockInvoke.mockRejectedValueOnce(new Error("gh: Not Found"));
	const view = render(() => <GithubOpsDashboard repoPath={repoPath} />);
	fireEvent.click(view.getByRole("button", { name: "Create issue" }));
	await waitFor(() => expect(view.getByText("gh: Not Found")).toBeInTheDocument());
	expect(view.getByRole("button", { name: "Create issue" })).not.toBeDisabled();
});
