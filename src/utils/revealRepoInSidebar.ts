import { repositoriesStore } from "../stores/repositories";

/**
 * Make a repository's sidebar row visible: expand its collapsed group and the
 * repo itself, then scroll the row into view once the DOM has re-rendered.
 * Marking it active is the caller's job (`repositoriesStore.setActive`).
 */
export function revealRepoInSidebar(repoPath: string): void {
	const group = repositoriesStore.getGroupForRepo(repoPath);
	if (group?.collapsed) repositoriesStore.toggleGroupCollapsed(group.id);
	const repo = repositoriesStore.get(repoPath);
	if (repo?.collapsed) repositoriesStore.toggleCollapsed(repoPath);
	if (repo && !repo.expanded) repositoriesStore.toggleExpanded(repoPath);

	requestAnimationFrame(() => {
		document.querySelector(`[data-sidebar-repo="${CSS.escape(repoPath)}"]`)?.scrollIntoView({ block: "nearest" });
	});
}
