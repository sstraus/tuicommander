import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import type { ActivityDescriptor } from "../../components/WorkflowDesigner/catalogTypes";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { blankDefinition, type NeutralPublication } from "../../components/WorkflowDesigner/neutralClient";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});

const child: NeutralPublication = {
	editor: { labels: {}, presets: {} },
	format_version: 11,
	id: "child-id",
	project: "/repo",
	revision: 2,
	definition: blankDefinition(),
	pins: { pins: [] },
	subflow_pins: [],
};
// What the backend returns for save_subflow: it, not the Designer, computes the pin.
const registered = (version: number): ActivityDescriptor => ({
	id: { namespace: "project", name: "lint-test-r2", version },
	purpose: "Run published workflow child-id revision 2",
	handler: { name: "host.subflow", version: 1 },
	target: {
		type: "subflow",
		publication: { reference: { project: "/repo", id: "child-id", revision: 2 }, sha256: "abc" },
	},
	config: { fields: {} },
	inputs: { fields: {} },
	subject: null,
	outputs: { fields: {} },
	outcomes: ["completed"],
	required_capabilities: [],
	execution_policies: ["local"],
});
let activities: ActivityDescriptor[] = [];
const calls: Record<string, unknown>[] = [];
beforeEach(() => {
	calls.length = 0;
	activities = [];
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		calls.push(action);
		switch (action.action) {
			case "catalog":
				return { type: "catalog", value: { activities, metanodes: [child] } };
			case "list_drafts":
				return {
					type: "drafts",
					value: [
						{
							id: "child-id",
							project: "/repo",
							name: "Lint & test",
							revision: 2,
							latest_published_revision: 2,
							definition: blankDefinition(),
							editor: { labels: {}, presets: {} },
						},
					],
				};
			case "templates":
				return { type: "templates", value: [] };
			case "save_subflow": {
				const saved = registered((action.id as { version: number }).version);
				activities = [...activities, saved];
				return { type: "operation", value: saved };
			}
			default:
				throw new Error(`unexpected ${String(action.action)}`);
		}
	});
});
const graph = () => screen.getByRole("region", { name: "Neutral control graph" });

it("registers a published workflow through the backend and places it on the graph", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	fireEvent.click(await screen.findByRole("button", { name: "Add as subflow: Lint & test · revision 2" }));
	await waitFor(() => expect(within(graph()).getByText("lint-test-r2 1")).toBeInTheDocument());
	const save = calls.filter((action) => action.action === "save_subflow");
	// One registration naming the exact publication; no hash is sent from the client.
	expect(save).toEqual([
		{
			action: "save_subflow",
			expected_version: 0,
			id: { namespace: "project", name: "lint-test-r2", version: 1 },
			publication: { project: "/repo", id: "child-id", revision: 2 },
		},
	]);
});

it("reuses the operation already pinned to that revision instead of registering another", async () => {
	activities = [registered(1)];
	render(() => <NeutralDesigner project="/repo" />);
	fireEvent.click(await screen.findByRole("button", { name: "Add as subflow: Lint & test · revision 2" }));
	await waitFor(() => expect(within(graph()).getByText("lint-test-r2 1")).toBeInTheDocument());
	expect(calls.some((action) => action.action === "save_subflow")).toBe(false);
});
