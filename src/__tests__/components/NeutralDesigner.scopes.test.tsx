import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import type { NeutralDefinition } from "../../components/WorkflowDesigner/neutralClient";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import descriptor from "../fixtures/workflow/custom_script_descriptor.json";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
const requests: Record<string, unknown>[] = [];
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});
beforeEach(() => {
	requests.length = 0;
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		requests.push(action);
		if (action.action === "catalog") return { type: "catalog", value: { activities: [descriptor], metanodes: [] } };
		if (action.action === "list_drafts") return { type: "drafts", value: [] };
		if (action.action === "templates") return { type: "templates", value: [] };
		if (action.action === "create_draft")
			return {
				type: "draft",
				value: {
					id: "saved",
					project: "/repo",
					revision: 1,
					latest_published_revision: 0,
					name: action.name,
					definition: action.definition,
					editor: action.editor,
				},
			};
		throw new Error(`Unexpected effect or request: ${action.action}`);
	});
});
async function ready() {
	render(() => <NeutralDesigner project="/repo" />);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
}
async function saved() {
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(requests.some((request) => request.action === "create_draft")).toBe(true));
	// Let the save response settle in the UI before the test root is disposed.
	await screen.findByText(/Saved · revision 1/);
	return requests.find((request) => request.action === "create_draft")!;
}

it("authors fork bodies without ordinary control owners or unsaveable synthetic label metadata", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add fork" }));
	fireEvent.click(screen.getByRole("button", { name: "Create scope contract" }));
	for (const name of ["Review", "Verify"]) {
		fireEvent.input(screen.getByLabelText("New branch name"), { target: { value: name } });
		fireEvent.change(screen.getByLabelText("New fork branch body"), { target: { value: "0" } });
		expect(screen.queryByLabelText("Activity label")).not.toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("Execution policy"), { target: { value: "local" } });
		fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	}
	fireEvent.input(screen.getByLabelText("Maximum concurrency"), { target: { value: "2" } });
	fireEvent.click(screen.getByRole("button", { name: "Open scope · Fork 1" }));
	const topology = within(screen.getByRole("region", { name: "Scope body graph" }));
	expect(topology.getByText("All join · complete / blocked")).toBeInTheDocument();
	fireEvent.click(topology.getByRole("button", { name: "Review · calculate activity 0 · required success" }));
	expect(screen.getByLabelText("Execution policy")).toHaveValue("local");
	fireEvent.click(screen.getByRole("button", { name: "Workflow graph" }));
	const request = await saved();
	const definition = request.definition as NeutralDefinition;
	expect(definition.forks).toEqual({
		fork_1: {
			branches: { Review: { activity: 0, required: true }, Verify: { activity: 1, required: true } },
			max_concurrency: 2,
		},
	});
	expect(definition.instances).toEqual(
		[0, 1].map(() => ({
			descriptor: { namespace: "custom", name: "calculate", version: 1 },
			config: {},
			execution_policy: "local",
		})),
	);
	expect(Object.values(definition.control.nodes).filter((node) => node.type === "activity")).toEqual([]);
	expect(request.editor).toEqual({ labels: {}, presets: {} });
	expect(
		requests.every((item) => ["catalog", "list_drafts", "templates", "create_draft"].includes(String(item.action))),
	).toBe(true);
});

it("writes loop carried results only as direct current-body output sources and never retargets an old result silently", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add loop" }));
	fireEvent.click(screen.getByRole("button", { name: "Create scope contract" }));
	fireEvent.change(screen.getByLabelText("New scope body"), { target: { value: "0" } });
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	fireEvent.input(screen.getByLabelText("Carried state new field name"), { target: { value: "value" } });
	fireEvent.change(screen.getByLabelText("Carried state new field type"), { target: { value: "integer" } });
	fireEvent.click(screen.getByRole("button", { name: "Add carried state field" }));
	fireEvent.change(screen.getByLabelText("value source"), { target: { value: "constant" } });
	fireEvent.change(screen.getByRole("textbox", { name: "value" }), { target: { value: "0" } });
	fireEvent.change(screen.getByLabelText("value loop result output"), { target: { value: "next" } });
	fireEvent.change(screen.getByLabelText("New scope body"), { target: { value: "0" } });
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	expect(screen.getByLabelText("value loop result output")).toHaveValue("");
	fireEvent.change(screen.getByLabelText("value loop result output"), { target: { value: "next" } });
	fireEvent.click(screen.getByRole("button", { name: "Open scope · Loop 1" }));
	expect(screen.getByText(/direct body outputs → next carried state/)).toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "Workflow graph" }));
	const request = await saved();
	const definition = request.definition as NeutralDefinition;
	expect(definition.instances).toHaveLength(1);
	expect(definition.loops).toEqual({
		loop_1: {
			body: 0,
			max_iterations: 3,
			carried: { fields: { value: { value_type: "integer", required: true, default: null } } },
			initial: { value: { source: "constant", value: 0 } },
			result: { value: { source: "activity_output", activity: 0, field: "next" } },
		},
	});
});

it("offers a body only to its own scope owner", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add loop" }));
	fireEvent.click(screen.getByRole("button", { name: "Create scope contract" }));
	fireEvent.change(screen.getByLabelText("New scope body"), { target: { value: "0" } });
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	fireEvent.click(screen.getByRole("button", { name: "Workflow graph" }));
	fireEvent.click(screen.getByRole("button", { name: "Add collection" }));
	fireEvent.click(screen.getByRole("button", { name: "Create scope contract" }));
	const options = within(screen.getByLabelText("Scope body"))
		.queryAllByRole("option")
		.map((option) => option.getAttribute("value"));
	expect(options).not.toContain("0");
});

it("authors collection contracts with ordered envelopes and preserves body inputs when navigating topology", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add collection" }));
	fireEvent.click(screen.getByRole("button", { name: "Create scope contract" }));
	fireEvent.change(screen.getByLabelText("New scope body"), { target: { value: "0" } });
	fireEvent.change(screen.getByLabelText("value source"), { target: { value: "workflow_input" } });
	fireEvent.input(screen.getByLabelText("value source field"), { target: { value: "source" } });
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	fireEvent.change(screen.getByLabelText("collection source"), { target: { value: "workflow_input" } });
	fireEvent.input(screen.getByLabelText("collection source field"), { target: { value: "flags" } });
	fireEvent.change(screen.getByLabelText("Item type"), { target: { value: "boolean" } });
	fireEvent.input(screen.getByLabelText("Item input"), { target: { value: "flag" } });
	fireEvent.input(screen.getByLabelText("Index input"), { target: { value: "position" } });
	fireEvent.input(screen.getByLabelText("Maximum concurrency"), { target: { value: "2" } });
	fireEvent.change(screen.getByLabelText("Item failure"), { target: { value: "collect" } });
	fireEvent.click(screen.getByRole("button", { name: "Open scope · Collection 1" }));
	expect(screen.getByText("Ordered result envelopes")).toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "Item body · calculate activity 0" }));
	expect(screen.getByLabelText("value source field")).toHaveValue("source");
	const request = await saved();
	const definition = request.definition as NeutralDefinition;
	expect(definition.collections).toEqual({
		collection_1: {
			collection: { source: "workflow_input", field: "flags" },
			item_type: "boolean",
			body: 0,
			item_input: "flag",
			index_input: "position",
			max_items: 10,
			max_concurrency: 2,
			empty: "complete",
			failure: "collect",
		},
	});
	expect(definition.input_bindings).toEqual({ "0": { value: { source: "workflow_input", field: "source" } } });
	expect(definition.control.edges).toEqual({ start: { next: "end" } });
});

it("authors a condition through its inspector while retaining separate true false and unknown route ports", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add condition" }));
	fireEvent.change(screen.getByLabelText("Condition operator"), { target: { value: "exists" } });
	const inspector = within(screen.getByRole("region", { name: "Condition contract inspector" }));
	fireEvent.input(inspector.getByLabelText("value source field"), { target: { value: "request" } });
	for (const port of ["true", "false", "unknown"])
		expect(screen.getByRole("button", { name: `Connect Condition 1 ${port}` })).toBeInTheDocument();
	const request = await saved();
	expect((request.definition as NeutralDefinition).conditions).toEqual({
		condition_1: { operator: "exists", value: { source: "workflow_input", field: "request" } },
	});
});
