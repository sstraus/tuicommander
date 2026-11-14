import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { blankDefinition, type NeutralPublication } from "../../components/WorkflowDesigner/neutralClient";
import { TriggerBindings } from "../../components/WorkflowDesigner/TriggerBindings";
import type { NeutralTriggerBinding, TriggerBindingDraft } from "../../components/WorkflowDesigner/triggerTypes";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import { repositoriesStore } from "../../stores/repositories";
import snapshot from "../fixtures/workflow/github-issue-opened.json";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});
const publication: NeutralPublication = {
	format_version: 1,
	id: "issue-workflow",
	project: "/repo",
	revision: 3,
	definition: {
		...blankDefinition(),
		inputs: { fields: { issue_title: { value_type: "string", required: true, default: null } } },
	},
	editor: { labels: {}, presets: {} },
	pins: { pins: [] },
	subflow_pins: [],
};
const calls: Record<string, unknown>[] = [];
let saved: NeutralTriggerBinding[];
let mutationError: boolean;
beforeEach(() => {
	calls.length = 0;
	saved = [];
	mutationError = false;
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command !== "workflow_neutral_action") return [];
		const action = args?.action as Record<string, unknown>;
		calls.push(action);
		switch (action.action) {
			case "trigger_bindings":
				return { type: "trigger_bindings", value: saved };
			case "trigger_schema":
				return { type: "schema", value: snapshot.schema };
			case "preview_trigger_mapping":
				return {
					type: "trigger_preview",
					value: { inputs: { issue_title: "Found a bug" }, subject: null, overlaps: { "existing-issue": 7 } },
				};
			case "save_trigger_binding": {
				const binding = { draft: action.draft as TriggerBindingDraft, revision: 1, enabled: false };
				saved = [binding];
				if (mutationError)
					throw new Error(
						"trigger binding issue-inbox committed at revision 1 but subscription configuration failed: account unavailable",
					);
				return { type: "trigger_binding", value: binding };
			}
			case "enable_trigger_binding": {
				const binding = { ...saved[0], revision: 2, enabled: true };
				saved = [binding];
				return { type: "trigger_binding", value: binding };
			}
			default:
				throw new Error(`Unexpected action ${action.action}`);
		}
	});
});
async function editBinding() {
	render(() => <TriggerBindings project="/repo" publications={[publication]} />);
	await waitFor(() => expect(calls.some((action) => action.action === "trigger_schema")).toBe(true));
	fireEvent.input(screen.getByLabelText("Binding ID"), { target: { value: "issue-inbox" } });
	fireEvent.change(screen.getByLabelText("Published workflow"), { target: { value: "issue-workflow@3" } });
	fireEvent.input(screen.getByLabelText("Resource prefix"), { target: { value: "github:octo-org/example/issue/" } });
	await screen.findByRole("option", { name: "title · string" });
	fireEvent.change(screen.getByLabelText("Workflow inputs: issue_title"), { target: { value: "title" } });
	fireEvent.input(screen.getByLabelText("Normalized event snapshot"), { target: { value: JSON.stringify(snapshot) } });
}
async function preview() {
	await waitFor(() => expect(screen.getByRole("button", { name: "Preview trigger mapping" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Preview trigger mapping" }));
	await screen.findByRole("region", { name: "Trigger mapping preview" });
}

it("selects a pinned workflow and previews the recorded issue payload with overlaps without activating", async () => {
	await editBinding();
	await preview();
	expect(screen.getByLabelText("Workflow inputs: issue_title")).toHaveValue("title");
	expect(calls.filter((action) => action.action === "trigger_schema")).toHaveLength(1);
	expect(screen.getByText("existing-issue · revision 7")).toBeInTheDocument();
	expect(screen.getByText(/Found a bug/)).toBeInTheDocument();
	expect(calls.find((action) => action.action === "preview_trigger_mapping")).toMatchObject({
		draft: {
			source: "github",
			kind: "issue",
			publication: { project: "/repo", id: "issue-workflow", revision: 3 },
			mapping: { inputs: { issue_title: "title" }, subject: {} },
		},
		snapshot,
	});
	expect(screen.getByRole("button", { name: "Enable binding" })).toBeDisabled();
	expect(
		calls.some((action) =>
			["save_trigger_binding", "enable_trigger_binding", "replay_trigger", "start"].includes(String(action.action)),
		),
	).toBe(false);
});

it("requires a fresh preview and exact overlap acknowledgement after saving before enable", async () => {
	await editBinding();
	await preview();
	fireEvent.click(screen.getByRole("button", { name: "Save disabled binding" }));
	await waitFor(() => expect(screen.getByText("Revision 1 · disabled")).toBeInTheDocument());
	expect(screen.queryByRole("region", { name: "Trigger mapping preview" })).not.toBeInTheDocument();
	await preview();
	fireEvent.click(screen.getByRole("checkbox"));
	fireEvent.click(screen.getByRole("button", { name: "Enable binding" }));
	await waitFor(() => expect(calls.some((action) => action.action === "enable_trigger_binding")).toBe(true));
	expect(calls.find((action) => action.action === "enable_trigger_binding")).toEqual({
		action: "enable_trigger_binding",
		id: "issue-inbox",
		expected_revision: 1,
		enabled: true,
		acknowledged_overlaps: { "existing-issue": 7 },
	});
});

it("invalidates acknowledged overlap revisions when the draft or snapshot changes", async () => {
	await editBinding();
	fireEvent.click(screen.getByRole("button", { name: "Save disabled binding" }));
	await waitFor(() => expect(screen.getByText("Revision 1 · disabled")).toBeInTheDocument());
	await preview();
	fireEvent.click(screen.getByRole("checkbox"));
	expect(screen.getByRole("button", { name: "Enable binding" })).not.toBeDisabled();
	fireEvent.input(screen.getByLabelText("Normalized event snapshot"), { target: { value: "{}" } });
	expect(screen.getByRole("button", { name: "Enable binding" })).toBeDisabled();
	expect(screen.queryByRole("region", { name: "Trigger mapping preview" })).not.toBeInTheDocument();
	await preview();
	fireEvent.click(screen.getByRole("checkbox"));
	fireEvent.input(screen.getByLabelText("Resource prefix"), { target: { value: "github:other/" } });
	expect(screen.getByRole("button", { name: "Enable binding" })).toBeDisabled();
});

it("rereads durable metadata when subscription configuration fails after a committed save", async () => {
	mutationError = true;
	await editBinding();
	fireEvent.click(screen.getByRole("button", { name: "Save disabled binding" }));
	await screen.findByRole("alert");
	expect(screen.getByRole("alert")).toHaveTextContent("committed at revision 1 but subscription configuration failed");
	expect(calls.filter((action) => action.action === "trigger_bindings")).toHaveLength(2);
	fireEvent.click(screen.getByRole("button", { name: "Load issue-inbox · revision 1 · disabled" }));
	expect(screen.getByText("Revision 1 · disabled")).toBeInTheDocument();
	expect(calls.some((action) => action.action === "enable_trigger_binding")).toBe(false);
});

// A slow preview must not restore acknowledgement after repository metadata changed.
it("discards a preview response invalidated by a newer binding refresh", async () => {
	await editBinding();
	let release: ((value: unknown) => void) | undefined;
	vi.mocked(invoke).mockImplementationOnce(
		() =>
			new Promise((resolve) => {
				release = resolve;
			}),
	);
	fireEvent.click(screen.getByRole("button", { name: "Preview trigger mapping" }));
	await waitFor(() => expect(release).toBeDefined());
	repositoriesStore.bumpRevision("/repo");
	await waitFor(() => expect(calls.filter((action) => action.action === "trigger_bindings")).toHaveLength(2));
	release?.({ type: "trigger_preview", value: { inputs: {}, subject: null, overlaps: { stale: 1 } } });
	await waitFor(() => expect(screen.getByRole("button", { name: "Preview trigger mapping" })).not.toBeDisabled());
	expect(screen.queryByRole("region", { name: "Trigger mapping preview" })).not.toBeInTheDocument();
	expect(screen.getByRole("button", { name: "Enable binding" })).toBeDisabled();
});
