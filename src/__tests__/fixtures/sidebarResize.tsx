import { render } from "solid-js/web";
import { Sidebar } from "../../components/Sidebar/Sidebar";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { uiStore } from "../../stores/ui";
import { workItemLinksStore } from "../../stores/workItemLinks";
import "../../global.css";

// Component-browser fixture: backend calls are isolated by the HTML adapter.
const query = new URLSearchParams(location.search);
const path = "/sidebar-resize-fixture";
const branch = "fix/2718";
const calls = { add: 0, remove: 0, select: 0 };
repositoriesStore.add({ path, displayName: "SIDEBAR RESIZE CHECK", initials: "SC" });
const terminal = terminalsStore.add({
	name: "COORDINATOR 2704",
	cwd: path,
	sessionId: null,
	fontSize: 13,
	awaitingInput: null,
});
repositoriesStore.setWorkspace(path, "main", { terminals: [terminal] });
repositoriesStore.setWorkspace(path, branch, {
	worktreePath: "/sidebar-resize-worktree",
	additions: query.get("badges") === "1" ? 12345 : 0,
	deletions: query.get("badges") === "1" ? 12345 : 0,
});
for (let i = 0; i < 30; i++) repositoriesStore.setWorkspace(path, `other/${i}`);
repositoriesStore.setActive(path);
settingsStore.setTabTreeEnabled(true);
uiStore.setSidebarWidth(300);
uiStore.cycleSidebarDensityMode();
if (query.get("density") === "rich") uiStore.cycleSidebarDensityMode();
if (query.get("badges") === "1") {
	githubStore.updateRepoData(path, [
		{
			branch,
			number: 123,
			title: "Sidebar resize",
			state: "OPEN",
			is_draft: false,
			url: "https://example.invalid/pull/123",
			additions: 12345,
			deletions: 12345,
			checks: { passed: 0, failed: 0, pending: 0, total: 0 },
			author: "fixture",
			commits: 1,
			mergeable: "MERGEABLE",
			merge_state_status: "CLEAN",
			review_decision: "",
			viewer_did_approve: false,
			labels: [],
			base_ref_name: "main",
			head_ref_oid: "fixture",
			created_at: "2026-11-02T00:00:00Z",
			updated_at: "2026-11-02T00:00:00Z",
			conflict_state: "clear",
			merge_state_label: null,
			review_state_label: null,
			merge_commit_allowed: true,
			squash_merge_allowed: true,
			rebase_merge_allowed: true,
			unresolved_threads: 0,
			unresolved_threads_truncated: false,
		},
	]);
	workItemLinksStore.get(path, branch);
	await workItemLinksStore.refresh();
	await workItemLinksStore.link(path, branch, {
		key: "ACME-2718",
		title: "Sidebar resize task",
		state: null,
		url: "https://example.invalid/task",
		reference: { providerId: "fixture", accountId: "fixture", id: "2718" },
	});
}
const noop = () => {};
render(
	() => (
		<Sidebar
			onBranchSelect={() => calls.select++}
			onAddTerminal={() => calls.add++}
			onRemoveBranch={() => calls.remove++}
			onRenameBranch={noop}
			onAddWorktree={noop}
			onAddRepo={noop}
			onRepoSettings={noop}
			onRemoveRepo={noop}
			onOpenSettings={noop}
		/>
	),
	document.getElementById("root")!,
);
Object.assign(window, { sidebarResizeCalls: calls });
