import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { type EditableStory, StoryEditForm } from "../../components/StoriesDialog/StoryEditForm";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const baseline: EditableStory = {
	id: "story",
	revision: 7,
	title: "Amend requirements",
	priority: 1,
	storyType: "fix",
	origin: { type: "native" },
	problem: "Original problem",
	risk: "high",
	criteria: ["Duplicate", "Duplicate"],
	fileScope: ["src/a.ts"],
	dependencies: [],
};
let status: string;
let claimSession: string | null;
let conflict: boolean;
const updates = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(([, args]) => (args as { action?: { action: string } })?.action?.action === "update_story");
beforeEach(() => {
	status = "ready";
	claimSession = null;
	conflict = false;
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans")
			return { type: "plans", value: [{ id: "plan", project: "/edit-test", title: "Plan", source: "plan.md" }] };
		if (action.action === "plan_view")
			return {
				type: "plan_view",
				value: {
					stories: [{ ...baseline, planId: "plan", checked: [true, true], status, claimSession, abandoned: false }],
					state: "draft",
					wontFixCount: 0,
					allCancelled: false,
				},
			};
		if (action.action === "update_story") {
			if (conflict) throw new Error("stale story revision: expected 7, current 8");
			return { type: "story", value: { ...baseline, revision: 8 } };
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

it("catches priority saves resending duplicate criteria and losing their checks", async () => {
	const ui = render(() => <StoriesDialog project="/edit-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: baseline.title });
	fireEvent.click(ui.getByRole("button", { name: "Edit" }));
	const form = within(ui.getByRole("form", { name: "Edit story" }));
	fireEvent.change(form.getByLabelText("Priority"), { target: { value: "3" } });
	fireEvent.click(form.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(1));
	expect(updates()[0][1]).toEqual({
		project: "/edit-test",
		action: { action: "update_story", story_id: "story", expected_revision: 7, patch: { priority: 3 } },
	});
	await waitFor(() => expect(ui.queryByRole("form", { name: "Edit story" })).toBeNull());
});

it("catches Cancel persisting a draft", async () => {
	const ui = render(() => <StoriesDialog project="/edit-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: baseline.title });
	fireEvent.click(ui.getByRole("button", { name: "Edit" }));
	fireEvent.input(ui.getByLabelText("Title", { exact: true }), { target: { value: "Not saved" } });
	fireEvent.click(ui.getByRole("button", { name: "Cancel" }));
	expect(updates()).toHaveLength(0);
	expect(ui.queryByRole("form", { name: "Edit story" })).toBeNull();
});

it("catches revision conflicts discarding the draft or silently retrying a newer revision", async () => {
	conflict = true;
	const ui = render(() => <StoriesDialog project="/edit-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: baseline.title });
	fireEvent.click(ui.getByRole("button", { name: "Edit" }));
	fireEvent.input(ui.getByLabelText("Title", { exact: true }), { target: { value: "Draft retained" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await ui.findByRole("alert");
	expect(ui.getByRole("alert").textContent).toContain("Cancel and reopen Edit");
	expect((ui.getByLabelText("Title", { exact: true }) as HTMLInputElement).value).toBe("Draft retained");
	expect(updates()).toHaveLength(1);
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7, patch: { title: "Draft retained" } } });
});

it.each(["qa", "in_progress", "review", "done", "wont_fix"])(
	"catches %s stories exposing an amendment form",
	async (value) => {
		status = value;
		const ui = render(() => <StoriesDialog project="/edit-test" onClose={() => {}} />);
		await ui.findByRole("heading", { name: baseline.title });
		expect(ui.queryByRole("button", { name: "Edit" })).toBeNull();
	},
);

it("catches claimed Ready stories exposing Edit", async () => {
	claimSession = "owner";
	const ui = render(() => <StoriesDialog project="/edit-test" onClose={() => {}} />);
	await ui.findByRole("heading", { name: baseline.title });
	expect(ui.queryByRole("button", { name: "Edit" })).toBeNull();
});

it("catches optional clearing, ordered requirements and dependency changes lost in form serialization", () => {
	const save = vi.fn();
	const ui = render(() => (
		<StoryEditForm
			story={{ ...baseline, dependencies: ["old"] }}
			candidates={[
				{ id: "old", title: "Old dependency" },
				{ id: "new", title: "New dependency" },
			]}
			busy={false}
			error=""
			onSave={save}
			onCancel={() => {}}
		/>
	));
	fireEvent.change(ui.getByLabelText("Story type"), { target: { value: "" } });
	fireEvent.input(ui.getByLabelText("Problem (optional)"), { target: { value: "" } });
	fireEvent.change(ui.getByLabelText("Risk (optional)"), { target: { value: "" } });
	fireEvent.change(ui.getByLabelText("Origin"), { target: { value: "plan_step" } });
	fireEvent.input(ui.getByLabelText("Plan step"), { target: { value: "Step 3" } });
	fireEvent.input(ui.getByLabelText("Acceptance criterion 1"), { target: { value: "First changed" } });
	fireEvent.input(ui.getByLabelText("Acceptance criterion 2"), { target: { value: "Second" } });
	fireEvent.click(ui.getByRole("button", { name: "Move criterion 2 up" }));
	fireEvent.input(ui.getByLabelText("File scope (one path per line)"), { target: { value: "src/b.ts\nsrc/c.ts" } });
	fireEvent.click(ui.getByLabelText("Old dependency"));
	fireEvent.click(ui.getByLabelText("New dependency"));
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect(save).toHaveBeenCalledWith({
		storyType: null,
		problem: null,
		risk: null,
		origin: { type: "plan_step", step: "Step 3" },
		criteria: ["Second", "First changed"],
		fileScope: ["src/b.ts", "src/c.ts"],
		dependencies: ["new"],
	});
});

it("catches editing one requirement splitting an untouched multiline criterion", () => {
	const save = vi.fn();
	const ui = render(() => (
		<StoryEditForm
			story={{ ...baseline, criteria: ["Verify output:\n  expected value", "Old requirement"] }}
			candidates={[]}
			busy={false}
			error=""
			onSave={save}
			onCancel={() => {}}
		/>
	));
	fireEvent.input(ui.getByLabelText("Acceptance criterion 2"), {
		target: { value: "Revised requirement" },
	});
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect(save).toHaveBeenCalledWith({ criteria: ["Verify output:\n  expected value", "Revised requirement"] });
});

it("catches priority-only edits normalizing untouched externally stored metadata", () => {
	const save = vi.fn();
	const ui = render(() => (
		<StoryEditForm
			story={{
				...baseline,
				title: " Historical title ",
				problem: "  Original problem\n",
				origin: { type: "plan_step", step: " Step 1 " },
			}}
			candidates={[]}
			busy={false}
			error=""
			onSave={save}
			onCancel={() => {}}
		/>
	));
	fireEvent.change(ui.getByLabelText("Priority"), { target: { value: "3" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect(save).toHaveBeenCalledWith({ priority: 3 });
});

it("catches adding, removing or reordering criteria normalizing untouched text", () => {
	const save = vi.fn();
	const ui = render(() => (
		<StoryEditForm
			story={{ ...baseline, criteria: ["  Keep this\r\n  indentation", "Remove this"] }}
			candidates={[]}
			busy={false}
			error=""
			onSave={save}
			onCancel={() => {}}
		/>
	));
	fireEvent.click(ui.getByRole("button", { name: "Remove criterion 2" }));
	fireEvent.click(ui.getByRole("button", { name: "Add criterion" }));
	fireEvent.input(ui.getByLabelText("Acceptance criterion 2"), { target: { value: "New\nrequirement" } });
	fireEvent.click(ui.getByRole("button", { name: "Move criterion 2 up" }));
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect(save).toHaveBeenCalledWith({ criteria: ["New\nrequirement", "  Keep this\r\n  indentation"] });
});
