import "../fixtures/dialogActivityHydration";
import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { openFileAction } from "../../utils/filePreview";
import { handleOpenUrl } from "../../utils/openUrl";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../utils/filePreview", () => ({ openFileAction: vi.fn() }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

let plan: { id: string; project: string; title: string; source: string; revision: number };
let failure: string;
const updates = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(([, args]) => (args as { action?: { action: string } })?.action?.action === "update_plan");
beforeEach(() => {
	plan = { id: "plan", project: "/plan-edit", title: "Original plan", source: "original.md", revision: 7 };
	failure = "";
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
		const action = (args as { action: { action: string; patch?: object } }).action;
		if (action.action === "list_plan_sources")
			return { type: "plan_sources", value: [{ title: "Repository plan", source: "repository-plan.md" }] };
		if (action.action === "list_plans") return { type: "plans", value: [{ ...plan }] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
		if (action.action === "update_plan") {
			if (failure) throw new Error(failure);
			plan = { ...plan, ...action.patch, revision: plan.revision + 1 };
			return { type: "plan", value: { ...plan } };
		}
		throw new Error(`Unexpected action ${action.action}`);
	});
});
afterEach(cleanup);

it("catches plan saves omitting a field, losing the original revision, or failing to refresh the title", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	const form = within(ui.getByRole("form", { name: "Edit plan" }));
	fireEvent.input(form.getByLabelText("Plan title"), { target: { value: "Corrected plan" } });
	fireEvent.change(form.getByRole("combobox", { name: "Repository plan document" }), {
		target: { value: "__other_source__" },
	});
	fireEvent.input(form.getByLabelText("Plan document or link"), { target: { value: "moved.md" } });
	fireEvent.click(form.getByRole("button", { name: "Save" }));
	await ui.findByRole("button", { name: "Corrected plan" });
	expect(updates()).toHaveLength(1);
	expect(updates()[0][1]).toEqual({
		project: "/plan-edit",
		action: {
			action: "update_plan",
			plan_id: "plan",
			expected_revision: 7,
			patch: { title: "Corrected plan", source: "moved.md" },
		},
	});
	expect(ui.queryByRole("form", { name: "Edit plan" })).toBeNull();
});

it("catches title-only edits resending an unchanged source and Cancel persisting a draft", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Discarded" } });
	fireEvent.click(ui.getByRole("button", { name: "Cancel" }));
	expect(updates()).toHaveLength(0);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Original plan");
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Only title" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(1));
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7, patch: { title: "Only title" } } });
	expect((updates()[0][1] as { action: { patch: object } }).action.patch).not.toHaveProperty("source");
});

it("catches a stale plan revision discarding the draft or silently rebasing its Save", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Keep my draft" } });
	plan = { ...plan, title: "Concurrent title", revision: 8 };
	failure = "stale plan revision: expected 7, current 8";
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect((await ui.findByRole("alert")).textContent).toContain("Cancel and reopen Edit plan");
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Keep my draft");
	expect(updates()).toHaveLength(1);
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7 } });
	fireEvent.click(ui.getByRole("button", { name: "Cancel" }));
	await ui.findByRole("button", { name: "Concurrent title" });
	fireEvent.click(ui.getByRole("button", { name: "Edit plan" }));
	expect((ui.getByLabelText("Plan title") as HTMLInputElement).value).toBe("Concurrent title");
	failure = "";
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Explicit new edit" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(2));
	expect(updates()[1][1]).toMatchObject({ action: { expected_revision: 8 } });
});

it("catches active-coordinator source refusals hiding the remedy or clearing the draft", async () => {
	failure =
		"plan source is in use by an active plan coordinator: run active; finish the coordinator or cancel the run before changing the source";
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.change(ui.getByRole("combobox", { name: "Repository plan document" }), {
		target: { value: "__other_source__" },
	});
	fireEvent.input(ui.getByLabelText("Plan document or link"), { target: { value: "moved.md" } });
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	expect((await ui.findByRole("alert")).textContent).toBe(failure);
	expect((ui.getByLabelText("Plan document or link") as HTMLInputElement).value).toBe("moved.md");
	expect(updates()).toHaveLength(1);
});

it("catches the plan source being available only as typed text", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	const picker = await ui.findByRole("combobox", { name: "Repository plan document" });
	await ui.findByRole("option", { name: "Repository plan" });
	fireEvent.change(picker, { target: { value: "repository-plan.md" } });
	expect(ui.queryByLabelText("Plan document or link")).toBeNull();
	expect(ui.getByText("repository-plan.md")).toBeVisible();
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(1));
	expect(updates()[0][1]).toMatchObject({ action: { expected_revision: 7, patch: { source: "repository-plan.md" } } });
});

it("catches disabled Save hiding its reason or submitting an invalid patch", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	const save = ui.getByRole("button", { name: "Save" });
	expect(save).toBeDisabled();
	expect(save).toHaveAccessibleDescription("Change the title or document to save.");
	expect(save).toHaveAttribute("title", "Change the title or document to save.");
	expect(ui.getByText("Change the title or document to save.")).not.toBeVisible();
	fireEvent.submit(ui.getByRole("form", { name: "Edit plan" }));
	expect(updates()).toHaveLength(0);
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: " " } });
	expect(save).toHaveAccessibleDescription("Enter a plan title to save.");
	expect(save).toHaveAttribute("title", "Enter a plan title to save.");
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Valid" } });
	fireEvent.change(ui.getByRole("combobox", { name: "Repository plan document" }), {
		target: { value: "__other_source__" },
	});
	fireEvent.input(ui.getByLabelText("Plan document or link"), { target: { value: " " } });
	expect(save).toHaveAccessibleDescription("Choose a plan document or paste a link to save.");
	expect(save).toHaveAttribute("title", "Choose a plan document or paste a link to save.");
	fireEvent.submit(ui.getByRole("form", { name: "Edit plan" }));
	expect(updates()).toHaveLength(0);
});

it("catches a pasted URL or selected file opening the wrong source", async () => {
	const onClose = vi.fn();
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={onClose} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	fireEvent.change(ui.getByRole("combobox", { name: "Repository plan document" }), {
		target: { value: "__other_source__" },
	});
	fireEvent.input(ui.getByLabelText("Plan document or link"), { target: { value: "https://example.com/plan" } });
	fireEvent.click(ui.getByRole("button", { name: "Open" }));
	expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/plan");
	expect(onClose).not.toHaveBeenCalled();
	const picker = ui.getByRole("combobox", { name: "Repository plan document" });
	await ui.findByRole("option", { name: "Repository plan" });
	fireEvent.change(picker, { target: { value: "repository-plan.md" } });
	fireEvent.click(ui.getByRole("button", { name: "Open" }));
	expect(openFileAction).toHaveBeenCalledWith("repository-plan.md", "/plan-edit");
	expect(onClose).toHaveBeenCalledOnce();
});

it("catches a failed document listing losing the draft instead of offering retry", async () => {
	const original = vi.mocked(invoke).getMockImplementation()!;
	let failListing = true;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if ((args as { action?: { action: string } })?.action?.action === "list_plan_sources" && failListing)
			throw new Error("Cannot read plan directory");
		return original(command, args);
	});
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	await ui.findByText("Cannot read plan directory");
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Keep this draft" } });
	failListing = false;
	fireEvent.click(ui.getByRole("button", { name: "Refresh" }));
	await ui.findByRole("option", { name: "Repository plan" });
	expect((ui.getByLabelText("Plan title") as HTMLTextAreaElement).value).toBe("Keep this draft");
	expect(ui.queryByLabelText("Plan document or link")).toBeNull();
	expect(ui.getByText("original.md")).toBeVisible();
});

it("catches archive action being separated from the selected plan actions", async () => {
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	const actions = within(await ui.findByRole("group", { name: "Plan actions" }));
	expect(actions.getByRole("button", { name: "Edit plan" })).toBeTruthy();
	expect(actions.getByRole("button", { name: "Archive plan" })).toBeTruthy();
	expect(actions.queryByLabelText("Archived plans")).toBeNull();
	expect(ui.getByLabelText("Archived plans")).toBeTruthy();
});

it("catches the current document losing selection when its repository option arrives asynchronously", async () => {
	plan.source = "repository-plan.md";
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	const option = await ui.findByRole("option", { name: "Repository plan" });
	const picker = ui.getByRole("combobox", { name: "Repository plan document" }) as HTMLSelectElement;
	await waitFor(() => expect(picker.value).toBe("repository-plan.md"));
	expect((option as HTMLOptionElement).selected).toBe(true);
	expect(ui.getByRole("button", { name: "Save" })).toBeDisabled();
});

it("catches Retry after a project read failure discarding an unsaved plan draft", async () => {
	const originalInvoke = vi.mocked(invoke).getMockImplementation()!;
	let unavailable = false;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const action = (args as { action?: { action: string } } | undefined)?.action?.action;
		if (unavailable && (action === "list_plan_sources" || action === "list_plans")) {
			throw new Error("project_unavailable: temporarily disconnected repository");
		}
		return originalInvoke(command, args);
	});
	const ui = render(() => <StoriesDialog project="/plan-edit" onClose={() => {}} />);
	fireEvent.click(await ui.findByRole("button", { name: "Edit plan" }));
	await waitFor(() => expect(ui.getByRole("button", { name: "Refresh" })).not.toBeDisabled());
	fireEvent.input(ui.getByLabelText("Plan title"), { target: { value: "Unsaved migration decision" } });
	fireEvent.change(ui.getByRole("combobox", { name: "Repository plan document" }), {
		target: { value: "__other_source__" },
	});
	fireEvent.input(ui.getByLabelText("Plan document or link"), { target: { value: "https://example.com/draft" } });
	unavailable = true;
	fireEvent.click(ui.getByRole("button", { name: "Refresh" }));
	await ui.findByRole("alert");
	fireEvent.click(ui.getByRole("button", { name: "Retry" }));
	await waitFor(() => expect(ui.queryByRole("status")).toBeNull());
	expect((ui.getByLabelText("Plan title") as HTMLTextAreaElement).value).toBe("Unsaved migration decision");
	unavailable = false;
	plan = { ...plan, revision: 8 };
	fireEvent.click(ui.getByRole("button", { name: "Retry" }));
	const title = await ui.findByLabelText("Plan title");
	expect((title as HTMLTextAreaElement).value).toBe("Unsaved migration decision");
	expect((ui.getByLabelText("Plan document or link") as HTMLTextAreaElement).value).toBe("https://example.com/draft");
	expect(updates()).toHaveLength(0);
	fireEvent.click(ui.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(updates()).toHaveLength(1));
	expect(updates()[0][1]).toMatchObject({
		action: {
			expected_revision: 7,
			patch: { title: "Unsaved migration decision", source: "https://example.com/draft" },
		},
	});
});
