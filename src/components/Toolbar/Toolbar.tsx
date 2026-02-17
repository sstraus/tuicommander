import { Component, Show } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { getModifierSymbol } from "../../platform";
import { IdeLauncher } from "../IdeLauncher";

export interface ToolbarProps {
  repoPath?: string;
  runCommand?: string;
  quickSwitcherActive?: boolean;
  onBranchClick?: () => void;
  onRun?: (shiftKey: boolean) => void;
}

export const Toolbar: Component<ToolbarProps> = (props) => {
  const activeBranch = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return null;
    const repo = repositoriesStore.state.repositories[activeRepoPath];
    if (!repo?.activeBranch) return null;
    return repo.branches[repo.activeBranch] || null;
  };

  const activeBranchName = () => activeBranch()?.name || null;

  const activeRepoName = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return null;
    const repo = repositoriesStore.state.repositories[activeRepoPath];
    return repo?.displayName || null;
  };

  // Use the branch's worktree path (falls back to repo path)
  const launchPath = () => activeBranch()?.worktreePath || props.repoPath;

  return (
    <div id="toolbar" data-tauri-drag-region>
      <div class="toolbar-left" data-tauri-drag-region>
        <button
          class="toolbar-sidebar-toggle"
          onClick={() => uiStore.toggleSidebar()}
          title={uiStore.state.sidebarVisible ? `Hide Sidebar (${getModifierSymbol()}[)` : `Show Sidebar (${getModifierSymbol()}[)`}
          style={{ position: "relative" }}
        >
          ◧
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}[</span>
        </button>
      </div>

      <div class="toolbar-center" data-tauri-drag-region>
        <Show when={activeBranchName()}>
          <button
            class="toolbar-branch"
            onClick={(e) => {
              e.stopPropagation();
              props.onBranchClick?.();
            }}
            title="Rename branch"
          >
            <span class="toolbar-branch-icon">Y</span>
            <Show when={activeRepoName()}>
              <span class="toolbar-repo-name">{activeRepoName()}</span>
              <span class="toolbar-branch-separator">/</span>
            </Show>
            <span class="toolbar-branch-name">{activeBranchName()}</span>
          </button>
        </Show>
      </div>

      <div class="toolbar-right">
        <IdeLauncher repoPath={launchPath()} runCommand={props.runCommand} onRun={props.onRun} />
      </div>
    </div>
  );
};

export default Toolbar;
