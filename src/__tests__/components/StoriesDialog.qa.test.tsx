import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "plan", project: "/qa-test", title: "QA plan", source: "plan.md" };
const qa = {
	id: "qa-story",
	planId: "plan",
	title: "Check device",
	criteria: ["[MANUAL] Hardware"],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status: "qa",
	revision: 7,
	claimSession: null,
	abandoned: false,
};
const statuses = ["backlog", "ready", "in_progress", "review", "qa", "blocked", "wont_fix", "done"];
let story = { ...qa };

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	story = { ...qa, checked: [false] };
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: { action: string; command?: unknown } }).action;
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "plan_view" || action.action === "filtered_plan_view")
			return {
				type: "plan_view",
				value: {
					stories: [story],
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
					visibleStoryIds: [story.id],
					columns: statuses.map((status) => ({ status, storyIds: status === story.status ? [story.id] : [] })),
				},
			};
		if (action.action === "transition") {
			story = {
				...story,
				revision: story.revision + 1,
				checked: action.command === "approve" ? story.checked : [true],
				status: action.command === "reject_qa" ? "in_progress" : action.command === "approve" ? "done" : "qa",
			};
			return { type: "story", value: story };
		}
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(() => {
	cleanup();
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
});

it("catches QA labels disappearing from list/filter/Kanban and QA criteria staying disabled", async () => {
	const ui = render(() => <StoriesDialog project="/qa-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: "Check device" });
	const details = within(ui.getByRole("region", { name: "Story details" }));
	expect(details.getByText("QA")).toBeTruthy();
	expect((details.getByRole("checkbox") as HTMLInputElement).disabled).toBe(false);
	fireEvent.change(ui.getByLabelText("Status"), { target: { value: "qa" } });
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/qa-test",
			action: {
				action: "filtered_plan_view",
				plan_id: "plan",
				filter: { visibility: "qa", storyType: null, untypedOnly: false },
			},
		}),
	);
	fireEvent.click(ui.getByRole("tab", { name: /^Kanban$/ }));
	const column = within(ui.getByRole("region", { name: "QA" }));
	expect(column.getByRole("button", { name: /Check device/ })).toBeTruthy();
	fireEvent.click(details.getByRole("checkbox"));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/qa-test",
			action: { action: "transition", story_id: "qa-story", expected_revision: 7, command: { check_criterion: 0 } },
		}),
	);
	await waitFor(() => expect((details.getByRole("checkbox") as HTMLInputElement).checked).toBe(true));
	await waitFor(() =>
		expect((details.getByRole("button", { name: "QA passed" }) as HTMLButtonElement).disabled).toBe(false),
	);
	fireEvent.click(details.getByRole("button", { name: "QA passed" }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/qa-test",
			action: { action: "transition", story_id: "qa-story", expected_revision: 8, command: "approve" },
		}),
	);
});

it("catches QA failure invoking review rejection or losing the current revision", async () => {
	storiesUi.restoreControls({ visibility: "qa", view: "kanban" });
	const ui = render(() => <StoriesDialog project="/qa-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: "Check device" });
	expect((ui.getByLabelText("Status") as HTMLSelectElement).value).toBe("qa");
	fireEvent.click(ui.getByRole("button", { name: "QA failed" }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/qa-test",
			action: { action: "transition", story_id: "qa-story", expected_revision: 7, command: "reject_qa" },
		}),
	);
});
