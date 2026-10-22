import { type Component, createEffect, createSignal, lazy, Show, Suspense } from "solid-js";
import { diffTabsStore } from "../stores/diffTabs";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { progressStore } from "../stores/progress";
import { settingsStore } from "../stores/settings";
import { storiesUi } from "../stores/storiesUi";
import { uiStore } from "../stores/ui";
import {
	canQueueToActiveTerminal,
	queueTextToActiveTerminal,
	sendTextToActiveTerminal,
} from "../utils/sendToActiveTerminal";
import { AutomationsHost } from "./AutomationsDialog/AutomationsHost";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { IdeasPanel } from "./IdeasPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { OutlinePanel } from "./OutlinePanel";
import { ProgressDialog } from "./ProgressDialog";
import { ReferencesPanel } from "./ReferencesPanel";

const StoriesDialog = lazy(() =>
	import("./StoriesDialog/StoriesDialog").then((module) => ({ default: module.StoriesDialog })),
);
const AIChatPanel = lazy(() => import("./AIChatPanel").then((module) => ({ default: module.AIChatPanel })));

export interface PanelOrchestratorProps {
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onOpenSettings?: (tab: string, section?: string) => void;
	onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

/**
 * Whether the hand-promoted, cross-repo global workspace is showing.
 *
 * A per-repo auto-consolidated workspace (#e767) has a single, well-defined
 * repo — `props.repoPath`/`fsRoot` still resolve correctly — so it must not
 * suppress these panels the way the manual, potentially cross-repo workspace
 * does.
 */
function manualWorkspaceActive(): boolean {
	return globalWorkspaceStore.isActive() && globalWorkspaceStore.getScope() === MANUAL_SCOPE;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
	const [aiChatRequested, setAiChatRequested] = createSignal(uiStore.state.aiChatPanelVisible);
	createEffect(() => {
		if (uiStore.state.aiChatPanelVisible) setAiChatRequested(true);
	});
	return (
		<>
			<AutomationsHost />
			<Show when={!uiStore.isDetached("file-browser")}>
				<FileBrowserPanel
					visible={uiStore.state.fileBrowserPanelVisible && !manualWorkspaceActive()}
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
				<IdeasPanel
					visible={uiStore.state.ideasPanelVisible}
					repoPath={props.repoPath}
					onClose={() => uiStore.toggleIdeasPanel()}
					onSendToTerminal={(text) => void sendTextToActiveTerminal(text)}
					onQueueToTerminal={canQueueToActiveTerminal() ? (text) => void queueTextToActiveTerminal(text) : undefined}
				/>
			</Show>

			<Show when={!uiStore.isDetached("outline") && uiStore.state.outlinePanelVisible}>
				<OutlinePanel visible={true} onClose={() => uiStore.toggleOutlinePanel()} />
			</Show>

			<Show when={!uiStore.isDetached("references") && uiStore.state.referencesPanelVisible}>
				<ReferencesPanel visible={true} onClose={() => uiStore.toggleReferencesPanel()} />
			</Show>

			<Show when={!uiStore.isDetached("git")}>
				<GitPanel
					visible={uiStore.state.gitPanelVisible && !manualWorkspaceActive()}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleGitPanel()}
					requestedTab={uiStore.state.gitPanelRequestedTab}
					onOpenDiff={diffTabsStore.add.bind(diffTabsStore)}
				/>
			</Show>

			<Show when={settingsStore.isAiChatEnabled() && !uiStore.isDetached("ai-chat") && aiChatRequested()}>
				<Suspense>
					<AIChatPanel
						visible={uiStore.state.aiChatPanelVisible}
						repoPath={props.repoPath}
						fsRoot={props.fsRoot}
						onClose={() => uiStore.toggleAiChatPanel()}
						onOpenSettings={props.onOpenSettings}
					/>
				</Suspense>
			</Show>

			<Show when={progressStore.dialogVisible()}>
				<ProgressDialog />
			</Show>
			<Show when={storiesUi.visible()}>
				<Show when={storiesUi.project()} keyed>
					{(project) => (
						<Suspense>
							<StoriesDialog project={project} onClose={() => storiesUi.close()} />
						</Suspense>
					)}
				</Show>
			</Show>
		</>
	);
};
