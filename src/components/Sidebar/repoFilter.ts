import type { RepositoryState } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { hasLiveHarness } from "./liveHarness";

export const repoIsActive = (repo: RepositoryState): boolean =>
	Object.values(repo.workspaces).some((workspace) => workspace.terminals.length > 0);

export function sidebarRepoFilter(): ((repo: RepositoryState) => boolean) | null {
	if (uiStore.state.repoFilterHarnessOnly) {
		return (repo) => Object.values(repo.workspaces).some((workspace) => hasLiveHarness(workspace.terminals));
	}
	return uiStore.state.repoFilterActiveOnly ? repoIsActive : null;
}

export function clearRepoFilters(): void {
	uiStore.setRepoFilterActiveOnly(false);
	uiStore.setRepoFilterHarnessOnly(false);
}
