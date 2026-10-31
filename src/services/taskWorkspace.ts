import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import { workItemLinksStore } from "../stores/workItemLinks";
import type { WorkItemRef } from "../types/work-items.gen";

/** Resolve only live sidebar rows; a removed checkout must not leave a dead return action. */
export function findTaskWorkspace(reference: WorkItemRef, url: string) {
	const linked = workItemLinksStore.findByItem(reference);
	if (linked) {
		const workspaceId = repositoriesStore.workspaceIdOnBranch(linked.repoPath, linked.branch);
		if (workspaceId) return { ...linked, workspaceId };
	}
	// A PR need not have been created by Start work. Match its full URL rather than
	// its number or head name, which collide across repositories and forks.
	for (const [repoPath, repo] of Object.entries(githubStore.state.repos)) {
		for (const pr of Object.values(repo.branches)) {
			if (pr.url !== url) continue;
			const workspaceId = repositoriesStore.workspaceIdOnBranch(repoPath, pr.branch);
			if (workspaceId) return { repoPath, workspaceId, branch: pr.branch };
		}
	}
	return undefined;
}
