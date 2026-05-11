import { type Component, Show } from "solid-js";
import { diffTabsStore } from "../stores/diffTabs";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";
import { uiStore } from "../stores/ui";
import { AIChatPanel } from "./AIChatPanel";
import { AiTriagePanel } from "./AiTriagePanel";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { OutlinePanel } from "./OutlinePanel";
import { ReferencesPanel } from "./ReferencesPanel";

export interface PanelOrchestratorProps {
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
	return (
		<>
			<Show when={!uiStore.isDetached("file-browser")}>
				<FileBrowserPanel
					visible={uiStore.state.fileBrowserPanelVisible && !globalWorkspaceStore.isActive()}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleFileBrowserPanel()}
					onFileOpen={props.onFileOpen}
				/>
			</Show>

			<Show when={!uiStore.isDetached("markdown")}>
				<MarkdownPanel
					visible={uiStore.state.markdownPanelVisible}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleMarkdownPanel()}
				/>
			</Show>

			<Show when={!uiStore.isDetached("notes")}>
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

			<Show when={!uiStore.isDetached("outline")}>
				<OutlinePanel
					visible={uiStore.state.outlinePanelVisible}
					onClose={() => uiStore.toggleOutlinePanel()}
				/>
			</Show>

			<Show when={!uiStore.isDetached("references")}>
				<ReferencesPanel
					visible={uiStore.state.referencesPanelVisible}
					onClose={() => uiStore.toggleReferencesPanel()}
				/>
			</Show>

			<Show when={!uiStore.isDetached("git")}>
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
				<AIChatPanel visible={uiStore.state.aiChatPanelVisible} onClose={() => uiStore.toggleAiChatPanel()} />
			</Show>

			<AiTriagePanel
				visible={uiStore.state.aiTriagePanelVisible}
				repoPath={props.repoPath}
				onClose={() => uiStore.toggleAiTriagePanel()}
			/>
		</>
	);
};
