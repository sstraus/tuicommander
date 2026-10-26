import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

let plan: { id: string; title: string; source: string; project: string; revision: number; archived: boolean };
let story: {
	id: string;
	planId: string;
	title: string;
	criteria: string[];
	checked: boolean[];
	dependencies: string[];
	priority: number;
	origin: { type: string };
	fileScope: string[];
	status: string;
	revision: number;
	archived: boolean;
	claimSession: string | null;
	abandoned: boolean;
};

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	plan = {
		id: "plan",
		title: "Archive plan fixture",
		source: "plan.md",
		project: "/archive",
		revision: 7,
		archived: false,
	};
	story = {
		id: "story",
		planId: plan.id,
		title: "Archive story fixture",
		criteria: ["Preserve this criterion"],
		checked: [false],
		dependencies: [],
		priority: 2,
		origin: { type: "native" },
		fileScope: [],
		status: "ready",
		revision: 11,
		archived: false,
		claimSession: null,
		abandoned: false,
	};
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: Record<string, unknown> }).action;
		switch (action.action) {
			case "list_plans":
				return { type: "plans", value: !!action.archived === plan.archived ? [{ ...plan }] : [] };
			case "list_plan_sources":
				return { type: "plan_sources", value: [] };
			case "plan_view":
			case "filtered_plan_view": {
				const archived = !!(action.filter as { archived?: boolean } | undefined)?.archived;
				const stories = archived === story.archived ? [{ ...story }] : [];
				return {
					type: "plan_view",
					value: {
						stories,
						visibleStoryIds: stories.map((item) => item.id),
						columns: [],
						state: "active",
						wontFixCount: 0,
						allCancelled: false,
					},
				};
			}
			case "archive_story":
			case "unarchive_story":
				story = { ...story, archived: action.action === "archive_story", revision: story.revision + 1 };
				return { type: "story", value: { ...story } };
			case "archive_plan":
			case "unarchive_plan":
				plan = { ...plan, archived: action.action === "archive_plan", revision: plan.revision + 1 };
				return { type: "plan", value: { ...plan } };
			default:
				throw new Error(`Unexpected archive test action ${action.action}`);
		}
	});
});
afterEach(cleanup);

it("catches archive/restore buttons sending stale story revisions or hiding the restore path", async () => {
	const ui = render(() => <StoriesDialog project="/archive" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Archive story" }));
	await waitFor(() => expect(ui.queryByRole("button", { name: "Archive story" })).toBeNull());
	expect(invoke).toHaveBeenCalledWith("story_action_command", {
		project: "/archive",
		action: { action: "archive_story", story_id: "story", expected_revision: 11 },
	});
	fireEvent.click(ui.getByLabelText("Archived stories"));
	const restore = await ui.findByRole("button", { name: "Restore story" });
	expect(ui.queryByRole("button", { name: "Start work" })).toBeNull();
	fireEvent.click(restore);
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/archive",
			action: { action: "unarchive_story", story_id: "story", expected_revision: 12 },
		}),
	);
	await waitFor(() => expect(ui.queryByRole("button", { name: "Restore story" })).toBeNull());
	fireEvent.click(ui.getByLabelText("Archived stories"));
	await ui.findByRole("button", { name: "Archive story" });
	expect(ui.getByText("Preserve this criterion")).toBeTruthy();
});

it("catches archived plans having no restore route or enabling new work before restoration", async () => {
	const ui = render(() => <StoriesDialog project="/archive" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Archive plan" }));
	await waitFor(() => expect(ui.queryByRole("button", { name: "Archive plan" })).toBeNull());
	expect(invoke).toHaveBeenCalledWith("story_action_command", {
		project: "/archive",
		action: { action: "archive_plan", plan_id: "plan", expected_revision: 7 },
	});
	fireEvent.click(ui.getByLabelText("Archived plans"));
	const restore = await ui.findByRole("button", { name: "Restore plan" });
	expect((ui.getByRole("button", { name: "New story" }) as HTMLButtonElement).disabled).toBe(true);
	expect(ui.queryByRole("button", { name: "Start work" })).toBeNull();
	fireEvent.click(restore);
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/archive",
			action: { action: "unarchive_plan", plan_id: "plan", expected_revision: 8 },
		}),
	);
});
