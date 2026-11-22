import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import type { ActivityDescriptor } from "../../components/WorkflowDesigner/catalogTypes";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import type { NeutralDefinition } from "../../components/WorkflowDesigner/neutralClient";
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

const field = (value_type: "string" | "boolean") => ({ value_type, required: true, default: null });
const unittest: ActivityDescriptor = {
	id: { namespace: "project", name: "unittest", version: 1 },
	purpose: "Run the unit tests",
	handler: { name: "script", version: 1 },
	target: null,
	config: { fields: {} },
	inputs: { fields: { repo: field("string") } },
	subject: null,
	outputs: { fields: { passed: field("boolean"), log: field("string") } },
	outcomes: ["ok"],
	required_capabilities: [],
	execution_policies: ["local"],
};
const lint: ActivityDescriptor = { ...unittest, id: { ...unittest.id, name: "lint" }, purpose: "Compile the sources" };
// Two activities: the return reads the second, so a select that falls back to its first option shows the wrong one.
const definition = (): NeutralDefinition => ({
	control: {
		nodes: {
			start: { type: "start" },
			lint: { type: "activity", activity: 0 },
			test: { type: "activity", activity: 1 },
			end: { type: "end", outcome: "completed" },
		},
		edges: { start: { next: "lint" }, lint: { ok: "test" }, test: { ok: "end" } },
	},
	inputs: { fields: { repo: field("string") } },
	outputs: { fields: { tests_passed: field("boolean") } },
	outcomes: ["completed"],
	subject: null,
	instances: [lint.id, unittest.id].map((descriptor) => ({ descriptor, config: {}, execution_policy: "local" })),
	input_bindings: {
		"0": { repo: { source: "workflow_input", field: "repo" } },
		"1": { repo: { source: "workflow_input", field: "repo" } },
	},
	conditions: {},
	collections: {},
	loops: {},
	forks: {},
	output_bindings: { tests_passed: { source: "activity_output", activity: 1, field: "passed" } },
	subflows: [],
});
const draft = () => ({
	id: "pipeline",
	project: "/repo",
	name: "Pipeline",
	revision: 2,
	latest_published_revision: 1,
	definition: definition(),
	editor: { labels: {}, presets: {} },
});
const calls: Record<string, unknown>[] = [];
beforeEach(() => {
	calls.length = 0;
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		calls.push(action);
		switch (action.action) {
			case "list_runs":
				return {
					type: "runs",
					value: [
						{
							id: "run-7",
							name: "Retained workflow",
							publication: { project: "/repo", id: "flow", revision: 1 },
							status: { type: "running" },
							current_node: "start",
							waiting_on_approval: false,
							started_ms: 1,
							updated_ms: 2,
						},
					],
				};
			case "catalog":
				return { type: "catalog", value: { activities: [lint, unittest], metanodes: [] } };
			case "list_drafts":
				return { type: "drafts", value: [draft()] };
			case "templates":
				return { type: "templates", value: [] };
			case "update_draft":
				return { type: "draft", value: { ...draft(), revision: 3, definition: action.definition } };
			case "start":
				return {
					type: "inspection",
					value: {
						snapshot: {
							run: { id: "run-7" },
							node: "end",
							status: { type: "completed" },
							outputs: { values: { tests_passed: true } },
							// Keyed by digest, as the backend retains them.
							effects: {
								a1: { observation: { outcome: { type: "completed" } } },
								b2: { observation: { outcome: { type: "completed" } } },
								c3: { observation: null },
							},
						},
						events: [{ sequence: 1 }],
					},
				};
			default:
				throw new Error(`unexpected ${String(action.action)}`);
		}
	});
});

async function openDraft() {
	render(() => <NeutralDesigner project="/repo" />);
	fireEvent.click(await screen.findByRole("button", { name: "Pipeline · revision 2" }));
}
const savedDefinition = async () => {
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(calls.map((action) => action.action)).toContain("update_draft"));
	// The save ends with a catalog refresh; the form stays disabled until it settles.
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).toBeEnabled());
	return calls.find((action) => action.action === "update_draft")?.definition as NeutralDefinition;
};

it("starts the published revision, not the newer draft revision, and reads the run back as rows", async () => {
	await openDraft();
	fireEvent.click(screen.getByRole("tab", { name: "Preview & runs" }));
	expect(screen.getByText(/Starts published revision 1\./)).toHaveTextContent("not part of the run");
	fireEvent.input(screen.getByRole("textbox", { name: "repo" }), { target: { value: "/workspace/repo" } });
	fireEvent.click(screen.getByRole("button", { name: "Start run" }));
	const run = await screen.findByLabelText("Run summary");
	const start = calls.find((action) => action.action === "start") as { request: Record<string, unknown> };
	expect(start.request).toMatchObject({
		publication: { project: "/repo", id: "pipeline", revision: 1 },
		inputs: { repo: "/workspace/repo" },
		subject: null,
		limits: { max_activations: 100, max_concurrency: 4, max_duration_ms: 600_000 },
	});
	expect(start.request.request_id).toEqual(expect.any(String));
	expect(within(run).getByText("completed")).toBeInTheDocument();
	expect(within(run).getByText("Return tests_passed").nextElementSibling).toHaveTextContent("true");
	expect(within(run).getByText("End · completed")).toBeInTheDocument();
	expect(within(run).getByText("Effects").nextElementSibling).toHaveTextContent("3 recorded · 2 completed, 1 pending");
	expect(screen.getByLabelText("Run summary")).toBeInTheDocument();
});

it("renaming a return field carries its mapping and removing it drops the mapping", async () => {
	await openDraft();
	fireEvent.click(screen.getByRole("tab", { name: "Inputs & returns" }));
	const name = screen.getByRole("textbox", { name: "Returns field tests_passed name" });
	fireEvent.input(name, { target: { value: "green" } });
	fireEvent.blur(name);
	expect(screen.getByRole("heading", { name: "Return mappings" })).toBeInTheDocument();
	let saved = await savedDefinition();
	expect(Object.keys(saved.outputs.fields)).toEqual(["green"]);
	expect(saved.output_bindings).toEqual({ green: { source: "activity_output", activity: 1, field: "passed" } });
	calls.length = 0;
	fireEvent.click(screen.getByRole("button", { name: "Remove returns field green" }));
	saved = await savedDefinition();
	expect(saved.outputs.fields).toEqual({});
	expect(saved.output_bindings).toEqual({});
});

it("offers the draft's activities and their declared outputs instead of an ordinal and a typed name", async () => {
	await openDraft();
	fireEvent.click(screen.getByRole("tab", { name: "Inputs & returns" }));
	const activity = screen.getByRole("combobox", { name: "tests_passed activity" }) as HTMLSelectElement;
	expect([...activity.options].map((option) => option.textContent)).toEqual(["lint 1", "unittest 2"]);
	expect(activity.value).toBe("1");
	const field = screen.getByRole("combobox", { name: "tests_passed source field" }) as HTMLSelectElement;
	expect([...field.options].map((option) => option.value)).toEqual(["", "passed", "log"]);
	expect(field.value).toBe("passed");
	fireEvent.change(field, { target: { value: "log" } });
	expect((await savedDefinition()).output_bindings).toEqual({
		tests_passed: { source: "activity_output", activity: 1, field: "log" },
	});
});
