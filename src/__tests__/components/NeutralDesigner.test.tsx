import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import type { ActivityDescriptor } from "../../components/WorkflowDesigner/catalogTypes";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import { repositoriesStore } from "../../stores/repositories";
import { subscribeEvents } from "../../transport";
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
const calls: Record<string, unknown>[] = [];
let valid = true;
let activities: ActivityDescriptor[] = [];
beforeEach(() => {
	calls.length = 0;
	valid = true;
	activities = [];
	vi.mocked(subscribeEvents).mockClear();
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		calls.push(action);
		switch (action.action) {
			case "catalog":
				return { type: "catalog", value: { activities, metanodes: [] } };
			case "list_drafts":
				return { type: "drafts", value: [] };
			case "create_draft":
				return {
					type: "draft",
					value: {
						id: "saved",
						project: "/repo",
						name: action.name,
						definition: action.definition,
						editor: action.editor,
						revision: 1,
						latest_published_revision: 0,
					},
				};
			case "validate":
				return {
					type: "validation",
					value: {
						valid,
						diagnostics: valid
							? []
							: [{ path: "/input_bindings/0/result", code: "incompatible", message: "Incompatible source type" }],
					},
				};
			case "inspect":
				return {
					type: "inspection",
					value: {
						snapshot: {
							node: "waiting",
							execution: "child",
							status: { type: "running" },
							wait_due_ms: 42,
							outputs: null,
							effects: { effect: { state: "uncertain" } },
							parent_fence: { execution: "root" },
						},
						events: [],
					},
				};
			case "preview":
				return {
					type: "preview",
					value: {
						validation: { valid: true, diagnostics: [] },
						activities: [
							{
								activity: 0,
								inputs: { result: { state: { state: "unknown" }, provenance: [] } },
								outputs: { fields: {} },
								outcomes: ["ok"],
								effect_intent: null,
								unavailable_reason: "producer output remains unknown",
							},
						],
						outputs: { fields: {} },
					},
				};
			default:
				throw new Error(`Unexpected action: ${action.action}`);
		}
	});
});
async function ready() {
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
}

it("creates a neutral blank draft without injecting a development kind or global review policy", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(calls.some((action) => action.action === "create_draft")).toBe(true));
	const request = calls.find((action) => action.action === "create_draft");
	expect(request).not.toHaveProperty("kind");
	expect(request?.definition).not.toHaveProperty("evidencePolicy");
	expect(request?.definition).toHaveProperty("instances", []);
	expect(screen.queryByText("Review policy")).not.toBeInTheDocument();
});

it("refuses publication when backend validation reports an incompatible mapping", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(screen.getByRole("button", { name: "Publish neutral draft" })).not.toBeDisabled());
	valid = false;
	fireEvent.click(screen.getByRole("button", { name: "Publish neutral draft" }));
	await screen.findByText("/input_bindings/0/result · Incompatible source type");
	expect(calls.some((action) => action.action === "publish")).toBe(false);
});

it("previews unsaved control edits without starting, publishing or executing an activity", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add wait" }));
	fireEvent.click(screen.getByRole("button", { name: "Preview sample IO and effect intents" }));
	await screen.findByText(/producer output remains unknown/);
	const preview = calls.find((action) => action.action === "preview");
	expect(preview?.definition).toMatchObject({
		control: {
			nodes: expect.objectContaining({ start: { type: "start" }, end: { type: "end", outcome: "completed" } }),
		},
	});
	if (!preview) throw new Error("Preview request was not submitted");
	const nodes = (preview.definition as { control: { nodes: Record<string, { type: string }> } }).control.nodes;
	expect(Object.values(nodes).some((node) => node.type === "wait")).toBe(true);
	expect(calls.map((action) => action.action)).not.toEqual(expect.arrayContaining(["publish"]));
	expect(
		calls.some((action) =>
			["start", "test_node", "save_operation", "activate_trigger"].includes(String(action.action)),
		),
	).toBe(false);
});

it("reports an unavailable catalog without manufacturing an operation", async () => {
	vi.mocked(invoke).mockRejectedValue(new Error("Neutral host unavailable"));
	render(() => <NeutralDesigner project="/repo" />);
	await screen.findByRole("alert");
	expect(screen.getByRole("alert")).toHaveTextContent("Neutral host unavailable");
	expect(screen.getByText(/No catalog activities loaded/)).toBeInTheDocument();
});

// Recorded by the backend serializer in 007, not a hand-written descriptor.
it("renders a registered custom script from its schema and saves config without a bespoke form", async () => {
	activities = [recordedScript as ActivityDescriptor];
	render(() => <NeutralDesigner project="/repo" />);
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add calculate · v1" }));
	expect(screen.getByText("Calculate a structured value")).toBeInTheDocument();
	expect((screen.getByRole("textbox", { name: "script" }) as HTMLTextAreaElement).value).toContain(
		'"timeout_secs": 10',
	);
	expect(screen.getByText("Named outcomes")).toBeInTheDocument();
	expect(
		within(screen.getByRole("region", { name: "Catalog activity inspector" })).getByText("ok"),
	).toBeInTheDocument();
	fireEvent.change(screen.getByRole("combobox", { name: "Execution policy" }), { target: { value: "local" } });
	fireEvent.input(screen.getByRole("textbox", { name: "Activity label" }), { target: { value: "Calculate sample" } });
	fireEvent.change(screen.getByRole("combobox", { name: "value source" }), { target: { value: "workflow_input" } });
	fireEvent.input(screen.getByRole("textbox", { name: "value source field" }), { target: { value: "count" } });
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(calls.some((action) => action.action === "create_draft")).toBe(true));
	const request = calls.find((action) => action.action === "create_draft");
	expect(request?.definition).toMatchObject({
		instances: [
			{ descriptor: { namespace: "custom", name: "calculate", version: 1 }, execution_policy: "local", config: {} },
		],
		input_bindings: { "0": { value: { source: "workflow_input", field: "count" } } },
	});
	if (!request) throw new Error("Draft was not submitted");
	expect(Object.values((request.editor as { labels: Record<string, string> }).labels)).toEqual(["Calculate sample"]);
	expect(calls.some((action) => ["start", "publish", "test_node"].includes(String(action.action)))).toBe(false);
});

it("rereads retained run state on lifecycle invalidation and reconnect without executing the run", async () => {
	render(() => <NeutralDesigner project="/repo" />);
	await ready();
	fireEvent.input(screen.getByRole("textbox", { name: "Run ID" }), { target: { value: "retained-run" } });
	fireEvent.click(screen.getByRole("button", { name: "Inspect retained run" }));
	await screen.findByRole("region", { name: "Retained run" });
	expect(screen.getByText(/"state": "uncertain"/)).toBeInTheDocument();
	const subscription = vi.mocked(subscribeEvents).mock.calls.at(-1);
	if (!subscription) throw new Error("Run invalidation subscription missing");
	subscription[0]["workflow-run-changed"]({ repo_path: "/another", payload: { runId: "retained-run" } });
	expect(calls.filter((action) => action.action === "inspect")).toHaveLength(1);
	subscription[0]["workflow-run-changed"]({ repo_path: "/repo", payload: { runId: "retained-run" } });
	await waitFor(() => expect(calls.filter((action) => action.action === "inspect")).toHaveLength(2));
	await ready();
	subscription[1]?.onResync?.("reconnect");
	await waitFor(() => expect(calls.filter((action) => action.action === "inspect")).toHaveLength(3));
	expect(calls.some((action) => action.action === "start")).toBe(false);
});

it("refreshes catalog metadata after an invalidation during a request without discarding unsaved graph edits", async () => {
	let release: (() => void) | undefined;
	const blocked = new Promise<void>((resolve) => {
		release = resolve;
	});
	vi.mocked(invoke).mockImplementationOnce(async () => {
		await blocked;
		return { type: "catalog", value: { activities: [], metanodes: [] } };
	});
	render(() => <NeutralDesigner project="/repo" />);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).toBeDisabled());
	repositoriesStore.bumpRevision("/repo");
	release?.();
	await ready();
	await waitFor(() => expect(calls.filter((action) => action.action === "catalog")).toHaveLength(1));
	fireEvent.click(screen.getByRole("button", { name: "Add wait" }));
	repositoriesStore.bumpRevision("/repo");
	await ready();
	expect(screen.getByRole("spinbutton", { name: "Wait milliseconds" })).toHaveValue(1000);
});
