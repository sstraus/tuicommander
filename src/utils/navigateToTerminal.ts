import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/**
 * Navigate to a terminal: switch repo/branch context, activate the terminal,
 * deactivate other tab stores, activate the correct pane group, and focus.
 */
export function navigateToTerminal(id: string): void {
	const repoPath = repositoriesStore.getRepoPathForTerminal(id);
	if (repoPath) {
		const repo = repositoriesStore.state.repositories[repoPath];
		if (repo) {
			for (const [branchName, branch] of Object.entries(repo.branches)) {
				if (branch.terminals.includes(id)) {
					if (repositoriesStore.state.activeRepoPath !== repoPath) {
						repositoriesStore.setActive(repoPath);
					}
					if (repo.activeBranch !== branchName) {
						repositoriesStore.setActiveBranch(repoPath, branchName);
					}
					break;
				}
			}
		}
	}
	terminalsStore.setActive(id);
	diffTabsStore.setActive(null);
	mdTabsStore.setActive(null);
	editorTabsStore.setActive(null);

	if (paneLayoutStore.isSplit()) {
		const groupId = paneLayoutStore.getGroupForTab(id);
		if (groupId) {
			paneLayoutStore.setActiveGroup(groupId);
			paneLayoutStore.setActiveTab(groupId, id);
		}
	}

	requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
}
