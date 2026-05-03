import { Component, onMount } from "solid-js";
import { invoke } from "../invoke";
import { repositoriesStore } from "../stores/repositories";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { classifyFile } from "../utils/filePreview";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { FileBrowserPanel } from "../components/FileBrowserPanel";
import type { PanelAdapter } from "../panelRouter";
import { uiStore } from "../stores/ui";

const DetachedFileBrowser: Component<{ params: URLSearchParams }> = (props) => {
  const repoPath = props.params.get("repoPath");
  const fsRoot = props.params.get("fsRoot");
  const { emitAction } = createPanelSyncReceiver<null>("file-browser");

  onMount(() => {
    void initPanelWindow();
  });

  return (
    <FileBrowserPanel
      visible={true}
      repoPath={repoPath}
      fsRoot={fsRoot}
      mode="detached"
      onClose={() => window.close()}
      onFileOpen={(repo, filePath, line) => {
        void emitAction("openFile", { repoPath: repo, filePath, line });
        void invoke("focus_main_window");
      }}
    />
  );
};

function getActiveFsRoot(): string | undefined {
  const activeRepo = repositoriesStore.getActive();
  if (!activeRepo?.activeBranch) return undefined;
  return activeRepo.branches[activeRepo.activeBranch]?.worktreePath || activeRepo.path;
}

export const fileBrowserPanelAdapter: PanelAdapter = {
  id: "file-browser",
  title: "File Browser",
  defaultSize: { width: 400, height: 700 },
  toggle: () => uiStore.toggleFileBrowserPanel(),
  onDetach: () => uiStore.setFileBrowserPanelVisible(false),
  detachParams: () => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    const fsRoot = getActiveFsRoot();
    return {
      ...(repoPath ? { repoPath } : {}),
      ...(fsRoot ? { fsRoot } : {}),
    };
  },
  handleAction(action: string, data: unknown) {
    if (action === "openFile" && data) {
      const d = data as Record<string, unknown>;
      const fsRoot = d.repoPath as string;
      const filePath = d.filePath as string;
      const line = d.line as number | undefined;
      const target = classifyFile(filePath);
      const repoPath = repositoriesStore.state.activeRepoPath || fsRoot;
      if (target === "markdown" && line === undefined) {
        mdTabsStore.add(repoPath, filePath, fsRoot || undefined);
      } else if (target === "preview" && line === undefined) {
        mdTabsStore.addHtmlPreview(repoPath, filePath, fsRoot || undefined);
      } else {
        editorTabsStore.add(fsRoot, filePath, line);
      }
      void invoke("focus_main_window");
    }
  },
  Component: DetachedFileBrowser,
};
