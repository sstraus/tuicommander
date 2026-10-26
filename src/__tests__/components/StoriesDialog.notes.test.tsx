import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { storiesUi } from "../../stores/storiesUi";
import { HttpRpcError } from "../../transport";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const story = {
	id: "first",
	planId: "plan",
	revision: 7,
	title: "First story",
	priority: 2,
	origin: { type: "native" },
	criteria: ["Observable effect"],
	checked: [false],
	dependencies: [],
	fileScope: [],
	status: "ready",
	claimSession: null,
	abandoned: false,
};
const historical = {
	id: "note",
	storyId: "first",
	storyRevision: 3,
	text: "Earlier evidence\n  preserved formatting",
	actor: { type: "managed_session", session_id: "worker-7" },
	createdAt: 1_790_000_000_000,
	receipts: [{ runId: "run-1", commandId: "check:original" }],
};
let notes: (typeof historical)[];
let append: (action: Record<string, unknown>) => Promise<unknown>;

beforeEach(() => {
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	notes = [historical];
	append = async (action) => {
		const note = { ...historical, id: "added", storyRevision: 7, text: String(action.text), receipts: [] };
		notes = [...notes, note];
		return { type: "note", value: note };
	};
	vi.mocked(invoke).mockReset();
	vi.mocked(appLogger.warn).mockClear();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: Record<string, unknown> }).action;
		if (action.action === "list_plans")
			return { type: "plans", value: [{ id: "plan", title: "Plan", project: "/notes", source: "plan.md" }] };
		if (action.action === "plan_view")
			return {
				type: "plan_view",
				value: {
					stories: [story, { ...story, id: "second", title: "Second story", revision: 9 }],
					state: "active",
					wontFixCount: 0,
					allCancelled: false,
				},
			};
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: action.story_id === "first" ? notes : [] };
		if (action.action === "add_note") return append(action);
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

it("catches omitting persisted notes/provenance/receipt references or advancing their historical revision in detail", async () => {
	const ui = render(() => <StoriesDialog project="/notes" onClose={() => {}} />);
	const region = await ui.findByRole("region", { name: "Notes and evidence" });
	const notesUi = within(region);
	expect(await notesUi.findByText("Managed session worker-7")).toBeTruthy();
	expect(notesUi.getByText("Revision 3")).toBeTruthy();
	expect(notesUi.getByText("run-1 / check:original")).toBeTruthy();
	expect(region.querySelector("time")?.dateTime).toBe(new Date(historical.createdAt).toISOString());
	expect(notesUi.getByText(/Earlier evidence/).textContent).toBe(historical.text);
	const input = notesUi.getByLabelText("Add note");
	fireEvent.input(input, { target: { value: "  New evidence\nkept verbatim  " } });
	fireEvent.click(notesUi.getByRole("button", { name: "Append note" }));
	await notesUi.findByText("Note added.");
	expect(invoke).toHaveBeenCalledWith("story_action_command", {
		project: "/notes",
		action: {
			action: "add_note",
			story_id: "first",
			expected_revision: 7,
			text: "  New evidence\nkept verbatim  ",
		},
	});
	expect((input as HTMLTextAreaElement).value).toBe("");
	await waitFor(() =>
		expect(notesUi.getAllByRole("listitem").some((row) => row.textContent?.includes("New evidence"))).toBe(true),
	);
	expect(notesUi.getByText("Revision 3")).toBeTruthy();
	expect(appLogger.warn).not.toHaveBeenCalled();
});

it("catches a rejected append discarding its draft or silently retrying a newer revision", async () => {
	append = async () => {
		throw new HttpRpcError(
			"story_action_command",
			422,
			'{"error":"invalid story note: stale story revision: expected 7, current 8"}',
		);
	};
	const ui = render(() => <StoriesDialog project="/notes" onClose={() => {}} />);
	const input = await ui.findByLabelText("Add note");
	fireEvent.input(input, { target: { value: "Keep this evidence" } });
	fireEvent.click(ui.getByRole("button", { name: "Append note" }));
	expect((await ui.findByRole("alert")).textContent).toBe(
		"invalid story note: stale story revision: expected 7, current 8",
	);
	expect((input as HTMLTextAreaElement).value).toBe("Keep this evidence");
	expect(
		vi
			.mocked(invoke)
			.mock.calls.filter(([, args]) => (args as { action?: { action: string } })?.action?.action === "add_note"),
	).toHaveLength(1);
	expect(appLogger.warn).toHaveBeenCalledWith("store", "Stories: note append failed", expect.anything());
});

it("catches a late append response clearing another story's draft or publishing the first story's notes there", async () => {
	let finish: (reply: unknown) => void = () => {
		throw new Error("append was not submitted");
	};
	append = () =>
		new Promise((resolve) => {
			finish = resolve;
		});
	const ui = render(() => <StoriesDialog project="/notes" onClose={() => {}} />);
	const input = await ui.findByLabelText("Add note");
	fireEvent.input(input, { target: { value: "First draft" } });
	fireEvent.click(ui.getByRole("button", { name: "Append note" }));
	fireEvent.click(ui.getByRole("button", { name: /Second story/ }));
	await ui.findByRole("heading", { name: "Second story" });
	const secondInput = ui.getByLabelText("Add note") as HTMLTextAreaElement;
	expect(secondInput.value).toBe("");
	fireEvent.input(secondInput, { target: { value: "Second draft" } });
	finish({ type: "note", value: { ...historical, id: "late", text: "First draft" } });
	// Drain the resolved IPC promise chain before asserting absent cross-subject effects.
	await new Promise<void>((resolve) => setTimeout(resolve, 0));
	expect(secondInput.value).toBe("Second draft");
	expect(ui.queryByText("First draft")).toBeNull();
	expect(ui.queryByText("Managed session worker-7")).toBeNull();
	expect(ui.queryByText("Note added.")).toBeNull();
});

it("catches external MCP notes remaining absent after Refresh notes", async () => {
	const ui = render(() => <StoriesDialog project="/notes" onClose={() => {}} />);
	await ui.findByText("Managed session worker-7");
	notes = [...notes, { ...historical, id: "external", text: "Added outside this WebView" }];
	fireEvent.click(ui.getByRole("button", { name: "Refresh notes" }));
	await ui.findByText("Added outside this WebView");
	expect(ui.queryByText("Note added.")).toBeNull();
	expect(appLogger.warn).not.toHaveBeenCalled();
});
