import { clearRepoFilters, sidebarRepoFilter } from "../components/Sidebar/repoFilter";
import { repositoriesStore } from "../stores/repositories";

/**
 * Make a repository's sidebar row visible: clear any filter that hides it and
 * expand its group and the repo itself, then scroll after the DOM re-renders.
 * Marking it active is the caller's job (`repositoriesStore.setActive`).
 */
export function revealRepoInSidebar(repoPath: string): void {
	const repo = repositoriesStore.get(repoPath);
	if (repo && sidebarRepoFilter()?.(repo) === false) clearRepoFilters();
	const group = repositoriesStore.getGroupForRepo(repoPath);
	if (group?.collapsed) repositoriesStore.toggleGroupCollapsed(group.id);
	if (repo?.collapsed) repositoriesStore.toggleCollapsed(repoPath);
	if (repo && !repo.expanded) repositoriesStore.toggleExpanded(repoPath);

	requestAnimationFrame(() => {
		document.querySelector(`[data-sidebar-repo="${CSS.escape(repoPath)}"]`)?.scrollIntoView({ block: "nearest" });
	});
}
