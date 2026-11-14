import "../fixtures/dialogActivityHydration";
import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { emitTo } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesPanelAdapter } from "../../panelAdapters/stories";
import { storiesUi } from "../../stores/storiesUi";
import { uiStore } from "../../stores/ui";
import { waitingItemsStore } from "../../stores/waitingItems";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../hooks/initPanelWindow", () => ({ initPanelWindow: vi.fn().mockResolvedValue(undefined) }));
const windowListeners = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());
vi.mock("@tauri-apps/api/webviewWindow", () => ({
	getCurrentWebviewWindow: () => ({
		listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
			windowListeners.set(name, handler);
			return () => windowListeners.delete(name);
		},
	}),
}));

const project = "/waiting";
const plans = ["A", "B"].map((id) => ({ id, project, title: `Plan ${id}`, source: `${id}.md` }));
const stories = ["A1", "B1"].map((id) => ({
	id,
	planId: id[0],
	title: `Story ${id}`,
	criteria: [`Criterion ${id}`],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status: id === "B1" ? "qa" : "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
}));
const item = (overrides: Record<string, unknown>) => ({
	kind: "manual_check",
	detail: "Manual checks are waiting in Plans and Stories.",
	options: [],
	project,
	story: null,
	...overrides,
});
const attention = {
	items: [
		item({ id: "story:B1", title: "Story B1", planId: "B", story: { id: "B1", title: "Story B1", status: "qa" } }),
		item({ id: "story:X", title: "Elsewhere", project: "/other", planId: "X" }),
	],
	health: [],
	warnings: [],
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
		if (command === "boss_attention") return attention;
		const action = (args as { action: { action: string; plan_id?: string } }).action;
		if (action.action === "list_plans") return { type: "plans", value: plans };
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "available_transitions") return { type: "available_transitions", value: { moves: [] } };
		if (action.action === "plan_view") {
			const selected = stories.filter((story) => story.planId === action.plan_id);
			return {
				type: "plan_view",
				value: {
					stories: selected,
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
					visibleStoryIds: selected.map((story) => story.id),
				},
			};
		}
		return { type: "unknown", value: [] };
	});
});
afterEach(() => {
	cleanup();
	waitingItemsStore._resetForTest();
	uiStore.clearDetached("stories");
	uiStore._testCancelPendingSave();
});

// Catches: Plans and Stories does not show the project's waiting items, shows
// another project's items, or its entries do not lead to the story.
it("waiting_section_lists_project_items_and_links_to_their_source", async () => {
	const ui = render(() => <StoriesDialog project={project} onClose={() => {}} />);
	const section = await ui.findByRole("region", { name: "Waiting on you" });
	await within(section).findByRole("button", { name: /Story B1/ });
	expect(within(section).queryByText("Elsewhere")).toBeNull();
	expect(ui.queryByText(/Boss/)).toBeNull();

	await ui.findByRole("heading", { name: "Story A1" });
	fireEvent.click(within(section).getByRole("button", { name: /Story B1/ }));
	await ui.findByRole("heading", { name: "Story B1" });
});

// Catches: a bell click while Plans and Stories is detached only focuses the
// detached window and never reaches the item, and leaves the target pending in
// the main window, so a later docked open of that project jumps to a stale item.
it("bell_click_while_detached_lands_on_the_item_without_a_stale_target", async () => {
	uiStore.setDetached("stories", "panel-stories");
	storiesUi.reveal({ project, planId: "B", storyId: "B1" });
	expect(storiesUi.target()).toBeNull();
	const sent = vi.mocked(emitTo).mock.calls.find(([to]) => to === "panel-stories");
	expect(sent).toBeDefined();
	const [, event, payload] = sent!;

	// The detached window is a separate WebView: its own store never saw the
	// main window's detach, and it opened on another project.
	uiStore.clearDetached("stories");
	const Panel = storiesPanelAdapter.Component;
	const ui = render(() => <Panel params={new URLSearchParams({ project: "/elsewhere" })} />);
	await ui.findByRole("heading", { name: "Story A1" });
	await waitFor(() => expect(windowListeners.has(event)).toBe(true));
	windowListeners.get(event)!({ payload });
	await ui.findByRole("heading", { name: "Story B1" });
	expect(vi.mocked(invoke)).toHaveBeenCalledWith("story_action_command", expect.objectContaining({ project }));
});

// Catches: a long waiting list pushing the columns down, or the total hidden
// because the header scrolled away with the rows.
it("twelve_waiting_items_keep_the_columns_and_show_the_total_in_a_fixed_header", async () => {
	const many = Array.from({ length: 12 }, (_, i) =>
		item({
			id: `story:M${i}`,
			title: `Many ${i}`,
			planId: "A",
			story: { id: `M${i}`, title: `Many ${i}`, status: "qa" },
		}),
	);
	const base = vi.mocked(invoke).getMockImplementation() as (command: string, args: unknown) => Promise<unknown>;
	vi.mocked(invoke).mockImplementation(((command: string, args: unknown) =>
		command === "boss_attention"
			? Promise.resolve({ items: many, health: [], warnings: [] })
			: base(command, args)) as never);
	const ui = render(() => <StoriesDialog project={project} onClose={() => {}} />);
	const section = await ui.findByRole("region", { name: "Waiting on you" });
	await within(section).findByRole("button", { name: /Many 11/ });
	const header = within(section).getByRole("heading", { name: "Waiting on you · 12" });
	const list = within(section).getByRole("navigation", { name: "Waiting items" });
	expect(within(list).getAllByRole("button")).toHaveLength(12);
	expect(list.contains(header)).toBe(false);
	expect(await ui.findByRole("heading", { name: "Story A1" })).toBeTruthy();
});
