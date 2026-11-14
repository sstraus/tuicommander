import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import issueTemplate from "../../../src-tauri/src/workflows/templates/github-issue-triage.json";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import authoring from "../fixtures/workflow/neutral-transport-authoring.json";

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
		expect(args?.project).toBe(authoring.project);
		const action = args?.action as Record<string, unknown>;
		requests.push(action);
		switch (action.action) {
			case "catalog":
				return authoring.catalog;
			case "list_drafts":
				return { type: "drafts", value: [authoring.draft.value] };
			case "templates":
				return { type: "templates", value: [issueTemplate] };
			case "create_draft":
				return {
					type: "draft",
					value: {
						id: "created",
						project: authoring.project,
						name: action.name,
						revision: 1,
						latest_published_revision: 0,
						definition: action.definition,
						editor: action.editor,
					},
				};
			case "update_draft":
				return authoring.updated_draft;
			case "get_publication":
				return authoring.publication;
			case "clone_publication":
				return authoring.clone;
			case "validate":
				return authoring.validation;
			case "preview":
				return authoring.preview;
			default:
				throw new Error(`No recorded reply for ${action.action}`);
		}
	});
});
async function ready(load = true) {
	render(() => <NeutralDesigner project={authoring.project} />);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
	if (load) fireEvent.click(screen.getByRole("button", { name: "operation · revision 1" }));
}
function selectWork() {
	fireEvent.click(screen.getByRole("button", { name: "Reviewer activity · activity 0" }));
}

it("edits recorded draft label metadata and saves at the original revision without changing semantics", async () => {
	await ready();
	selectWork();
	expect(screen.getByLabelText("Activity label")).toHaveValue("Reviewer");
	fireEvent.input(screen.getByLabelText("Activity label"), { target: { value: "Writer" } });
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
	expect(requests.find((action) => action.action === "update_draft")).toEqual({
		action: "update_draft",
		id: authoring.draft.value.id,
		expected_revision: 1,
		name: "operation",
		definition: authoring.draft.value.definition,
		editor: authoring.updated_draft.value.editor,
	});
	expect(screen.getByRole("button", { name: "Writer activity · activity 0" })).toBeInTheDocument();
	expect(authoring.publication.value.editor.labels.work).toBe("Reviewer");
});

it("customizes the recorded pinned publication in a distinct draft without changing its retained identity", async () => {
	await ready(false);
	const pin = authoring.publication.value;
	fireEvent.click(screen.getByRole("button", { name: `Inspect ${pin.id} · revision ${pin.revision}` }));
	await screen.findByRole("region", { name: "Pinned metanode graph" });
	await waitFor(() => expect(screen.getByRole("button", { name: "Customize in a new draft" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Customize in a new draft" }));
	await waitFor(() => expect(screen.queryByRole("region", { name: "Pinned metanode graph" })).not.toBeInTheDocument());
	expect(screen.getByLabelText("Workflow name")).toHaveValue(authoring.clone.value.name);
	expect(requests.find((action) => action.action === "clone_publication")).toEqual({
		action: "clone_publication",
		id: pin.id,
		revision: pin.revision,
		name: `${pin.id} draft`,
	});
	expect(authoring.clone.value.id).not.toBe(pin.id);
	expect(authoring.clone.value.definition).toEqual(pin.definition);
	selectWork();
	fireEvent.input(screen.getByLabelText("Activity label"), { target: { value: "Independent copy" } });
	expect(pin.editor.labels.work).toBe("Reviewer");
	expect(requests.some((action) => ["update_draft", "publish", "start"].includes(String(action.action)))).toBe(false);
});

it("shows the real invalid typed-link diagnostic before allowing publication", async () => {
	await ready();
	selectWork();
	fireEvent.change(screen.getByRole("combobox", { name: "value source" }), { target: { value: "constant" } });
	fireEvent.change(screen.getByRole("textbox", { name: "value" }), { target: { value: '"wrong"' } });
	fireEvent.click(screen.getByRole("button", { name: "Validate links and inputs" }));
	await screen.findByText("/definition/input_bindings/0/value · binding.value: incompatible constant");
	expect(requests.find((action) => action.action === "validate")).toMatchObject({
		definition: { input_bindings: { "0": { value: { source: "constant", value: "wrong" } } } },
	});
	expect(screen.getByRole("button", { name: "Publish neutral draft" })).toBeDisabled();
	expect(requests.some((action) => action.action === "publish")).toBe(false);
});

it("renders the actual pure preparation and present sample input without save, publication or dispatch", async () => {
	await ready();
	fireEvent.input(screen.getByLabelText("Sample inputs (JSON)"), { target: { value: '{"value":7}' } });
	fireEvent.click(screen.getByRole("button", { name: "Preview sample IO and effect intents" }));
	const preview = await screen.findByRole("region", { name: "Resolved preview" });
	const blocks = within(preview).getAllByText((_, node) => node?.tagName === "PRE");
	expect(JSON.parse(blocks[0].textContent ?? "null")).toEqual(authoring.preview.value.activities[0].inputs);
	expect(JSON.parse(blocks[1].textContent ?? "null")).toEqual(authoring.preview.value.activities[0].effect_intent);
	expect(requests.find((action) => action.action === "preview")).toEqual({
		action: "preview",
		definition: authoring.draft.value.definition,
		inputs: { value: 7 },
		subject: null,
	});
	expect(
		requests.some((action) =>
			["create_draft", "update_draft", "publish", "start", "enable_trigger_binding", "replay_trigger"].includes(
				String(action.action),
			),
		),
	).toBe(false);
});

// Catches: the template button opening a blank or stale definition, so Save
// creates a draft without the built-in issue workflow graph and labels.
it("opens the built-in issue template as a new draft and saves its exact definition", async () => {
	await ready(false);
	fireEvent.click(await screen.findByRole("button", { name: "New from template: GitHub issue triage" }));
	fireEvent.click(screen.getByRole("button", { name: "Save neutral draft" }));
	await waitFor(() => expect(requests.some((action) => action.action === "create_draft")).toBe(true));
	expect(requests.find((action) => action.action === "create_draft")).toEqual({
		action: "create_draft",
		name: issueTemplate.name,
		definition: issueTemplate.definition,
		editor: issueTemplate.editor,
	});
});
