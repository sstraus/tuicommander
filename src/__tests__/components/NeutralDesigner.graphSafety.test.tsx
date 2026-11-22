import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import type { ActivityDescriptor } from "../../components/WorkflowDesigner/catalogTypes";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import type { NeutralDefinition, NeutralDraft } from "../../components/WorkflowDesigner/neutralClient";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import recordedScript from "../fixtures/workflow/custom_script_descriptor.json";

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});

/** A saved draft with a cycle, a disconnected node and a route to a missing node. */
function fixtureDefinition(): NeutralDefinition {
	return {
		control: {
			nodes: {
				start: { type: "start" },
				gate: { type: "condition", condition: "ready" },
				pause: { type: "wait", duration_ms: 500 },
				done: { type: "end", outcome: "completed" },
				fail: { type: "end", outcome: "failed" },
				orphan: { type: "wait", duration_ms: 10 },
			},
			edges: {
				start: { next: "gate" },
				gate: { true: "done", false: "fail", unknown: "pause" },
				pause: { next: "gate" },
				orphan: { next: "ghost" },
			},
		},
		outputs: { fields: {} },
		outcomes: ["completed", "failed"],
		inputs: { fields: { ticket: { value_type: "string", required: true, default: "T-1" } } },
		subject: null,
		instances: [],
		input_bindings: {},
		conditions: { ready: true },
		collections: {},
		loops: {},
		forks: {},
		output_bindings: {},
		subflows: [],
	} as NeutralDefinition;
}
const fixtureLabels = { gate: "Gate check", pause: "Cool down", orphan: "Orphan wait" };
function draft(id: string, name: string): NeutralDraft {
	return {
		id,
		project: "/repo",
		name,
		revision: 3,
		latest_published_revision: 0,
		definition: fixtureDefinition(),
		editor: { labels: { ...fixtureLabels }, presets: {} },
	};
}

const calls: Record<string, unknown>[] = [];
let activities: ActivityDescriptor[] = [];
beforeEach(() => {
	calls.length = 0;
	activities = [];
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		calls.push(structuredClone(action));
		switch (action.action) {
			case "list_runs":
				return {
					type: "runs",
					value: [
						{
							id: "run-1",
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
				return { type: "catalog", value: { activities, metanodes: [] } };
			case "list_drafts":
				return { type: "drafts", value: [draft("d1", "Fixture flow"), draft("d2", "Twin flow")] };
			case "templates":
				return { type: "templates", value: [] };
			case "update_draft":
				return {
					type: "draft",
					value: {
						id: action.id,
						project: "/repo",
						name: action.name,
						definition: action.definition,
						editor: action.editor,
						revision: (action.expected_revision as number) + 1,
						latest_published_revision: 0,
					},
				};
			case "validate":
				return { type: "validation", value: { valid: true, diagnostics: [] } };
			case "preview":
				return {
					type: "preview",
					value: { validation: { valid: true, diagnostics: [] }, activities: [], outputs: { fields: {} } },
				};
			case "inspect":
				return {
					type: "inspection",
					value: {
						snapshot: {
							node: "pause",
							execution: "root",
							status: { type: "running" },
							wait_due_ms: 1,
							outputs: null,
							effects: {},
							parent_fence: null,
						},
						events: [],
					},
				};
			default:
				throw new Error(`Unexpected action: ${action.action}`);
		}
	});
});

async function openFixture(name = "Fixture flow · revision 3") {
	render(() => <NeutralDesigner project="/repo" />);
	await waitFor(() => expect(screen.getByRole("button", { name })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name }));
	await waitFor(() => expect(screen.getByText(/Saved · revision 3/)).toBeInTheDocument());
}
const graph = () => screen.getByRole("region", { name: "Neutral control graph" });
const graphNode = (title: string) =>
	within(graph()).getByRole("button", { name: new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`) });
async function savedRequest() {
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(calls.some((action) => action.action === "update_draft")).toBe(true));
	return calls.filter((action) => action.action === "update_draft").at(-1) as {
		definition: NeutralDefinition;
		editor: { labels: Record<string, string> };
	};
}
const writes = () =>
	calls.filter((action) =>
		["create_draft", "update_draft", "publish", "clone_publication", "save_preset"].includes(action.action as string),
	);

it("renders every saved node, including cyclic and disconnected ones, and flags routes to missing nodes", async () => {
	await openFixture();
	for (const title of ["Start", "Gate check", "Cool down", "End · completed", "End · failed", "Orphan wait"])
		expect(graphNode(title)).toBeInTheDocument();
	expect(within(graph()).getByText(/6 nodes · 6 routes/)).toBeInTheDocument();
	expect(within(graph()).getByText(/routes reference missing nodes/)).toBeInTheDocument();
	fireEvent.click(graphNode("Orphan wait"));
	const target = screen.getByRole("combobox", { name: "Orphan wait next target" }) as HTMLSelectElement;
	expect(target.value).toBe("ghost");
	expect(target.selectedOptions[0].textContent).toBe("Missing destination");
});

it("reconnecting one route from the graph changes only that source port and keeps siblings, data and labels", async () => {
	await openFixture();
	fireEvent.click(graphNode("Gate check"));
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect Gate check false" }));
	expect(within(graph()).getByRole("status")).toHaveTextContent("Connect Gate check · false");
	fireEvent.click(graphNode("Cool down"));
	expect(within(graph()).queryByRole("status")).not.toBeInTheDocument();
	expect(screen.getByText(/Unsaved changes/)).toBeInTheDocument();
	const saved = await savedRequest();
	const expected = fixtureDefinition();
	expected.control.edges.gate = { true: "done", false: "pause", unknown: "pause" };
	expect(saved.definition).toEqual(expected);
	expect(saved.editor.labels).toEqual(fixtureLabels);
});

it("reconnecting or clearing one route from the inspector keeps every sibling route", async () => {
	await openFixture();
	fireEvent.click(graphNode("Gate check"));
	for (const [port, target] of Object.entries(fixtureDefinition().control.edges.gate))
		expect((screen.getByRole("combobox", { name: `Gate check ${port} target` }) as HTMLSelectElement).value).toBe(
			target,
		);
	fireEvent.change(screen.getByRole("combobox", { name: "Gate check unknown target" }), { target: { value: "done" } });
	fireEvent.change(screen.getByRole("combobox", { name: "Gate check true target" }), { target: { value: "" } });
	const saved = await savedRequest();
	const expected = fixtureDefinition();
	expected.control.edges.gate = { false: "fail", unknown: "done" };
	expect(saved.definition).toEqual(expected);
});

it("zoom, fit, selection and cancelled connections never mark or change the draft", async () => {
	await openFixture();
	const save = screen.getByRole("button", { name: "Save neutral draft" });
	fireEvent.click(within(graph()).getByRole("button", { name: "Zoom in" }));
	fireEvent.click(within(graph()).getByRole("button", { name: "Zoom out" }));
	fireEvent.click(within(graph()).getByRole("button", { name: "Zoom out" }));
	fireEvent.click(within(graph()).getByRole("button", { name: "Fit graph" }));
	for (const title of ["Orphan wait", "Start", "Gate check"]) fireEvent.click(graphNode(title));
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect Gate check true" }));
	fireEvent.click(within(graph()).getByRole("button", { name: "Cancel connection" }));
	fireEvent.click(graphNode("End · failed"));
	fireEvent.click(graphNode("Gate check"));
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect Gate check false" }));
	fireEvent.keyDown(within(graph()).getByLabelText("Scrollable workflow canvas"), { key: "Escape" });
	fireEvent.click(graphNode("Orphan wait"));
	expect(screen.getByText(/Saved · revision 3/)).toBeInTheDocument();
	expect(save).not.toHaveAttribute("data-ready");
	const saved = await savedRequest();
	expect(saved.definition).toEqual(fixtureDefinition());
	expect(saved.editor.labels).toEqual(fixtureLabels);
});

it("a connection pending in one document never applies to another opened with the same node ids", async () => {
	await openFixture();
	fireEvent.click(graphNode("Gate check"));
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect Gate check false" }));
	fireEvent.click(screen.getByRole("button", { name: "Twin flow · revision 3" }));
	await waitFor(() => expect(within(graph()).queryByRole("status")).not.toBeInTheDocument());
	fireEvent.click(graphNode("Cool down"));
	expect(screen.getByText(/Saved · revision 3/)).toBeInTheDocument();
	const saved = await savedRequest();
	expect(saved.definition).toEqual(fixtureDefinition());
	expect(calls.find((action) => action.action === "update_draft")?.id).toBe("d2");
});

it("a node label is saved editor metadata and leaves the definition untouched", async () => {
	await openFixture();
	fireEvent.click(graphNode("Orphan wait"));
	fireEvent.input(screen.getByRole("textbox", { name: "Node label" }), { target: { value: "Lonely wait" } });
	expect(graphNode("Lonely wait")).toBeInTheDocument();
	const saved = await savedRequest();
	expect(saved.definition).toEqual(fixtureDefinition());
	expect(saved.editor.labels).toEqual({ ...fixtureLabels, orphan: "Lonely wait" });
});

it("the activity palette is driven by any catalog descriptor and exposes its declared outcomes as routes", async () => {
	activities = [
		{
			...(recordedScript as unknown as ActivityDescriptor),
			id: { namespace: "acme.custom", name: "ticket_sync", version: 2 },
			purpose: "Synchronise a ticket",
			outcomes: ["synced", "conflict"],
		},
	];
	await openFixture();
	const palette = screen.getByRole("complementary", { name: "Catalog palette" });
	fireEvent.input(within(palette).getByRole("searchbox", { name: "Search catalog" }), { target: { value: "zzz" } });
	expect(within(palette).queryByRole("button", { name: /ticket_sync/ })).not.toBeInTheDocument();
	fireEvent.input(within(palette).getByRole("searchbox", { name: "Search catalog" }), { target: { value: "acme" } });
	fireEvent.click(within(palette).getByRole("button", { name: /Add ticket_sync · v2/ }));
	expect(within(graph()).getByRole("button", { name: "Connect ticket_sync 1 synced" })).toBeInTheDocument();
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect ticket_sync 1 conflict" }));
	fireEvent.click(graphNode("End · failed"));
	const saved = await savedRequest();
	expect(saved.definition.instances).toEqual([
		{ descriptor: { namespace: "acme.custom", name: "ticket_sync", version: 2 }, config: {}, execution_policy: "" },
	]);
	const added = Object.entries(saved.definition.control.nodes).find(([, node]) => node.type === "activity");
	expect(added?.[1]).toEqual({ type: "activity", activity: 0 });
	expect(saved.definition.control.edges[added?.[0] ?? ""]).toEqual({ conflict: "fail" });
	const { [added?.[0] ?? ""]: _node, ...otherNodes } = saved.definition.control.nodes;
	const { [added?.[0] ?? ""]: _edges, ...otherEdges } = saved.definition.control.edges;
	expect(otherNodes).toEqual(fixtureDefinition().control.nodes);
	expect(otherEdges).toEqual(fixtureDefinition().control.edges);
});

it("root typed inputs add a unique field, reject duplicates and retain existing defaults", async () => {
	await openFixture();
	fireEvent.click(screen.getByRole("tab", { name: "Inputs & returns" }));
	const inputs = screen.getByRole("region", { name: "Inputs" });
	expect(within(inputs).getByText(/Default: "T-1"/)).toBeInTheDocument();
	fireEvent.input(within(inputs).getByRole("textbox", { name: "Inputs new field name" }), {
		target: { value: "ticket" },
	});
	fireEvent.click(within(inputs).getByRole("button", { name: "Add inputs field" }));
	expect(within(inputs).getByRole("alert")).toHaveTextContent("Use a nonempty, unique field name.");
	expect(screen.getByText(/Saved · revision 3/)).toBeInTheDocument();
	fireEvent.input(within(inputs).getByRole("textbox", { name: "Inputs new field name" }), {
		target: { value: "priority" },
	});
	fireEvent.change(within(inputs).getByRole("combobox", { name: "Inputs new field type" }), {
		target: { value: "integer" },
	});
	fireEvent.click(within(inputs).getByRole("button", { name: "Add inputs field" }));
	fireEvent.change(within(inputs).getByRole("combobox", { name: "Inputs ticket presence" }), {
		target: { value: "optional" },
	});
	const saved = await savedRequest();
	const expected = fixtureDefinition();
	expected.inputs = {
		fields: {
			ticket: { value_type: "string", required: false, default: "T-1" },
			priority: { value_type: "integer", required: true, default: null },
		},
	};
	expect(saved.definition).toEqual(expected);
});

it("validation, preview and retained inspection use the unsaved graph edit without writing anything", async () => {
	await openFixture();
	fireEvent.click(graphNode("Gate check"));
	fireEvent.click(within(graph()).getByRole("button", { name: "Connect Gate check false" }));
	fireEvent.click(graphNode("Cool down"));
	const expected = fixtureDefinition();
	expected.control.edges.gate = { true: "done", false: "pause", unknown: "pause" };
	fireEvent.click(screen.getByRole("button", { name: "Validate links and inputs" }));
	await waitFor(() => expect(calls.some((action) => action.action === "validate")).toBe(true));
	expect(calls.find((action) => action.action === "validate")?.definition).toEqual(expected);
	fireEvent.click(screen.getByRole("tab", { name: "Preview & runs" }));
	await waitFor(() =>
		expect(screen.getByRole("button", { name: "Preview sample IO and effect intents" })).not.toBeDisabled(),
	);
	fireEvent.click(screen.getByRole("button", { name: "Preview sample IO and effect intents" }));
	await waitFor(() => expect(screen.getByRole("region", { name: "Resolved preview" })).toBeInTheDocument());
	expect(calls.find((action) => action.action === "preview")?.definition).toEqual(expected);
	fireEvent.click(await screen.findByRole("button", { name: /Retained workflow · revision 1/ }));
	await waitFor(() => expect(screen.getByRole("region", { name: "Retained run" })).toBeInTheDocument());
	expect(calls.find((action) => action.action === "inspect")).toMatchObject({ run_id: "run-1" });
	expect(writes()).toEqual([]);
	expect(screen.getByText(/Unsaved changes/)).toBeInTheDocument();
});

const scopedCatalog = (): ActivityDescriptor[] => [
	{ ...(recordedScript as unknown as ActivityDescriptor), id: { namespace: "acme", name: "step", version: 1 } },
];
const scopeInspector = () => screen.getByRole("region", { name: "Scope contract inspector" });
function addScopeNode(kind: "loop" | "fork" | "collection") {
	fireEvent.click(screen.getByRole("button", { name: `Add ${kind}` }));
	fireEvent.click(within(scopeInspector()).getByRole("button", { name: "Create scope contract" }));
}
const scopeNodeId = (definition: NeutralDefinition, type: string) =>
	Object.entries(definition.control.nodes).find(([, node]) => node.type === type)?.[0] ?? "";

it("a new collection scope writes exactly the backend contract fields and binds the node to it", async () => {
	await openFixture();
	addScopeNode("collection");
	const saved = await savedRequest();
	const contract = saved.definition.collections.collection_1 as Record<string, unknown>;
	// Mirrors the deny_unknown_fields CollectionContract in src-tauri/src/workflows/collection.rs.
	expect(Object.keys(contract).sort()).toEqual(
		[
			"body",
			"collection",
			"empty",
			"failure",
			"index_input",
			"item_input",
			"item_type",
			"max_concurrency",
			"max_items",
		].sort(),
	);
	expect(saved.definition.control.nodes[scopeNodeId(saved.definition, "collection")]).toEqual({
		type: "collection",
		scope: "collection_1",
	});
});

it("a loop scope body comes from the catalog, its carried result reads that body directly, and its condition is a tagged predicate", async () => {
	activities = scopedCatalog();
	await openFixture();
	addScopeNode("loop");
	fireEvent.change(within(scopeInspector()).getByRole("combobox", { name: "New scope body" }), {
		target: { value: "0" },
	});
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	const carried = screen.getByRole("region", { name: "Carried state" });
	fireEvent.input(within(carried).getByRole("textbox", { name: "Carried state new field name" }), {
		target: { value: "count" },
	});
	fireEvent.change(within(carried).getByRole("combobox", { name: "Carried state new field type" }), {
		target: { value: "integer" },
	});
	fireEvent.click(within(carried).getByRole("button", { name: "Add carried state field" }));
	fireEvent.change(screen.getByRole("combobox", { name: "count loop result output" }), { target: { value: "next" } });
	fireEvent.change(screen.getByRole("combobox", { name: "Condition operator" }), { target: { value: "compare" } });
	const saved = await savedRequest();
	expect(saved.definition.instances).toEqual([
		{ descriptor: { namespace: "acme", name: "step", version: 1 }, config: {}, execution_policy: "" },
	]);
	expect(saved.definition.loops.loop_1).toEqual({
		body: 0,
		max_iterations: 3,
		carried: { fields: { count: { value_type: "integer", required: true, default: null } } },
		initial: {},
		result: { count: { source: "activity_output", activity: 0, field: "next" } },
	});
	expect(saved.definition.conditions).toEqual({
		ready: true,
		condition_1: {
			operator: "compare",
			comparison: "equal",
			left: { source: "workflow_input", field: "" },
			right: { source: "constant", value: "" },
		},
	});
	expect(saved.definition.control.nodes[scopeNodeId(saved.definition, "loop")]).toEqual({
		type: "loop",
		scope: "loop_1",
		condition: "condition_1",
	});
});

it("fork branches need a unique name and each new branch owns a fresh catalog instance", async () => {
	activities = scopedCatalog();
	await openFixture();
	addScopeNode("fork");
	const picker = () => within(scopeInspector()).getByRole("combobox", { name: "New fork branch body" });
	expect(picker()).toBeDisabled();
	fireEvent.input(within(scopeInspector()).getByRole("textbox", { name: "New branch name" }), {
		target: { value: "left" },
	});
	fireEvent.change(picker(), { target: { value: "0" } });
	fireEvent.click(screen.getByRole("button", { name: "Back to scope" }));
	fireEvent.input(within(scopeInspector()).getByRole("textbox", { name: "New branch name" }), {
		target: { value: "left" },
	});
	expect(picker()).toBeDisabled();
	const saved = await savedRequest();
	expect(saved.definition.forks.fork_1).toEqual({
		branches: { left: { activity: 0, required: true } },
		max_concurrency: 1,
	});
	expect(saved.definition.instances).toHaveLength(1);
});
