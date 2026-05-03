import { Component, Show } from "solid-js";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { AIChatPanel } from "./AIChatPanel";
import { AiTriagePanel } from "./AiTriagePanel";
import { DetachedPlaceholder } from "./DetachedPlaceholder";
import { diffTabsStore } from "../stores/diffTabs";
import { uiStore } from "../stores/ui";
import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { settingsStore } from "../stores/settings";

export interface PanelOrchestratorProps {
  repoPath: string | null;
  /** Effective filesystem root (worktree path when on a linked worktree) */
  fsRoot?: string | null;
  onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
  return (
    <>
      <Show
        when={!uiStore.isDetached("file-browser")}
        fallback={<DetachedPlaceholder panel="File Browser" panelId="file-browser" />}
      >
        <FileBrowserPanel
          visible={uiStore.state.fileBrowserPanelVisible && !globalWorkspaceStore.isActive()}
          repoPath={props.repoPath}
          fsRoot={props.fsRoot}
          onClose={() => uiStore.toggleFileBrowserPanel()}
          onFileOpen={props.onFileOpen}
        />
      </Show>

      <Show
        when={!uiStore.isDetached("markdown")}
        fallback={<DetachedPlaceholder panel="Markdown" panelId="markdown" />}
      >
        <MarkdownPanel
          visible={uiStore.state.markdownPanelVisible}
          repoPath={props.repoPath}
          fsRoot={props.fsRoot}
          onClose={() => uiStore.toggleMarkdownPanel()}
        />
      </Show>

      <Show
        when={!uiStore.isDetached("notes")}
        fallback={<DetachedPlaceholder panel="Notes" panelId="notes" />}
      >
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
      </Show>

      <Show
        when={!uiStore.isDetached("git")}
        fallback={<DetachedPlaceholder panel="Git" panelId="git" />}
      >
        <GitPanel
          visible={uiStore.state.gitPanelVisible && !globalWorkspaceStore.isActive()}
          repoPath={props.repoPath}
          fsRoot={props.fsRoot}
          onClose={() => uiStore.toggleGitPanel()}
          requestedTab={uiStore.state.gitPanelRequestedTab}
          onOpenDiff={diffTabsStore.add.bind(diffTabsStore)}
        />
      </Show>

      <Show when={settingsStore.isAiChatEnabled() && !uiStore.isDetached("ai-chat")}>
        <AIChatPanel
          visible={uiStore.state.aiChatPanelVisible}
          onClose={() => uiStore.toggleAiChatPanel()}
        />
      </Show>

      <AiTriagePanel
        visible={uiStore.state.aiTriagePanelVisible}
        repoPath={props.repoPath}
        onClose={() => uiStore.toggleAiTriagePanel()}
      />
    </>
  );
};
