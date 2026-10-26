import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

let plan: { id: string; project: string; title: string; source: string; revision: number };
let failure: string;
const updates = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(([, args]) => (args as { action?: { action: string } })?.action?.action === "update_plan");
beforeEach(() => {
	plan = { id: "plan", project: "/plan-edit", title: "Original plan", source: "original.md", revision: 7 };
	failure = "";
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: { action: string; patch?: object } }).action;
		if (action.action === "list_plans") return { type: "plans", value: [{ ...plan }] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
		if (action.action === "update_plan") {
			if (failure) throw new Error(failure);
			plan = { ...plan, ...action.patch, revision: plan.revision + 1 };
			return { type: "plan", value: { ...plan } };
		}
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(cleanup);

it("catches plan saves omitting a field, losing the original revision, or failing to refresh the title", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	const form = within(ui.getByRole("form", { name: "Edit plan" }));
	fireEvent.input(form.getByLabelText("Plan title"), { target: { value: "Corrected plan" } });
	fireEvent.input(form.getByLabelText("Plan document or link"), { target: { value: "moved.md" } });
	fireEvent.click(form.getByRole("button", { name: "Save" }));
	await ui.findByRole("button", { name: "Corrected plan" });
	expect(updates()).toHaveLength(1);
	expect(updates()[0][1]).toEqual({
		project: "/plan-edit",
		action: {
			action: "update_plan",
			plan_id: "plan",
			expected_revision: 7,
			patch: { title: "Corrected plan", source: "moved.md" },
		},
	});
	expect(ui.queryByRole("form", { name: "Edit plan" })).toBeNull();
});

it("catches title-only edits resending an unchanged source and Cancel persisting a draft", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Discarded" } });
	fireEvent.click(ui.getByRole("button", { name: "Cancel" }));
	expect(updates()).toHaveLength(0);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Original plan");
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Only title" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(1));
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7, patch: { title: "Only title" } } });
	expect((updates()[0][1] as { action: { patch: object } }).action.patch).not.toHaveProperty("source");
});

it("catches a stale plan revision discarding the draft or silently rebasing its Save", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Keep my draft" } });
	plan = { ...plan, title: "Concurrent title", revision: 8 };
	failure = "stale plan revision: expected 7, current 8";
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect((await ui.findByRole("alert")).textContent).toContain("Cancel and reopen Edit plan");
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Keep my draft");
	expect(updates()).toHaveLength(1);
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7 } });
	fireEvent.click(ui.getByRole("button", { name: "Cancel" }));
	await ui.findByRole("button", { name: "Concurrent title" });
	fireEvent.click(ui.getByRole("button", { name: "Edit plan" }));
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Concurrent title");
	failure = "";
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Explicit new edit" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(2));
	expect(updates()[1][1]).toMatchObject({ action: { expected_revision: 8 } });
});

it("catches active-coordinator source refusals hiding the remedy or clearing the draft", async () => {
	failure =
		"plan source is in use by an active plan coordinator: run active; finish the coordinator or cancel the run before changing the source";
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.input(ui.getByLabelText("Plan document or link"), { target: { value: "moved.md" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect((await ui.findByRole("alert")).textContent).toBe(failure);
	expect((ui.getByLabelText("Plan document or link") as HTMLInputElement).value).toBe("moved.md");
	expect(updates()).toHaveLength(1);
});
