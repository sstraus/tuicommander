import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import engine from "../fixtures/workflow/neutral-transport-engine.json";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});
const requests: Record<string, unknown>[] = [];
beforeEach(() => {
	requests.length = 0;
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		expect(args?.project).toBe(engine.project);
		const action = args?.action as Record<string, unknown>;
		requests.push(action);
		switch (action.action) {
			case "catalog":
				return engine.catalog;
			case "list_drafts":
				return { type: "drafts", value: [] };
			case "get_publication":
				return engine.publication;
			case "inspect":
				return engine.inspection;
			default:
				throw new Error(`No recorded reply for ${action.action}`);
		}
	});
});
async function ready() {
	render(() => <NeutralDesigner project={engine.project} />);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
}

// These envelopes are actual engine serialization, not hand-written DTO approximations.
it("renders the engine catalog descriptor schema and named ports through the shared client", async () => {
	await ready();
	fireEvent.click(screen.getByRole("button", { name: "Add increment · v1" }));
	const inspector = within(screen.getByRole("region", { name: "Catalog activity inspector" }));
	expect(inspector.getByText("Increment a typed value")).toBeInTheDocument();
	expect(JSON.parse(inspector.getByText(/"handler":/).textContent ?? "null")).toEqual(
		engine.catalog.value.activities[0],
	);
	expect(inspector.getByText("ok")).toBeInTheDocument();
	expect(screen.getByRole("combobox", { name: "value source" })).toBeInTheDocument();
	expect(screen.queryByText("Review policy")).not.toBeInTheDocument();
	expect(requests.every((action) => ["catalog", "list_drafts", "templates"].includes(String(action.action)))).toBe(
		true,
	);
});

it("opens the exact pinned engine graph without cloning, updating or repinning it", async () => {
	await ready();
	const pin = engine.publication.value;
	fireEvent.click(screen.getByRole("button", { name: `Inspect ${pin.id} · revision ${pin.revision}` }));
	const graph = await screen.findByRole("region", { name: "Pinned metanode graph" });
	expect(within(graph).getByRole("heading")).toHaveTextContent(`${pin.id} · revision 1 · read only`);
	expect(JSON.parse(graph.querySelector("pre")?.textContent ?? "null")).toEqual(pin.definition);
	expect(requests.at(-1)).toEqual({ action: "get_publication", id: pin.id, revision: pin.revision });
	expect(
		requests.some((action) =>
			["clone_publication", "update_draft", "publish", "start"].includes(String(action.action)),
		),
	).toBe(false);
});

it("shows the retained engine path and output8 from its ledger without executing another run", async () => {
	await ready();
	const snapshot = engine.inspection.value.snapshot;
	const runId = snapshot.run.id;
	fireEvent.input(screen.getByRole("textbox", { name: "Run ID" }), { target: { value: runId } });
	fireEvent.click(screen.getByRole("button", { name: "Inspect retained run" }));
	const retained = await screen.findByRole("region", { name: "Retained run" });
	const blocks = retained.querySelectorAll("pre");
	expect(JSON.parse(blocks[0].textContent ?? "null")).toEqual({
		node: snapshot.node,
		execution: snapshot.execution,
		status: snapshot.status,
	});
	expect(JSON.parse(blocks[1].textContent ?? "null")).toEqual({
		outputs: snapshot.outputs,
		activity_outputs: snapshot.activity_outputs,
	});
	expect(JSON.parse(blocks[4].textContent ?? "null")).toEqual(engine.inspection.value.events);
	expect(snapshot.outputs.values.answer).toBe(8);
	expect(requests.at(-1)).toEqual({ action: "inspect", run_id: runId, after_sequence: 0, limit: 500 });
	expect(
		requests.some((action) => ["start", "answer", "cancel", "replay_trigger"].includes(String(action.action))),
	).toBe(false);
});
