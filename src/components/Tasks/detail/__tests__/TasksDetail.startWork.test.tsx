// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkItemAction, WorkItemDetail } from "../../../../types/work-items.gen";
import { TasksDetail } from "../TasksDetail";
import { detail, provider, reference } from "./detailFixture";

vi.mock("../../../../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn().mockResolvedValue("DEMO-1: Inspect the work item\n\nA **clear** description."),
}));
const hostStartWork = vi.hoisted(() => vi.fn());
const writeClipboard = vi.hoisted(() => vi.fn());
vi.mock("../../../../hooks/git/startWork", () => ({ startWork: hostStartWork }));
vi.mock("../../../../utils/clipboard", () => ({ writeClipboard }));

const START = { id: "start", name: "Start work", kind: "start_work", enabled: true, disabledReason: null } as const;

function withStartWork(overrides: Partial<WorkItemDetail> = {}): WorkItemDetail {
	const value = detail();
	value.actions.push({ ...START });
	return { ...value, ...overrides };
}

/** A provider that, like the Rust one, answers a start_work action with the host intent it received. */
function startProvider(value = withStartWork()) {
	const acted: WorkItemAction[] = [];
	const source = provider({
		detail: async () => value,
		act: async (_ref, action) => {
			acted.push(action);
			if (action.kind === "start_work") return { kind: "start_work", value: action.value };
			return { kind: "item", value };
		},
	});
	return { source, acted };
}

beforeEach(() => {
	hostStartWork.mockReset();
	hostStartWork.mockImplementation(async (request: { repoPath: string; branch: string }) => ({
		...request,
		workspaceId: "w",
		worktreePath: "/p",
	}));
	writeClipboard.mockReset();
	writeClipboard.mockResolvedValue(undefined);
});
afterEach(cleanup);

describe("TasksDetail start work", () => {
	// Catches: action kind start_work filtered out of the detail
	it("renders_a_start_work_button_for_a_start_work_descriptor", async () => {
		render(() => <TasksDetail provider={startProvider().source} reference={reference} repoPath="/repo" />);
		expect(await screen.findByRole("button", { name: "Start work" })).toBeEnabled();
	});

	// Catches: a provider without the action still showing a button
	it("shows_no_start_work_button_when_the_provider_declares_none", async () => {
		render(() => <TasksDetail provider={provider()} reference={reference} repoPath="/repo" />);
		await screen.findByText("Inspect the work item");
		expect(screen.queryByRole("button", { name: "Start work" })).toBeNull();
	});

	// Catches: the click sending the provider-suggested branch nowhere, or a made-up repo/prompt to the host
	it("start_work_uses_the_provider_suggested_branch_and_links_the_item", async () => {
		const value = withStartWork();
		value.item.fields.branch = { kind: "text", value: "feature/demo-1-from-provider" };
		const { source, acted } = startProvider(value);
		render(() => <TasksDetail provider={source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Start work" }));
		await screen.findByText(/Started work on feature\/demo-1-from-provider/);
		expect(acted).toEqual([
			{
				kind: "start_work",
				value: {
					repoPath: "/repo",
					branch: "feature/demo-1-from-provider",
					baseRef: null,
					prompt: expect.stringContaining("DEMO-1: Inspect the work item"),
					agentType: null,
					delivery: "draft",
				},
			},
		]);
		expect(hostStartWork).toHaveBeenCalledWith(
			expect.objectContaining({
				repoPath: "/repo",
				branch: "feature/demo-1-from-provider",
				link: expect.objectContaining({ key: "DEMO-1", reference, url: "https://example.com/1" }),
			}),
		);
	});

	// Catches: no branch name when the provider suggests none
	it("start_work_derives_a_branch_from_key_and_title_without_a_suggestion", async () => {
		render(() => <TasksDetail provider={startProvider().source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Start work" }));
		await screen.findByText(/Started work on demo-1-inspect-the-work-item/);
	});

	// Catches: a second click starting a second worktree while the first is in flight, or no pending feedback
	it("shows_a_pending_state_and_blocks_a_second_click", async () => {
		let finish: (value: unknown) => void = () => {};
		hostStartWork.mockReturnValue(new Promise((resolve) => (finish = resolve)));
		render(() => <TasksDetail provider={startProvider().source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Start work" }));
		const busy = await screen.findByRole("button", { name: "Starting…" });
		expect(busy).toBeDisabled();
		fireEvent.click(busy);
		finish({ repoPath: "/repo", workspaceId: "w", branch: "b", worktreePath: "/p" });
		await screen.findByText(/Started work on/);
		expect(hostStartWork).toHaveBeenCalledTimes(1);
	});

	// Catches: a host failure swallowed, or Retry that does not rerun start work
	it("shows_the_error_with_retry_that_runs_start_work_again", async () => {
		hostStartWork.mockRejectedValueOnce(new Error("worktree exists"));
		render(() => <TasksDetail provider={startProvider().source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Start work" }));
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toContain("worktree exists");
		fireEvent.click(screen.getByRole("button", { name: "Retry start work" }));
		await screen.findByText(/Started work on/);
		expect(hostStartWork).toHaveBeenCalledTimes(2);
		expect(screen.queryByRole("alert")).toBeNull();
	});

	// Catches: a provider that answers with an item instead of the host intent being reported as success
	it("reports_a_provider_that_does_not_return_the_start_work_intent", async () => {
		const value = withStartWork();
		const source = provider({ detail: async () => value, act: async () => ({ kind: "item", value }) });
		render(() => <TasksDetail provider={source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Start work" }));
		expect((await screen.findByRole("alert")).textContent).toContain("did not return");
		expect(hostStartWork).not.toHaveBeenCalled();
	});

	// Catches: Start work clickable with no repository to create the worktree in
	it("disables_start_work_without_a_repository", async () => {
		render(() => <TasksDetail provider={startProvider().source} reference={reference} />);
		const button = await screen.findByRole("button", { name: "Start work" });
		expect(button).toBeDisabled();
		expect(button.getAttribute("title")).toMatch(/repository/i);
	});

	// Catches: a provider-disabled action being clickable, or its reason lost
	it("keeps_a_disabled_descriptor_disabled_with_its_reason", async () => {
		const value = withStartWork();
		value.actions = value.actions.map((a) =>
			a.kind === "start_work" ? { ...a, enabled: false, disabledReason: "Already linked" } : a,
		);
		render(() => <TasksDetail provider={startProvider(value).source} reference={reference} repoPath="/repo" />);
		const button = await screen.findByRole("button", { name: "Start work" });
		expect(button).toBeDisabled();
		expect(button.getAttribute("title")).toBe("Already linked");
	});

	// Catches: Copy branch name copying the title or nothing instead of the branch Start work would use
	it("copy_branch_name_copies_the_branch_start_work_would_use", async () => {
		const value = withStartWork();
		value.item.fields.branch = { kind: "text", value: "feature/demo-1" };
		render(() => <TasksDetail provider={startProvider(value).source} reference={reference} repoPath="/repo" />);
		fireEvent.click(await screen.findByRole("button", { name: "Copy branch name" }));
		await waitFor(() => expect(writeClipboard).toHaveBeenCalledWith("feature/demo-1"));
	});
});
