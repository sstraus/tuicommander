import "../fixtures/dialogActivityHydration";
import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { Suspense } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plans = ["A", "B"].map((id) => ({ id, project: "/selection", title: `Plan ${id}`, source: `${id}.md` }));
const stories = ["A1", "A2", "B1"].map((id) => ({
	id,
	planId: id[0],
	title: `Story ${id}`,
	criteria: [`Criterion ${id}`],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
}));
let pendingNotes: Map<string, () => void>;

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	pendingNotes = new Map();
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
		const action = (args as { action: { action: string; plan_id?: string; story_id?: string } }).action;
		if (action.action === "list_plans") return { type: "plans", value: plans };
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") {
			if (action.story_id === "A1") return { type: "notes", value: [] };
			return new Promise((resolve) => {
				pendingNotes.set(action.story_id!, () => resolve({ type: "notes", value: [] }));
			});
		}
		if (action.action === "plan_view" || action.action === "filtered_plan_view") {
			const selected = stories.filter((story) => story.planId === action.plan_id);
			return {
				type: "plan_view",
				value: {
					stories: selected,
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
					visibleStoryIds: action.action === "filtered_plan_view" ? ["A2"] : selected.map((story) => story.id),
				},
			};
		}
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(() => cleanup());

it.each(["plan", "story", "filter"])(
	"catches %s selection letting pending notes remove or hide the whole dialog through outer Suspense",
	async (selection) => {
		// Both inline and detached production hosts have this outer lazy-loading boundary.
		const ui = render(() => (
			<Suspense fallback={<p>Loading panel</p>}>
				<StoriesDialog project="/selection" onClose={() => {}} />
			</Suspense>
		));
		await ui.findByText("Criterion A1");
		await waitFor(() => expect(ui.queryByText("Loading notes…")).toBeNull());
		const dialog = ui.getByRole("dialog", { name: "Plans and Stories" });
		const removed: Node[] = [];
		const observer = new MutationObserver((records) => {
			for (const record of records) removed.push(...record.removedNodes);
		});
		observer.observe(ui.container, { childList: true, subtree: true });
		try {
			if (selection === "plan") fireEvent.click(ui.getByRole("button", { name: "Plan B" }));
			else if (selection === "story") fireEvent.click(ui.getByRole("button", { name: /Story A2/ }));
			else fireEvent.change(ui.getByLabelText("Status"), { target: { value: "open" } });
			const next = selection === "plan" ? "B1" : "A2";
			await waitFor(() => expect(pendingNotes.has(next)).toBe(true));
			expect(ui.getByRole("dialog", { name: "Plans and Stories" })).toBe(dialog);
			expect(dialog.isConnected).toBe(true);
			expect(dialog).toBeVisible();
			expect(removed.some((node) => node === dialog || node.contains(dialog))).toBe(false);
			expect(within(dialog).getByRole("heading", { name: `Story ${next}` })).toBeTruthy();
			pendingNotes.get(next)!();
			await waitFor(() => expect(ui.queryByText("Loading notes…")).toBeNull());
			expect(ui.getByRole("dialog", { name: "Plans and Stories" })).toBe(dialog);
			expect(dialog).toBeVisible();
		} finally {
			observer.disconnect();
			for (const finish of pendingNotes.values()) finish();
		}
	},
);
