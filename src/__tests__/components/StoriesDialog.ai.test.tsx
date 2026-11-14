import "../fixtures/dialogActivityHydration";
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

const draft = {
	title: "Search saved work",
	storyType: "feature",
	priority: 2,
	problem: "Saved work is hard to find.",
	criteria: ["A title query finds matching work", "[MANUAL] Review keyboard navigation"],
};
let prepare: () => Promise<string>;
let generate: () => Promise<unknown>;
let cancel: () => Promise<unknown>;
const actions = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(([command]) => command === "story_action_command")
		.map(([, args]) => (args as { action: { action: string; input?: unknown } }).action);

beforeEach(() => {
	storiesUi.setVisibility("all");
	vi.mocked(invoke).mockReset();
	prepare = async () => "fill-1";
	generate = async () => draft;
	cancel = async () => true;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
		if (command === "acp_one_shot_prepare") return prepare();
		if (command === "acp_one_shot_cancel") return cancel();
		if (command === "story_generate_draft") return generate();
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_plans")
			return { type: "plans", value: [{ id: "p1", title: "Plan", project: "/repo", source: "plan.md" }] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
		if (action.action === "create_story") return { type: "story", value: { id: "s1" } };
		throw new Error(`Unexpected command ${command} ${action.action}`);
	});
});

async function openForm(project: string | (() => string) = "/repo") {
	const view = render(() => (
		<StoriesDialog project={typeof project === "string" ? project : project()} onClose={() => {}} />
	));
	const open = await screen.findByRole("button", { name: "New story" });
	await waitFor(() => expect((open as HTMLButtonElement).disabled).toBe(false));
	fireEvent.click(open);
	fireEvent.input(screen.getByLabelText("Request"), { target: { value: "Let me search saved work" } });
	return view;
}

describe("AI story draft review boundary", () => {
	it("never saves generated fields until the user reviews and explicitly creates the story", async () => {
		// Catches: a successful AI fill bypasses review and calls create_story itself.
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("Search saved work"));
		expect(actions().some((action) => action.action === "create_story")).toBe(false);
		expect(invoke).toHaveBeenCalledWith("story_generate_draft", {
			project: "/repo",
			request: "Let me search saved work",
			requestId: "fill-1",
		});
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Reviewed search" } });
		fireEvent.submit(screen.getByRole("button", { name: "Create story" }).closest("form")!);
		await waitFor(() =>
			expect(actions()).toContainEqual(
				expect.objectContaining({
					action: "create_story",
					problem: draft.problem,
					input: expect.objectContaining({
						title: "Reviewed search",
						criteria: draft.criteria,
						priority: 2,
						storyType: "feature",
					}),
				}),
			),
		);
	});

	it("cancels owned model work and rejects a late reply without clearing the request", async () => {
		// Catches: cancelled generation still overwrites the editable draft.
		let finish!: (value: unknown) => void;
		generate = () =>
			new Promise((resolve) => {
				finish = resolve;
			});
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_generate_draft", expect.anything()));
		expect(screen.getByRole("status").textContent).toContain("Filling");
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("acp_one_shot_cancel", { project: "/repo", requestId: "fill-1" }),
		);
		finish(draft);
		await screen.findByText("Fill cancelled.");
		expect((screen.getByLabelText("Request") as HTMLTextAreaElement).value).toBe("Let me search saved work");
		expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("");
		expect(actions().some((action) => action.action === "create_story")).toBe(false);
	});

	it("revokes a late reservation when cancelled before model launch", async () => {
		// Catches: Cancel while prepare is pending still launching model work when the handle arrives.
		let reserve!: (id: string) => void;
		prepare = () =>
			new Promise((resolve) => {
				reserve = resolve;
			});
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await screen.findByText("Fill cancelled.");
		reserve("late-handle");
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("acp_one_shot_cancel", { project: "/repo", requestId: "late-handle" }),
		);
		expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "story_generate_draft")).toBe(false);
	});

	it("keeps request and manually entered fields usable when generation fails", async () => {
		// Catches: malformed/model failure destroys user input or leaves the form locked.
		generate = async () => {
			throw new Error("Invalid story draft reply");
		};
		await openForm();
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "My title" } });
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await screen.findByText("Invalid story draft reply");
		expect((screen.getByLabelText("Request") as HTMLTextAreaElement).value).toBe("Let me search saved work");
		expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("My title");
		expect((screen.getByRole("button", { name: "Fill with AI" }) as HTMLButtonElement).disabled).toBe(false);
	});
});

// Exercise the real invoke/router for owned work, keeping unrelated story-list
// setup local to this component fixture. Assert outgoing destinations, not mocks'
// return values or a hand-copied routing argument shape.
async function routeOwnedCalls() {
	const { invoke: realInvoke } = await vi.importActual<typeof import("../../invoke")>("../../invoke");
	const runtime = await import("../../transportRuntime");
	runtime.setRepoConnectionLookup((path) => (path === "/remote/repo" ? "story-daemon" : undefined));
	runtime.setRemoteBaseUrlLookup((id) => (id === "story-daemon" ? "http://story-daemon.test" : undefined));
	runtime.setRemoteTokenLookup(() => undefined);
	const setupInvoke = vi.mocked(invoke).getMockImplementation()!;
	vi.mocked(invoke).mockImplementation((command, args) =>
		command === "acp_one_shot_prepare" || command === "acp_one_shot_cancel" || command === "story_generate_draft"
			? realInvoke(command, args)
			: setupInvoke(command, args),
	);
	const fetch = vi.fn<typeof globalThis.fetch>();
	vi.stubGlobal("fetch", fetch);
	return fetch;
}

afterEach(async () => {
	vi.unstubAllGlobals();
	const runtime = await import("../../transportRuntime");
	runtime.setRepoConnectionLookup(() => undefined);
	runtime.setRemoteBaseUrlLookup(() => undefined);
	runtime.setRemoteTokenLookup(() => undefined);
});

describe("remote story fill ownership", () => {
	it("prepares, generates and cancels on the same daemon even after the viewed project changes", async () => {
		// Catches: a remote fill reserves locally, or cancellation follows the new project.
		const fetch = await routeOwnedCalls();
		let finish!: (response: Response) => void;
		fetch.mockImplementation(async (url) => {
			if (String(url).endsWith("/prepare")) return Response.json("remote-fill");
			if (String(url).endsWith("/cancel")) return Response.json(true);
			return new Promise((resolve) => {
				finish = resolve;
			});
		});
		const [project, setProject] = createSignal("/remote/repo");
		await openForm(project);
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
		setProject("/other/repo");
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3));
		finish(Response.json(draft));
		await screen.findByText("Fill cancelled.");
		expect(fetch.mock.calls.map(([url]) => String(url))).toEqual([
			"http://story-daemon.test/acp/one-shot/prepare",
			"http://story-daemon.test/acp/story-draft",
			"http://story-daemon.test/acp/one-shot/remote-fill/cancel",
		]);
		expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("");
	});

	it("revokes a late remote reservation on its original daemon after cancellation", async () => {
		// Catches: the cancel-before-prepare cleanup drops the remote owner.
		const fetch = await routeOwnedCalls();
		let reserve!: (response: Response) => void;
		fetch.mockImplementation(async (url) =>
			String(url).endsWith("/prepare")
				? new Promise((resolve) => {
						reserve = resolve;
					})
				: Response.json(true),
		);
		await openForm("/remote/repo");
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await screen.findByText("Fill cancelled.");
		reserve(Response.json("late-remote"));
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
		expect(fetch.mock.calls.map(([url]) => String(url))).toEqual([
			"http://story-daemon.test/acp/one-shot/prepare",
			"http://story-daemon.test/acp/one-shot/late-remote/cancel",
		]);
	});

	it("unmount cleanup cancels the original daemon rather than the currently viewed project", async () => {
		// Catches: closing the dialog leaves owned model work running on a remote daemon.
		const fetch = await routeOwnedCalls();
		let finish!: (response: Response) => void;
		fetch.mockImplementation(async (url) => {
			if (String(url).endsWith("/prepare")) return Response.json("remote-fill");
			if (String(url).endsWith("/cancel")) return Response.json(true);
			return new Promise((resolve) => {
				finish = resolve;
			});
		});
		const [project, setProject] = createSignal("/remote/repo");
		const view = await openForm(project);
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
		setProject("/other/repo");
		view.unmount();
		await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3));
		expect(String(fetch.mock.calls[2][0])).toBe("http://story-daemon.test/acp/one-shot/remote-fill/cancel");
		finish(Response.json(draft));
	});
});
