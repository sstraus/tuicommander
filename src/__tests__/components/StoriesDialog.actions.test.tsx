import "../fixtures/dialogActivityHydration";
import { cleanup, fireEvent, render, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "p1", project: "/actions", title: "Action plan", source: "action.md", revision: 1 };
const story = {
	id: "s1",
	planId: "p1",
	title: "Action story",
	criteria: ["Works"],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: "native",
	fileScope: [],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
};

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
		const action = (args as { action: { action: string } }).action.action;
		if (action === "list_plans") return { type: "plans", value: [plan] };
		if (action === "plan_view")
			return {
				type: "plan_view",
				value: {
					stories: [story, { ...story, id: "s2", title: "Other story" }],
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
				},
			};
		if (action === "list_plan_sources") return { type: "plan_sources", value: [] };
		if (action === "transition_history") return { type: "transitions", value: [] };
		if (action === "list_notes") return { type: "notes", value: [] };
		throw new Error(`Unexpected action ${action}`);
	});
});
afterEach(cleanup);

it("catches New story Cancel separated from the bottom primary action", async () => {
	const ui = render(() => <StoriesDialog project="/actions" onClose={() => {}} />);
	await ui.findByRole("heading", { name: "Action story" });
	fireEvent.click(ui.getByRole("button", { name: "New story" }));
	const create = ui.getByRole("button", { name: "Create story" });
	const cancel = ui.getByRole("button", { name: "Cancel" });
	expect(create.parentElement).toBe(cancel.parentElement);
	expect(Array.from(create.parentElement!.children)).toEqual([create, cancel]);
	expect(create.closest("form")?.lastElementChild).toBe(create.parentElement);
	fireEvent.click(cancel);
	expect(ui.queryByRole("button", { name: "Create story" })).toBeNull();
	fireEvent.click(ui.getByRole("button", { name: "Edit plan" }));
	const form = within(ui.getByRole("form", { name: "Edit plan" }));
	const save = form.getByRole("button", { name: "Save" });
	expect(Array.from(save.parentElement!.children)).toEqual([save, form.getByRole("button", { name: "Cancel" })]);
});

it("catches story Edit detached from the story header", async () => {
	const ui = render(() => <StoriesDialog project="/actions" onClose={() => {}} />);
	const title = await ui.findByRole("heading", { name: "Action story" });
	const header = within(title.parentElement!.parentElement!);
	fireEvent.click(header.getByRole("button", { name: "Edit" }));
	expect(ui.getByRole("button", { name: "Save" })).toBeTruthy();
});

it("catches Add dependency disabled without an accessible explanation or with a stale reason after selection", async () => {
	const ui = render(() => <StoriesDialog project="/actions" onClose={() => {}} />);
	const add = (await ui.findByRole("button", { name: "Add dependency" })) as HTMLButtonElement;
	expect(add.disabled).toBe(true);
	expect(add.title).toBe("Select a story to add a dependency.");
	const hint = document.getElementById(add.getAttribute("aria-describedby") ?? "");
	expect(hint?.textContent).toBe(add.title);
	fireEvent.change(ui.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s2" } });
	expect(add.disabled).toBe(false);
	expect(add.title).toBe("");
	expect(hint?.textContent).toBe("");
});
