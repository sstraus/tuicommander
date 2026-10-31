import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { openTaskConversation, registerTaskWorkspaceOpener, taskNavigation } from "../../services/taskNavigation";
import { urlResolverRegistry } from "../CommandPalette/urlPaste";
import { detail, provider } from "./detail/__tests__/detailFixture";
import { TasksShell } from "./TasksShell";

const { openUrl, addTasks, workspaceIdOnBranch, branches } = vi.hoisted(() => ({
	openUrl: vi.fn(),
	addTasks: vi.fn(),
	workspaceIdOnBranch: vi.fn(),
	branches: {} as Record<string, { branch: string; url: string }>,
}));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: openUrl }));
vi.mock("../../stores/mdTabs", () => ({ mdTabsStore: { addTasks, onPluginPanelClosed: () => () => {} } }));
vi.mock("../../stores/github", () => ({ githubStore: { state: { repos: { "/repo": { branches } } } } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		state: { activeRepoPath: null },
		workspaceIdOnBranch,
	},
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(async () => null) }));

const url = "https://github.com/acme/remote-repo/pull/42";
const disposals: Array<{ dispose(): void }> = [];
afterEach(() => {
	cleanup();
	taskNavigation.clear();
	for (const registration of disposals.splice(0)) registration.dispose();
	for (const key of Object.keys(branches)) delete branches[key];
	vi.resetAllMocks();
});
function register(target = provider()) {
	disposals.push(workItemProviderRegistry.register(target));
	disposals.push(
		urlResolverRegistry.register("test-provider", async () => ({
			link: {
				reference: { providerId: target.providerId, accountId: target.accountId, id: "1" },
				key: "PR-42",
				title: "Remote-only PR",
				url,
				state: null,
			},
		})),
	);
	return target;
}

// Catches: a remote-only PR being opened in the currently selected provider/account, or threads hidden behind comment pagination.
it("opens the resolved account in the real Tasks detail and reveals paginated review threads", async () => {
	disposals.push(workItemProviderRegistry.register(provider({ accountId: "wrong-account", name: "Other source" })));
	const value = detail();
	value.item.itemType = "pull";
	value.item.url = url;
	const comment = value.comments.entries[0];
	if (comment.kind !== "normal") throw new Error("Expected normal comment fixture");
	const target = register(
		provider({
			detail: async () => value,
			comments: async () => ({
				entries: [
					{
						kind: "review_thread",
						value: {
							id: "thread-42",
							resolved: false,
							outdated: false,
							path: "src/remote.ts",
							line: 12,
							originalLine: null,
							commitId: "fixture-commit",
							comments: [comment.value],
							finding: null,
						},
					},
				],
				nextCursor: null,
			}),
		}),
	);
	const read = vi.spyOn(target, "detail");
	render(() => <TasksShell />);
	await openTaskConversation(url);
	expect(await screen.findByRole("region", { name: "Review thread" })).toHaveTextContent("src/remote.ts:12");
	expect(screen.getByRole("combobox", { name: "Tasks source" })).toHaveValue('["demo","demo-account"]');
	expect(read).toHaveBeenCalledWith(
		{ providerId: "demo", accountId: "demo-account", id: "1" },
		expect.any(AbortSignal),
	);
	expect(addTasks).toHaveBeenCalledOnce();
	expect(openUrl).not.toHaveBeenCalled();
});

// Catches: a provider-less click opening an empty Tasks tab instead of a usable conversation.
it("opens GitHub synchronously when no provider is loaded", async () => {
	const navigation = openTaskConversation(url);
	expect(openUrl).toHaveBeenCalledWith(url);
	await navigation;
	expect(addTasks).not.toHaveBeenCalled();
});

// Catches: a resolver whose account was unloaded opening a detail under a different account.
it("falls back to GitHub if the resolver has no matching registered account", async () => {
	disposals.push(workItemProviderRegistry.register(provider()));
	disposals.push(
		urlResolverRegistry.register("unloaded", async () => ({
			link: {
				reference: { providerId: "github", accountId: "gone", id: "acme/remote-repo/pull/42" },
				key: "PR-42",
				title: "Unloaded",
				url,
				state: null,
			},
		})),
	);
	await openTaskConversation(url);
	expect(openUrl).toHaveBeenCalledWith(url);
	expect(addTasks).not.toHaveBeenCalled();
});

// Catches: a slow earlier resolver stealing focus back after a newer PR click.
it("ignores an older navigation that finishes after the next click", async () => {
	disposals.push(workItemProviderRegistry.register(provider()));
	let finish: (value: null) => void = () => {};
	disposals.push(
		urlResolverRegistry.register("slow", async (value) =>
			value === url
				? new Promise<null>((resolve) => {
						finish = resolve;
					})
				: null,
		),
	);
	const older = openTaskConversation(url);
	await openTaskConversation("https://github.com/acme/remote-repo/pull/43");
	finish(null);
	await older;
	expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://github.com/acme/remote-repo/pull/43");
});

// Catches: returning to a PR's branch by head name alone, or rendering a dead action for a removed checkout.
it("returns through the host branch-selection path only while the exact PR has a live workspace", async () => {
	const value = detail();
	value.item.itemType = "pull";
	value.item.url = url;
	value.comments.nextCursor = null;
	register(provider({ detail: async () => structuredClone(value) }));
	branches.feature = { branch: "feature", url };
	branches.wrong = { branch: "wrong", url: "https://github.com/other/repo/pull/42" };
	workspaceIdOnBranch.mockImplementation((_repo, branch) => (branch === "feature" ? "workspace-42" : null));
	const open = vi.fn();
	const unregister = registerTaskWorkspaceOpener(open);
	try {
		render(() => <TasksShell />);
		await openTaskConversation(url);
		fireEvent.click(await screen.findByRole("button", { name: "Open branch feature" }));
		expect(open).toHaveBeenCalledExactlyOnceWith("/repo", "workspace-42");
		workspaceIdOnBranch.mockReturnValue(null);
		fireEvent.click(
			within(screen.getByRole("region", { name: "Task detail" })).getByRole("button", { name: "Refresh" }),
		);
		await waitFor(() => expect(screen.queryByRole("button", { name: "Open branch feature" })).not.toBeInTheDocument());
	} finally {
		unregister();
	}
});
