import "../fixtures/dialogActivityHydration";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import type { EditableStory } from "../../components/StoriesDialog/StoryEditForm";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import type { StoryStatus } from "../../types/stories.gen";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

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
	storiesUi.setVisibility("all");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
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
	await waitFor(() => expect(screen.getByRole("region", { name: "Transition history" })).toHaveTextContent("Blocked"));
	expect(metadata.getByText("Revision 7")).toBeTruthy();
});
it("transition history renders human labels — catches: raw JSON, zero-based criteria and closed-session UUIDs", async () => {
	const { StoryMetadata } = await import("../../components/StoriesDialog/StoryMetadata");
	const closedId = "5a18a948-8050-4185-838c-e7b879fb3884";
	vi.mocked(invoke).mockResolvedValue({
		type: "transitions",
		value: [
			{
				storyId: "s1",
				revision: 3,
				command: { check_criterion: 0 },
				actor: { type: "managed_session", session_id: closedId },
			},
		],
	});
	render(() => <StoryMetadata project="/repo" story={story} />);
	const history = within(screen.getByRole("region", { name: "Transition history" }));
	expect(await history.findByText("Criterion 1 checked: Works")).toBeTruthy();
	expect(history.getByText("5a18a948")).toHaveAttribute("title", closedId);
	expect(history.queryByText(closedId)).toBeNull();
	expect(history.queryByText('{"check_criterion":0}')).toBeNull();
});

it("labels every command and preserves actor identities — catches: omitted variants, missing criteria and JSON fallback", async () => {
	const { StoryMetadata } = await import("../../components/StoriesDialog/StoryMetadata");
	const cases: [string | Record<string, unknown>, string][] = [
		["start_manual", "Started manually"],
		["submit_qa", "Submitted for QA"],
		["reject_qa", "QA rejected"],
		["submit_review", "Submitted for review"],
		["approve", "Approved"],
		["reject_review", "Review rejected"],
		["block", "Blocked"],
		["unblock", "Unblocked"],
		["wont_fix", "Cancelled"],
		[{ uncheck_criterion: 0 }, "Criterion 1 unchecked: Works"],
		[{ check_criterion: 99 }, "Criterion 100 checked"],
		[{ set_criterion_manual: { index: 0, manual: true } }, "Criterion 1 marked manual: Works"],
		[{ set_criterion_manual: { index: 0, manual: false } }, "Criterion 1 no longer manual: Works"],
		[{ future_command: { secret: 123 } }, "Future command"],
		["future_action", "Future action"],
	];
	vi.mocked(invoke).mockResolvedValue({
		type: "transitions",
		value: cases.map(([command], i) => ({
			storyId: "s1",
			revision: i + 1,
			command,
			actor: i === 0 ? { type: "managed_session", session_id: "session-1" } : { type: i === 1 ? "human" : "local_api" },
		})),
	});
	render(() => <StoryMetadata project="/repo" story={story} />);
	const history = within(screen.getByRole("region", { name: "Transition history" }));
	await history.findByText("Started manually");
	for (const [, label] of cases) expect.soft(history.queryByText(label), label).toBeTruthy();
	expect(history.getByText("API worker")).toHaveAttribute("title", "session-1");
	expect(history.getByText("· Human")).toBeTruthy();
	expect(history.getAllByText("· Local API").length).toBeGreaterThan(0);
	expect(screen.getByRole("region", { name: "Transition history" }).textContent).not.toContain("{");
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
	expect(screen.queryByText("Blocked")).toBeNull();
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
