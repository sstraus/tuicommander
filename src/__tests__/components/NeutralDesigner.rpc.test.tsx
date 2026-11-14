import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import {
	type NeutralAction,
	type NeutralDefinition,
	type NeutralReplies,
	neutralClient,
} from "../../components/WorkflowDesigner/neutralClient";
import { activityStore } from "../../stores/activityStore";
import authoring from "../fixtures/workflow/neutral-transport-authoring.json";
import engine from "../fixtures/workflow/neutral-transport-engine.json";

// Keep invoke(), rpc(), HTTP error decoding and the command mapper real.
// Only the network edge returns recorded engine replies; no running backend is claimed.
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
const requests: { project: string | null; method: string | undefined; action: Record<string, unknown> }[] = [];
let missingCatalog = false;
const fetchReply = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
	const url = new URL(String(input));
	if (init?.method === "GET") return Response.json([]);
	if (url.pathname !== "/workflows/neutral/action") throw new Error(`Unexpected HTTP route: ${url.pathname}`);
	const action = JSON.parse(String(init?.body)) as Record<string, unknown>;
	requests.push({ project: url.searchParams.get("path"), method: init?.method, action });
	if (missingCatalog && action.action === "catalog")
		return Response.json({ error: "native catalog not installed" }, { status: 400 });
	if (url.searchParams.get("path") === engine.project && action.action === "inspect")
		return Response.json(engine.inspection);
	expect(url.searchParams.get("path")).toBe(authoring.project);
	switch (action.action) {
		case "catalog":
			return Response.json(authoring.catalog);
		case "list_drafts":
			return Response.json({ type: "drafts", value: [authoring.draft.value] });
		case "get_publication":
			return Response.json(authoring.publication);
		case "clone_publication":
			return Response.json(authoring.clone);
		case "update_draft":
			return Response.json(authoring.updated_draft);
		case "preview":
			return Response.json(authoring.preview);
		case "validate":
			return Response.json(authoring.validation);
		default:
			throw new Error(`No recorded HTTP reply for ${action.action}`);
	}
});
beforeAll(async () => {
	vi.stubGlobal("__TAURI_SHIM__", true);
	vi.stubGlobal("fetch", fetchReply);
	await activityStore.hydrate();
});
afterAll(() => vi.unstubAllGlobals());
beforeEach(() => {
	requests.length = 0;
	fetchReply.mockClear();
	missingCatalog = false;
});

it("keeps independent drafts visible when the real RPC path returns missing native catalog and recovers on refresh", async () => {
	missingCatalog = true;
	render(() => <NeutralDesigner project={authoring.project} />);
	expect(await screen.findByRole("alert")).toHaveTextContent("native catalog not installed");
	expect(screen.getByRole("button", { name: "operation · revision 1" })).toBeInTheDocument();
	expect(screen.queryByRole("button", { name: "Add increment · v1" })).not.toBeInTheDocument();
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "operation · revision 1" }));
	expect(screen.getByLabelText("Workflow name")).toHaveValue("operation");
	expect(screen.getByRole("alert")).toHaveTextContent("native catalog not installed");
	missingCatalog = false;
	await waitFor(() => expect(screen.getByRole("button", { name: "Refresh catalog" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Refresh catalog" }));
	await screen.findByRole("button", { name: "Add increment · v1" });
	expect(screen.queryByRole("alert")).not.toBeInTheDocument();
	expect(requests.every((request) => request.method === "POST" && request.project === authoring.project)).toBe(true);
});

it("sends exact neutral definitions and pins through invoke and HTTP without fixture-only client routing", async () => {
	const call = neutralClient(() => authoring.project);
	const definition = authoring.draft.value.definition as NeutralDefinition;
	const pin = authoring.publication.value;
	const cases: { action: NeutralAction; expected: keyof NeutralReplies; value: unknown }[] = [
		{ action: { action: "catalog" }, expected: "catalog", value: authoring.catalog.value },
		{ action: { action: "get_publication", id: pin.id, revision: pin.revision }, expected: "publication", value: pin },
		{
			action: { action: "clone_publication", id: pin.id, revision: pin.revision, name: "copy" },
			expected: "draft",
			value: authoring.clone.value,
		},
		{
			action: {
				action: "update_draft",
				id: authoring.draft.value.id,
				expected_revision: 1,
				name: "operation",
				definition,
				editor: authoring.updated_draft.value.editor,
			},
			expected: "draft",
			value: authoring.updated_draft.value,
		},
		{
			action: {
				action: "validate",
				definition: { ...definition, input_bindings: { "0": { value: { source: "constant", value: "wrong" } } } },
			},
			expected: "validation",
			value: authoring.validation.value,
		},
		{
			action: { action: "preview", definition, inputs: { value: 7 }, subject: null },
			expected: "preview",
			value: authoring.preview.value,
		},
	];
	for (const item of cases) {
		expect(await call(item.action, item.expected)).toEqual(item.value);
		expect(requests.at(-1)).toEqual({ project: authoring.project, method: "POST", action: item.action });
	}
	expect(requests).toHaveLength(cases.length);
	expect(
		requests.some(({ action }) =>
			["publish", "start", "enable_trigger_binding", "replay_trigger"].includes(String(action.action)),
		),
	).toBe(false);
});

it("reads retained ledger envelopes through real HTTP mapping without changing their coherent project identity", async () => {
	const call = neutralClient(() => engine.project);
	const action = {
		action: "inspect" as const,
		run_id: engine.inspection.value.snapshot.run.id,
		after_sequence: 0,
		limit: 500,
	};
	expect(await call(action, "inspection")).toEqual(engine.inspection.value);
	expect(requests).toEqual([{ project: engine.project, method: "POST", action }]);
});
