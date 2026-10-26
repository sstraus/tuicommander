import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "plan", project: "/filter-test", title: "Filter plan", source: "plan.md" };
const ready = {
	id: "ready",
	planId: "plan",
	title: "Open feature",
	storyType: "feature",
	criteria: ["Open criterion"],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
};
const done = {
	...ready,
	id: "done",
	title: "Closed task",
	storyType: "task",
	criteria: ["Closed criterion"],
	checked: [true],
	status: "done",
};
const states = ["backlog", "ready", "in_progress", "review", "qa", "blocked", "wont_fix", "done"];
const view = (ids: string[]) => ({
	type: "plan_view",
	value: {
		stories: [ready, done],
		state: "active",
		wontFixCount: 0,
		allCancelled: false,
		visibleStoryIds: ids,
		columns: states.map((status) => ({ status, storyIds: ids.filter((id) => id === status) })),
	},
});

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (
			args as {
				action: { action: string; filter?: { visibility: string; storyType: string | null; untypedOnly: boolean } };
			}
		).action;
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "plan_view") return view(["ready", "done"]);
		if (action.action === "filtered_plan_view")
			return view(action.filter?.untypedOnly ? [] : action.filter?.visibility === "closed" ? ["done"] : ["ready"]);
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(() => {
	cleanup();
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
});

it("catches ignoring the Rust projection or resetting combined filters when switching view and reopening", async () => {
	const first = render(() => <StoriesDialog project="/filter-test" onClose={() => {}} />);
	const storyList = within(first.getByRole("region", { name: "Stories" }));
	await storyList.findByRole("button", { name: /Closed task/ });
	fireEvent.change(first.getByLabelText("Status"), { target: { value: "open" } });
	await waitFor(() => expect(storyList.queryByRole("button", { name: /Closed task/ })).toBeNull());
	fireEvent.change(first.getByLabelText("Story type"), { target: { value: "feature" } });
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/filter-test",
			action: {
				action: "filtered_plan_view",
				plan_id: "plan",
				filter: { visibility: "open", storyType: "feature", untypedOnly: false },
			},
		}),
	);
	fireEvent.click(first.getByRole("tab", { name: /^Kanban$/ }));
	const board = within(first.getByRole("region", { name: "Kanban" }));
	expect(board.getAllByRole("region")).toHaveLength(8);
	expect(board.queryByRole("button", { name: /Closed task/ })).toBeNull();
	fireEvent.click(board.getByRole("button", { name: /Open feature/ }));
	expect(within(first.getByRole("region", { name: "Story details" })).getByText("Open criterion")).toBeTruthy();
	first.unmount();
	const reopened = render(() => <StoriesDialog project="/filter-test" onClose={() => {}} />);
	expect((reopened.getByLabelText("Status") as HTMLSelectElement).value).toBe("open");
	expect((reopened.getByLabelText("Story type") as HTMLSelectElement).value).toBe("feature");
	expect(reopened.getByRole("tab", { name: /^Kanban$/ }).getAttribute("aria-selected")).toBe("true");
	await waitFor(() =>
		expect(
			within(reopened.getByRole("region", { name: "Kanban" })).getByRole("button", { name: /Open feature/ }),
		).toBeTruthy(),
	);
	fireEvent.change(reopened.getByLabelText("Story type"), { target: { value: "all" } });
	fireEvent.change(reopened.getByLabelText("Status"), { target: { value: "closed" } });
	await waitFor(() =>
		expect(
			within(reopened.getByRole("region", { name: "Kanban" })).getByRole("button", { name: /Closed task/ }),
		).toBeTruthy(),
	);
	fireEvent.click(reopened.getByRole("tab", { name: /^List$/ }));
	expect(
		within(reopened.getByRole("region", { name: "Stories" })).queryByRole("button", { name: /Open feature/ }),
	).toBeNull();
});

it("catches treating the untyped filter as all types or sending it as a fabricated story kind", async () => {
	const ui = render(() => <StoriesDialog project="/filter-test" onClose={() => {}} />);
	await ui.findByRole("button", { name: /Open feature/ });
	fireEvent.change(ui.getByLabelText("Story type"), { target: { value: "untyped" } });
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/filter-test",
			action: {
				action: "filtered_plan_view",
				plan_id: "plan",
				filter: { visibility: "all", storyType: null, untypedOnly: true },
			},
		}),
	);
});

it("catches dropping an explicitly selected story type from the create request", async () => {
	const original = vi.mocked(invoke).getMockImplementation()!;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const action = (args as { action?: { action: string } })?.action;
		if (action?.action === "create_story") return { type: "story", value: ready };
		return original(command, args);
	});
	const ui = render(() => <StoriesDialog project="/filter-test" onClose={() => {}} />);
	await ui.findByRole("button", { name: /Open feature/ });
	fireEvent.click(ui.getByRole("button", { name: "New story" }));
	fireEvent.input(ui.getByLabelText("Title"), { target: { value: "Typed creation" } });
	fireEvent.input(ui.getByLabelText("Acceptance criteria, one per line"), { target: { value: "Type survives" } });
	const typeControls = ui.getAllByLabelText("Story type");
	fireEvent.change(typeControls[1], { target: { value: "fix" } });
	fireEvent.click(ui.getByRole("button", { name: "Create story" }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/filter-test",
			action: {
				action: "create_story",
				input: {
					planId: "plan",
					title: "Typed creation",
					criteria: ["Type survives"],
					priority: 2,
					storyType: "fix",
					origin: { type: "native" },
					fileScope: [],
				},
			},
		}),
	);
	await waitFor(() => expect(ui.queryByRole("button", { name: "Create story" })).toBeNull());
});

it("catches a hand-rolled view switch bypassing the shared form controls", async () => {
	const ui = render(() => <StoriesDialog project="/filter-test" onClose={() => {}} />);
	await ui.findByRole("button", { name: /Open feature/ });
	const tabs = within(ui.getByRole("tablist", { name: "Story view" }));
	const list = tabs.getByRole("tab", { name: "List" });
	const kanban = tabs.getByRole("tab", { name: "Kanban" });
	expect(list).toHaveClass("segment");
	expect(kanban).toHaveClass("segment");
	expect(list).toHaveAttribute("aria-selected", "true");
	fireEvent.keyDown(list, { key: "ArrowRight" });
	expect(kanban).toHaveAttribute("aria-selected", "true");
	expect(storiesUi.view()).toBe("kanban");
	expect(tabs.queryByRole("button")).toBeNull();
});
