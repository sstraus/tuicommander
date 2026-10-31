/** Isolated real-component navigation fixture over synthetic core-domain data; no API compatibility claim. */
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import "../global.css";
import "../styles.css";
import { urlResolverRegistry } from "../components/CommandPalette/urlPaste";
import { PrDetailPopover } from "../components/PrDetailPopover/PrDetailPopover";
import { TasksShell } from "../components/Tasks/TasksShell";
import { Button } from "../components/ui/Button";
import { createInMemoryWorkItemProvider } from "../plugins/testing/inMemoryWorkItemProvider";
import { workItemsDemo } from "../plugins/testing/work-items-demo.gen";
import { workItemProviderRegistry } from "../plugins/workItemProviderRegistry";
import { registerTaskWorkspaceOpener } from "../services/taskNavigation";
import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import type { WorkItemProviderSnapshot } from "../types/work-items.gen";
import { buildPrStatus } from "./presets";
import "./prConversationPreview.css";

const repoPath = "/synthetic/acme/remote";
const branch = "feature/remote-comments";
const url = "https://github.com/acme/remote/pull/42";
repositoriesStore.add({ path: repoPath, displayName: "remote" });
repositoriesStore.setWorkspace(repoPath, branch, { worktreePath: "/synthetic/worktree" });
repositoriesStore.setActive(repoPath);
githubStore.updateRepoData(repoPath, [
	buildPrStatus(branch, {
		number: 42,
		title: "Keep PR comments connected to the workspace",
		url,
		unresolved_threads: 2,
		checks: { passed: 0, failed: 0, pending: 0, total: 0 },
		check_details: [],
	}),
]);
const seed: WorkItemProviderSnapshot = structuredClone(workItemsDemo);
seed.providerId = "github";
seed.accountId = "fixture";
seed.name = "GitHub · synthetic account";
const first = seed.items[0];
seed.items = [first];
first.item.reference = { providerId: "github", accountId: "fixture", id: "acme/remote/pull/42" };
first.item.itemType = "pull";
first.item.title = "Keep PR comments connected to the workspace";
first.item.key = "acme/remote#42";
first.item.url = url;
first.body = "This synthetic PR connects the sidebar, Tasks comments and the existing workspace.";
first.fields = [];
first.actions = [];
first.comments = {
	entries: [
		{
			kind: "normal",
			value: {
				id: "normal",
				author: { id: "alice", name: "Alice", bot: false, avatarUrl: null },
				body: "Please keep the conversation reachable from the sidebar.",
				createdAt: "2026-10-31T10:00:00Z",
				url,
			},
		},
		{
			kind: "review_thread",
			value: {
				id: "thread",
				path: "src/services/taskNavigation.ts",
				line: 24,
				originalLine: null,
				commitId: "synthetic",
				resolved: false,
				outdated: false,
				finding: null,
				comments: [
					{
						id: "review",
						author: { id: "bob", name: "Bob", bot: false, avatarUrl: null },
						body: "Use the PR URL, not the active branch, so remote-only PRs open the right discussion.",
						createdAt: "2026-10-31T10:05:00Z",
						url,
					},
				],
			},
		},
	],
	nextCursor: null,
};
const provider = createInMemoryWorkItemProvider(seed);
// This visual fixture has no Rust projection endpoint; keep listing provider-owned.
const list = provider.list.bind(provider);
provider.list = async (query, signal) => {
	const { total: _total, ...page } = await list(query, signal);
	return page;
};
function Preview() {
	const [popover, setPopover] = createSignal(true);
	const [loaded, setLoaded] = createSignal(true);
	const [location, setLocation] = createSignal("No branch selected by Tasks yet");
	let registration = workItemProviderRegistry.register(provider);
	urlResolverRegistry.register("fixture", async () => ({
		link: {
			reference: first.item.reference,
			key: first.item.key,
			title: first.item.title,
			url,
			state: first.item.state,
		},
	}));
	registerTaskWorkspaceOpener((repo, workspace) => setLocation(`Opened ${repo} · ${workspace}`));
	return (
		<div class="pr-conversation-fixture">
			<header>
				<Button onClick={() => setPopover(true)}>Show PR popover</Button>
				<Button
					onClick={() => {
						if (loaded()) registration.dispose();
						else registration = workItemProviderRegistry.register(provider);
						setLoaded(!loaded());
					}}
				>
					Provider: {loaded() ? "loaded" : "unloaded"}
				</Button>
				<span role="status">{location()}</span>
			</header>
			<main>
				<TasksShell />
			</main>
			<Show when={popover()}>
				<PrDetailPopover repoPath={repoPath} branch={branch} anchor="top" onClose={() => setPopover(false)} />
			</Show>
		</div>
	);
}
render(() => <Preview />, document.getElementById("root")!);
