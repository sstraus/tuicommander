import { type Component, onMount } from "solid-js";
import { NotesPanel } from "../components/NotesPanel";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { invoke } from "../invoke";
import type { PanelAdapter } from "../panelRouter";
import { repositoriesStore } from "../stores/repositories";
import { uiStore } from "../stores/ui";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { sendTextToActiveTerminal } from "../utils/sendToActiveTerminal";

const DetachedNotesPanel: Component<{ params: URLSearchParams }> = (props) => {
	const repoPath = props.params.get("repoPath");
	const { emitAction } = createPanelSyncReceiver<null>("notes");

	onMount(() => {
		void initPanelWindow();
	});

	return (
		<NotesPanel
			visible={true}
			repoPath={repoPath}
			mode="detached"
			onClose={() => window.close()}
			onSendToTerminal={(text) => {
				void emitAction("sendToTerminal", { text });
				void invoke("focus_main_window");
			}}
		/>
	);
};

export const notesPanelAdapter: PanelAdapter = {
	id: "notes",
	title: "Notes",
	defaultSize: { width: 450, height: 600 },
	toggle: () => uiStore.toggleNotesPanel(),
	onDetach: () => uiStore.setNotesPanelVisible(false),
	detachParams: (): Record<string, string> => {
		const repoPath = repositoriesStore.state.activeRepoPath;
		return repoPath ? { repoPath } : {};
	},
	async handleAction(action: string, data: unknown) {
		if (action === "sendToTerminal" && data) {
			const d = data as Record<string, unknown>;
			await sendTextToActiveTerminal(d.text as string);
			void invoke("focus_main_window");
		}
	},
	Component: DetachedNotesPanel,
};
