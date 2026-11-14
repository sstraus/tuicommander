import "../fixtures/dialogActivityHydration";
import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = {
	id: "p1",
	project: "/phase-moves",
	title: "Release",
	source: "release.md",
	revision: 1,
	archived: false,
};
const initial = {
	id: "s1",
	planId: "p1",
	title: "Deliver phase moves",
	storyType: "feature",
	criteria: ["Works"],
	checked: [false],
	dependencies: [],
	priority: 1,
	origin: { type: "native" },
	fileScope: [],
	status: "ready",
	revision: 7,
	claimSession: null,
	abandoned: false,
	archived: false,
};
let story = { ...initial };
let reject = "";
const transition = vi.fn();
const states = ["backlog", "ready", "in_progress", "review", "qa", "blocked", "wont_fix", "done"];
beforeEach(() => {
	story = { ...initial };
	reject = "";
	transition.mockReset();
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("kanban");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (command === "workflow_neutral_action") return { type: "delivery_runs", value: [] };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
		const action = (args as { action: Record<string, unknown> }).action;
		switch (action.action) {
			case "list_plans":
				return { type: "plans", value: [plan] };
			case "plan_view":
				return {
					type: "plan_view",
					value: {
						stories: [story],
						state: "active",
						wontFixCount: 0,
						allCancelled: false,
						columns: states.map((status) => ({ status, storyIds: status === story.status ? [story.id] : [] })),
					},
				};
			case "available_transitions":
				return {
					type: "available_transitions",
					value: {
						revision: story.revision,
						moves:
							story.status === "ready"
								? [
										{ status: "in_progress", command: "start_manual" },
										{ status: "blocked", command: "block" },
										{ status: "wont_fix", command: "wont_fix" },
									]
								: [],
					},
				};
			case "transition_history":
				return { type: "transitions", value: [] };
			case "list_notes":
				return { type: "notes", value: [] };
			case "transition":
				transition(action);
				if (reject) throw new Error(reject);
				story = { ...story, status: "in_progress", revision: story.revision + 1 };
				return { type: "story", value: story };
			default:
				throw new Error(`Unexpected action ${action.action}`);
		}
	});
});
afterEach(() => {
	cleanup();
	storiesUi.setView("list");
	vi.restoreAllMocks();
});

async function board() {
	const ui = render(() => <StoriesDialog project="/phase-moves" onClose={() => {}} />);
	const card = await ui.findByRole("button", { name: /Deliver phase moves/ });
	const lanes = within(ui.getByRole("region", { name: "Kanban" }));
	const target = lanes.getByRole("region", { name: "In progress" });
	const illegal = lanes.getByRole("region", { name: "Done" });
	vi.spyOn(target, "getBoundingClientRect").mockReturnValue({ left: 200, right: 350, top: 0, bottom: 300 } as DOMRect);
	vi.spyOn(illegal, "getBoundingClientRect").mockReturnValue({ left: 400, right: 550, top: 0, bottom: 300 } as DOMRect);
	return { ui, card, target, illegal, lanes };
}
function pointer(target: Element | Document, type: string, x: number) {
	const event = new Event(type, { bubbles: true, cancelable: true });
	Object.assign(event, { pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, clientX: x, clientY: 20 });
	fireEvent(target, event);
}
function drag(card: HTMLElement, x: number) {
	pointer(card, "pointerdown", 20);
	pointer(document, "pointermove", x);
	pointer(document, "pointerup", x);
}
it("catches a drop moving only UI state instead of submitting the displayed revision", async () => {
	const { ui, card } = await board();
	drag(card, 250);
	await waitFor(() =>
		expect(transition).toHaveBeenCalledExactlyOnceWith({
			action: "transition",
			story_id: "s1",
			expected_revision: 7,
			command: "start_manual",
		}),
	);
	await waitFor(() =>
		expect(
			within(ui.getByRole("region", { name: "In progress" })).getByRole("button", { name: /Deliver phase moves/ }),
		).toBeTruthy(),
	);
	expect(ui.queryByRole("alert")).toBeNull();
});
it("catches an illegal column bypassing backend-provided moves", async () => {
	const { ui, card, lanes } = await board();
	pointer(card, "pointerdown", 20);
	pointer(document, "pointermove", 450);
	expect(lanes.getByRole("region", { name: "In progress" }).getAttribute("data-drop-allowed")).toBe("true");
	expect(lanes.getByRole("region", { name: "Done" }).getAttribute("data-drop-allowed")).not.toBe("true");
	pointer(document, "pointerup", 450);
	expect(transition).not.toHaveBeenCalled();
	expect(
		within(lanes.getByRole("region", { name: "Ready" })).getByRole("button", { name: /Deliver phase moves/ }),
	).toBeTruthy();
	expect(await ui.findByRole("alert")).toHaveTextContent(/No legal transition/);
});
it("catches a stale revision rejection stranding a card in the target lane", async () => {
	reject = "stale story revision: expected 7, actual 8";
	const { ui, card, target, lanes } = await board();
	drag(card, 250);
	expect(await ui.findByRole("alert")).toHaveTextContent(reject);
	expect(within(target).queryByRole("button", { name: /Deliver phase moves/ })).toBeNull();
	expect(
		within(lanes.getByRole("region", { name: "Ready" })).getByRole("button", { name: /Deliver phase moves/ }),
	).toBeTruthy();
});
it("catches list and detail bypassing the shared revision-checked transition path", async () => {
	storiesUi.setView("list");
	const ui = render(() => <StoriesDialog project="/phase-moves" onClose={() => {}} />);
	const selector = await ui.findByRole("combobox", { name: "Phase for Deliver phase moves" });
	await waitFor(() => expect(within(selector).queryByRole("option", { name: "In progress" })).toBeTruthy());
	expect(within(selector).queryByRole("option", { name: "Done" })).toBeNull();
	const detail = within(ui.getByRole("region", { name: "Story details" }));
	const phase = detail.getByRole("combobox", { name: "Phase" });
	const edit = detail.getByRole("button", { name: "Edit" });
	expect(phase.closest("[data-story-header-actions]")).toBe(edit.closest("[data-story-header-actions]"));
	fireEvent.change(selector, { target: { value: "start_manual" } });
	await waitFor(() =>
		expect(transition).toHaveBeenCalledExactlyOnceWith({
			action: "transition",
			story_id: "s1",
			expected_revision: 7,
			command: "start_manual",
		}),
	);
});
it("catches Escape dispatching a retained drop after drag cancellation", async () => {
	const { card } = await board();
	pointer(card, "pointerdown", 20);
	pointer(document, "pointermove", 250);
	fireEvent.keyDown(document, { key: "Escape" });
	pointer(document, "pointerup", 250);
	expect(transition).not.toHaveBeenCalled();
});

it("catches closing the dialog leaving a ghost or dispatching a late drop", async () => {
	const { ui, card } = await board();
	pointer(card, "pointerdown", 20);
	pointer(document, "pointermove", 250);
	ui.unmount();
	expect(document.querySelector("button[data-control-layout=card]")).toBeNull();
	pointer(document, "pointerup", 250);
	expect(transition).not.toHaveBeenCalled();
});

it("catches closing a loading dialog continuing to fan out phase reads", async () => {
	const original = vi.mocked(invoke).getMockImplementation()!;
	const pending: Array<(value: unknown) => void> = [];
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const action = (args as { action?: { action: string } } | undefined)?.action;
		if (action?.action === "available_transitions") return new Promise((resolve) => pending.push(resolve));
		if (action?.action === "plan_view")
			return {
				type: "plan_view",
				value: {
					stories: Array.from({ length: 8 }, (_, index) => ({ ...initial, id: `story-${index}` })),
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
					columns: [],
				},
			};
		return original(command, args);
	});
	const ui = render(() => <StoriesDialog project="/phase-moves" onClose={() => {}} />);
	await waitFor(() => expect(pending).toHaveLength(4));
	ui.unmount();
	for (const release of pending) release({ type: "available_transitions", value: { revision: 7, moves: [] } });
	await Promise.resolve();
	await Promise.resolve();
	expect(pending).toHaveLength(4);
});
