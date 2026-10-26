import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import type { EditableStory } from "../../components/StoriesDialog/StoryEditForm";
import { invoke } from "../../invoke";
import type { StoryStatus } from "../../types/stories.gen";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: {
			terminals: {
				worker: { tuicSession: "tab-1", sessionId: "session-1", name: "API worker" },
			},
		},
	},
}));
const story: EditableStory & {
	planId: string;
	checked: boolean[];
	status: StoryStatus;
	claimSession: string | null;
	abandoned: boolean;
} = {
	id: "s1",
	planId: "p1",
	title: "Build API",
	criteria: ["Works"],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "plan_step", step: "Step 4" },
	fileScope: [],
	status: "in_progress",
	revision: 7,
	claimSession: "session-1",
	abandoned: false,
};
beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans")
			return { type: "plans", value: [{ id: "p1", title: "API plan", project: "/repo" }] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
		if (action.action === "get_story" || action.action === "create_story") return { type: "story", value: story };
		if (action.action === "transition_history")
			return {
				type: "transitions",
				value: [
					{ storyId: "s1", revision: 7, command: "block", actor: { type: "managed_session", session_id: "session-1" } },
				],
			};
		throw new Error(`unexpected action ${action.action}`);
	});
});
it("story_detail_shows_claim_session_and_history — catches: detail that drops fields the API returns", async () => {
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	await screen.findByRole("heading", { name: "Build API" });
	expect(await screen.findByText("Step 4")).toBeTruthy();
	const metadata = within(screen.getByRole("region", { name: "Story metadata" }));
	expect(metadata.getByText("API worker")).toBeTruthy();
	expect(metadata.getByText("session-1")).toBeTruthy();
	await waitFor(() => expect(screen.getByRole("region", { name: "Transition history" })).toHaveTextContent("block"));
	expect(metadata.getByText("Revision 7")).toBeTruthy();
});
it("creates a plan-step story — catches: silently saving a selected plan step as native", async () => {
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	await screen.findByRole("heading", { name: "Build API" });
	fireEvent.click(screen.getByRole("button", { name: "New story" }));
	fireEvent.change(screen.getByLabelText("Origin"), { target: { value: "plan_step" } });
	fireEvent.input(screen.getByLabelText("Plan step"), { target: { value: "Step 5" } });
	fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Next API" } });
	fireEvent.input(screen.getByLabelText("Acceptance criteria, one per line"), { target: { value: "Works" } });
	fireEvent.click(screen.getByRole("button", { name: "Create story" }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: {
				action: "create_story",
				input: expect.objectContaining({ origin: { type: "plan_step", step: "Step 5" } }),
			},
		}),
	);
});

it("does not show another story's late history — catches: a slow selection overwriting the current detail", async () => {
	const { createSignal } = await import("solid-js");
	const { StoryMetadata } = await import("../../components/StoriesDialog/StoryMetadata");
	let finishFirst: ((value: unknown) => void) | undefined;
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action: { story_id: string } }).action;
		if (action.story_id === "s1")
			return new Promise((resolve) => {
				finishFirst = resolve;
			});
		return { type: "transitions", value: [] };
	});
	const [selected, setSelected] = createSignal(story);
	render(() => <StoryMetadata project="/repo" story={selected()} />);
	await waitFor(() => expect(finishFirst).toBeTypeOf("function"));
	setSelected({ ...story, id: "s2", claimSession: null, origin: { type: "native" } });
	await screen.findByText("No transitions recorded.");
	finishFirst?.({
		type: "transitions",
		value: [{ storyId: "s1", revision: 7, command: "block", actor: { type: "human" } }],
	});
	await Promise.resolve();
	expect(screen.queryByText("block")).toBeNull();
	expect(screen.getByText("Unclaimed")).toBeTruthy();
	expect(screen.queryByText("Step 4")).toBeNull();
});

it("can retry failed history — catches: presenting a failed history read as an empty successful read", async () => {
	const { StoryMetadata } = await import("../../components/StoriesDialog/StoryMetadata");
	vi.mocked(invoke)
		.mockRejectedValueOnce(new Error("History offline"))
		.mockResolvedValueOnce({ type: "transitions", value: [] });
	render(() => <StoryMetadata project="/repo" story={story} />);
	expect(await screen.findByRole("alert")).toHaveTextContent("History offline");
	expect(screen.queryByText("No transitions recorded.")).toBeNull();
	fireEvent.click(screen.getByRole("button", { name: "Refresh history" }));
	await screen.findByText("No transitions recorded.");
	expect(screen.queryByRole("alert")).toBeNull();
});
