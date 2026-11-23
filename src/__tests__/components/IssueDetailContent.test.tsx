import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import type { GitHubIssue } from "../../types";
import { mockInvoke } from "../mocks/tauri";

// Stand-in for the split button: its main part runs the first start, as the real one does with no last pick.
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({
	SmartButtonStrip: (p: { label?: string; starts?: { onSelect: () => void }[] }) => (
		<button type="button" onClick={() => p.starts?.[0]?.onSelect()}>
			{p.label}
		</button>
	),
}));

// The real preview, with the provider's offers and actions replaced so the panel can be driven.
const fake = vi.hoisted(() => ({
	offers: (_kind: string): boolean => false,
	act: vi.fn(async (_action: unknown) => {}),
	github: { state: { viewerLogin: null as string | null } },
}));
vi.mock("../../stores/github", () => ({ githubStore: fake.github }));
vi.mock("../../components/shared/WorkItemPreview", async (actual) => {
	const real = await actual<typeof import("../../components/shared/WorkItemPreview")>();
	return {
		...real,
		createWorkItemPreview: (...args: Parameters<typeof real.createWorkItemPreview>) => ({
			...real.createWorkItemPreview(...args),
			offers: (kind: string) => fake.offers(kind),
			act: fake.act,
		}),
	};
});

import { IssueDetailContent } from "../../components/IssueDetailPopover/IssueDetailContent";

const issue = (over: Partial<GitHubIssue> = {}) =>
	({
		number: 7,
		title: "Login fails",
		state: "OPEN",
		author: "alice",
		url: "",
		labels: [],
		assignees: [],
		comments_count: 3,
		created_at: "",
		updated_at: "",
		...over,
	}) as GitHubIssue;

const handlers = () => ({
	onReopen: vi.fn(),
	onAutofix: vi.fn(),
	onCreatePr: vi.fn(),
	onOpenBranch: vi.fn(),
});
const sectionOpen = (title: string) =>
	screen
		.getAllByRole("button")
		.find((b) => b.hasAttribute("aria-expanded") && b.textContent?.includes(title))
		?.getAttribute("aria-expanded");

describe("IssueDetailContent", () => {
	it("offers Reopen, and nothing to work on, for a closed issue", () => {
		// Catches: Auto-fix offered on a closed issue, or no way back from Closed.
		const h = handlers();
		render(() => <IssueDetailContent issue={issue({ state: "CLOSED" })} repoPath="/r" {...h} />);
		fireEvent.click(screen.getByRole("button", { name: "Reopen" }));
		expect(h.onReopen).toHaveBeenCalledOnce();
		expect(screen.queryByRole("button", { name: "Work on it" })).toBeNull();
	});

	it("moves to Create draft PR once the auto-fix branch has a worktree", () => {
		// Catches: Auto-fix offered again for an issue whose fix branch is already waiting for a PR.
		const h = handlers();
		render(() => <IssueDetailContent issue={issue()} repoPath="/r" autofixBranchReady {...h} />);
		expect(screen.getByText("Branch autofix/issue-7 ready · no linked PR")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Create draft PR" }));
		fireEvent.click(screen.getByRole("button", { name: "Open worktree" }));
		expect(h.onCreatePr).toHaveBeenCalledOnce();
		expect(h.onOpenBranch).toHaveBeenCalledOnce();
		expect(screen.queryByRole("button", { name: "Work on it" })).toBeNull();
	});

	it("offers Work on it, running Auto-fix, for an open issue without a fix branch", () => {
		// Catches: Create draft PR as the next step when no branch exists to open it from.
		const h = handlers();
		render(() => <IssueDetailContent issue={issue()} repoPath="/r" {...h} />);
		expect(screen.getByText("No branch or worktree for this issue")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Work on it" }));
		expect(h.onAutofix).toHaveBeenCalledOnce();
		expect(screen.queryByRole("button", { name: "Create draft PR" })).toBeNull();
	});

	it("opens the description of an open issue and leaves the comments closed", () => {
		// Catches: both sections expanding (and fetching) on every row open.
		render(() => <IssueDetailContent issue={issue()} repoPath="/r" />);
		expect(sectionOpen("Description")).toBe("true");
		expect(sectionOpen("Comments")).toBe("false");
	});

	it("assigns the viewer next to the current assignees", async () => {
		// Catches: the assignee PATCH replacing the list and dropping whoever was already assigned.
		fake.offers = (kind) => kind === "edit_field";
		fake.github.state.viewerLogin = "bob";
		render(() => <IssueDetailContent issue={issue({ assignees: ["carol"] })} repoPath="/r" />);
		fireEvent.click(screen.getByRole("button", { name: "Assign me" }));
		await vi.waitFor(() => expect(screen.queryByRole("button", { name: "Assign me" })).toBeNull());
		const action = fake.act.mock.calls[0]?.[0] as { value: { value: { value: { name: string }[] } } };
		expect(action.value.value.value.map((u) => u.name)).toEqual(["carol", "bob"]);
	});

	it("offers no Assign me when the viewer is already assigned or the provider cannot edit", () => {
		// Catches: a button that assigns the viewer twice, or fails on a read-only provider.
		fake.offers = () => true;
		fake.github.state.viewerLogin = "bob";
		const { unmount } = render(() => <IssueDetailContent issue={issue({ assignees: ["bob"] })} repoPath="/r" />);
		expect(screen.queryByRole("button", { name: "Assign me" })).toBeNull();
		unmount();
		fake.offers = () => false;
		render(() => <IssueDetailContent issue={issue()} repoPath="/r" />);
		expect(screen.queryByRole("button", { name: "Assign me" })).toBeNull();
	});

	it("leads with Reply and opens Comments when comments arrived since the last visit", async () => {
		// Catches: new comments recorded but never surfaced, or Work on it still claiming the next step.
		fake.offers = (kind) => kind === "comment";
		mockInvoke.mockImplementation(async (cmd: string) =>
			cmd === "visit_work_item" ? { count: 2, sinceMs: 1_000 } : undefined,
		);
		render(() => (
			<IssueDetailContent issue={issue({ url: "https://github.com/a/b/issues/7", comments_count: 4 })} repoPath="/r" />
		));
		expect(await screen.findByText("2 new comments since the last visit")).toBeInTheDocument();
		expect(screen.getByTestId("issue-comments").textContent).toContain("4 · 2 new");
		expect(screen.getByRole("button", { name: "Reply" })).toBeInTheDocument();
		expect(sectionOpen("Comments")).toBe("true");
		expect(sectionOpen("Description")).toBe("false");
		expect(mockInvoke).toHaveBeenCalledWith("visit_work_item", {
			itemUrl: "https://github.com/a/b/issues/7",
			commentCount: 4,
		});
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});

	it("shows nothing new on a first visit", async () => {
		// Catches: a first visit calling every existing comment new.
		mockInvoke.mockImplementation(async (cmd: string) =>
			cmd === "visit_work_item" ? { count: 0, sinceMs: null } : undefined,
		);
		render(() => <IssueDetailContent issue={issue({ url: "https://github.com/a/b/issues/7" })} repoPath="/r" />);
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("visit_work_item", expect.anything()));
		expect(screen.getByText("No branch or worktree for this issue")).toBeInTheDocument();
		expect(screen.getByTestId("issue-comments").textContent).not.toContain("new");
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});
});
