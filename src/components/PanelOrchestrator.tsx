import { Component } from "solid-js";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { PlanPanel } from "./PlanPanel/PlanPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { uiStore } from "../stores/ui";
import { terminalsStore } from "../stores/terminals";

export interface PanelOrchestratorProps {
  repoPath: string | null;
  onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
  return (
    <>
      <FileBrowserPanel
        visible={uiStore.state.fileBrowserPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleFileBrowserPanel()}
        onFileOpen={props.onFileOpen}
      />

      <MarkdownPanel
        visible={uiStore.state.markdownPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleMarkdownPanel()}
      />

      <NotesPanel
        visible={uiStore.state.notesPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleNotesPanel()}
        onSendToTerminal={(text) => {
          const active = terminalsStore.getActive();
          if (active?.ref) {
            active.ref.write(`${text}\r`);
            requestAnimationFrame(() => active.ref?.focus());
          }
        }}
      />

      <PlanPanel
        visible={uiStore.state.planPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.togglePlanPanel()}
      />

      <GitPanel
        visible={uiStore.state.gitPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleGitPanel()}
      />
    </>
  );
};
