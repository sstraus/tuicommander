import { Component, Show } from "solid-js";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { repositoriesStore } from "../../stores/repositories";
import { paneLayoutKey } from "../../stores/savedPaneLayouts";
import { GlobeIcon } from "../GlobeIcon";
import s from "./Sidebar.module.css";

/** Build the savedPaneLayouts key for the currently active repo+branch */
function currentRepoLayoutKey(): string | undefined {
  const repoPath = repositoriesStore.state.activeRepoPath;
  if (!repoPath) return undefined;
  const repo = repositoriesStore.state.repositories[repoPath];
  if (!repo?.activeBranch) return undefined;
  return paneLayoutKey(repoPath, repo.activeBranch);
}

export const GlobalWorkspaceEntry: Component = () => {
  const handleClick = () => {
    const key = currentRepoLayoutKey();
    if (globalWorkspaceStore.isActive()) {
      globalWorkspaceStore.deactivate(key);
    } else {
      globalWorkspaceStore.activate(key);
    }
  };

  return (
    <Show when={globalWorkspaceStore.hasPromoted()}>
      <div
        class={`${s.globalWorkspaceEntry} ${globalWorkspaceStore.isActive() ? s.globalWorkspaceActive : ""}`}
        onClick={handleClick}
      >
        <GlobeIcon />
        <span class={s.globalWorkspaceLabel}>Global</span>
        <span class={s.globalWorkspaceBadge}>{globalWorkspaceStore.getPromotedIds().length}</span>
      </div>
    </Show>
  );
};
