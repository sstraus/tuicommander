import { Component, onMount } from "solid-js";
import { repositoriesStore } from "../stores/repositories";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { MarkdownPanel } from "../components/MarkdownPanel";
import type { PanelAdapter } from "../panelRouter";
import { uiStore } from "../stores/ui";

const DetachedMarkdownPanel: Component<{ params: URLSearchParams }> = (props) => {
  const repoPath = props.params.get("repoPath");
  const fsRoot = props.params.get("fsRoot");

  onMount(() => {
    void initPanelWindow();
  });

  return (
    <MarkdownPanel
      visible={true}
      repoPath={repoPath}
      fsRoot={fsRoot}
      mode="detached"
      onClose={() => window.close()}
    />
  );
};

function getActiveFsRoot(): string | undefined {
  const activeRepo = repositoriesStore.getActive();
  if (!activeRepo?.activeBranch) return undefined;
  return activeRepo.branches[activeRepo.activeBranch]?.worktreePath || activeRepo.path;
}

export const markdownPanelAdapter: PanelAdapter = {
  id: "markdown",
  title: "Markdown",
  defaultSize: { width: 500, height: 700 },
  toggle: () => uiStore.toggleMarkdownPanel(),
  onDetach: () => uiStore.setMarkdownPanelVisible(false),
  detachParams: () => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    const fsRoot = getActiveFsRoot();
    return {
      ...(repoPath ? { repoPath } : {}),
      ...(fsRoot ? { fsRoot } : {}),
    };
  },
  Component: DetachedMarkdownPanel,
};
