import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { blankDefinition } from "../../components/WorkflowDesigner/neutralClient";
import type { Inspection, RunView } from "../../components/WorkflowDesigner/neutralInspectionTypes";
import { invoke } from "../../invoke";

const handlers: Record<string, (payload: unknown) => void> = {};
vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async (callbacks) => {
		Object.assign(handlers, callbacks);
		return () => {};
	}),
}));
const run: RunView = {
	id: "run-retained",
	name: "Deploy",
	publication: { project: "/repo", id: "flow", revision: 3 },
	status: { type: "running" },
	current_node: "end",
	waiting_on_approval: true,
	started_ms: 100,
	updated_ms: 200,
};
let inspection: Inspection;
beforeEach(() => {
	inspection = {
		snapshot: { node: "end", status: { type: "running" }, sequence: 10 },
		events: [],
		progress: {
			name: "Deploy",
			publication: {
				id: "flow",
				project: "/repo",
				revision: 3,
				format_version: 1,
				definition: blankDefinition(),
				editor: { labels: { end: "Release" }, presets: {} },
				pins: { pins: [] },
				subflow_pins: [],
			},
			node_states: { start: "completed", end: "waiting" },
			waiting_on_approval: true,
			approvals: [],
			started_ms: 100,
			updated_ms: 200,
		},
	};
	vi.mocked(invoke)
		.mockReset()
		.mockImplementation(async (command, args) => {
			if (command !== "workflow_neutral_action") return [];
			switch ((args?.action as { action: string } | undefined)?.action) {
				case "catalog":
					return { type: "catalog", value: { activities: [], metanodes: [] } };
				case "templates":
					return { type: "templates", value: [] };
				case "list_drafts":
					return { type: "drafts", value: [] };
				case "list_runs":
					return { type: "runs", value: [run] };
				case "inspect":
					return { type: "inspection", value: structuredClone(inspection) };
				default:
					throw new Error("Unexpected action");
			}
		});
});

// Catches: users must type run ids, or inspection renders an editable draft instead of the retained graph.
it("opens a listed run and maps live backend node states onto its pinned graph", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	fireEvent.click(screen.getByRole("tab", { name: "Preview & runs" }));
	const row = await screen.findByRole("button", { name: /Deploy · revision 3/ });
	expect(row).toHaveTextContent("Waiting for approval");
	fireEvent.click(row);
	const graph = await screen.findByRole("img", { name: "Live run node states" });
	expect(within(graph).getByText("Start").parentElement).toHaveAttribute("data-state", "completed");
	expect(within(graph).getByText("Release").parentElement).toHaveAttribute("data-state", "waiting");
	expect(screen.queryByRole("textbox", { name: "Run ID" })).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("tab", { name: "Design" }));
	fireEvent.click(screen.getByRole("tab", { name: "Preview & runs" }));
	expect(screen.getByRole("img", { name: "Live run node states" })).toBe(graph);
	if (!inspection.progress) throw new Error("Missing progress fixture");
	inspection.progress.node_states.end = "resource_wait";
	inspection.progress.resource_waits = [
		{ resource: "repo:/repo", owner_run: "run-owner", owner_effect: "effect-owner" },
	];
	handlers["workflow-run-changed"]({ repo_path: "/repo", payload: { runId: "run-retained" } });
	await screen.findByText("Waiting on resource repo:/repo, held by run run-owner");
	await waitFor(() =>
		expect(within(graph).getByText("Release").parentElement).toHaveAttribute("data-state", "resource_wait"),
	);
	inspection.progress.node_states.end = "failed";
	handlers["workflow-run-changed"]({ repo_path: "/other", payload: { runId: "run-retained" } });
	expect(within(graph).getByText("Release").parentElement).toHaveAttribute("data-state", "resource_wait");
	handlers["workflow-run-changed"]({ repo_path: "/repo", payload: { runId: "run-retained" } });
	await waitFor(() => expect(within(graph).getByText("Release").parentElement).toHaveAttribute("data-state", "failed"));
});
