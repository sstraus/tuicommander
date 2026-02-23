import { Component } from "solid-js";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { DiffPanel } from "./DiffPanel";
import { uiStore } from "../stores/ui";
import { terminalsStore } from "../stores/terminals";

export interface PanelOrchestratorProps {
  repoPath: string | null;
  onFileOpen: (repoPath: string, filePath: string) => void;
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

      <DiffPanel
        visible={uiStore.state.diffPanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleDiffPanel()}
      />
    </>
  );
};
