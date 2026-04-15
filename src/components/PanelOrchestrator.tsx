import { Component } from "solid-js";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { AIChatPanel } from "./AIChatPanel";
import { uiStore } from "../stores/ui";
import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";

export interface PanelOrchestratorProps {
  repoPath: string | null;
  /** Effective filesystem root (worktree path when on a linked worktree) */
  fsRoot?: string | null;
  onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
  return (
    <>
      <FileBrowserPanel
        visible={uiStore.state.fileBrowserPanelVisible && !globalWorkspaceStore.isActive()}
        repoPath={props.repoPath}
        fsRoot={props.fsRoot}
        onClose={() => uiStore.toggleFileBrowserPanel()}
        onFileOpen={props.onFileOpen}
      />

      <MarkdownPanel
        visible={uiStore.state.markdownPanelVisible}
        repoPath={props.repoPath}
        fsRoot={props.fsRoot}
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

      <GitPanel
        visible={uiStore.state.gitPanelVisible && !globalWorkspaceStore.isActive()}
        repoPath={props.repoPath}
        fsRoot={props.fsRoot}
        onClose={() => uiStore.toggleGitPanel()}
        requestedTab={uiStore.state.gitPanelRequestedTab}
      />

      <AIChatPanel
        visible={uiStore.state.aiChatPanelVisible}
        onClose={() => uiStore.toggleAiChatPanel()}
      />
    </>
  );
};
