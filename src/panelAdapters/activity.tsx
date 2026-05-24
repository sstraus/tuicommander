import { type Component, createEffect, createSignal, onMount } from "solid-js";
import { ActivityDashboard, statusClasses, type TerminalRow } from "../components/ActivityDashboard/ActivityDashboard";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { invoke } from "../invoke";
import type { PanelAdapter } from "../panelRouter";
import { activityDashboardStore } from "../stores/activityDashboard";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import {
	type ActivitySnapshot,
	buildActivitySnapshot,
	projectName,
	terminalStatusLabel,
} from "../utils/activitySnapshot";
import { navigateToTerminal } from "../utils/navigateToTerminal";
import { createPanelSyncReceiver } from "../utils/panelSync";

function snapshotToRows(snap: ActivitySnapshot): TerminalRow[] {
	return snap.terminals.map((t) => ({
		id: t.id,
		name: t.name,
		project: projectName(t.cwd),
		projectColor: undefined,
		agent: t.agentType || "shell",
		status: terminalStatusLabel(t.shellState, t.awaitingInput, t.isRateLimited, statusClasses),
		isWorking: t.isRateLimited || !!t.awaitingInput || t.shellState === "busy",
		lastDataAt: t.lastDataAt,
		idleSince: t.idleSince,
		lastPrompt: t.lastPrompt,
		agentIntent: t.agentIntent,
		currentTask: t.currentTask,
		activeSubTasks: t.activeSubTasks,
		isActive: t.isActive,
		isPromoted: t.isPromoted,
	}));
}

const DetachedActivityDashboard: Component<{ params: URLSearchParams }> = () => {
	const { state, emitAction } = createPanelSyncReceiver<ActivitySnapshot>("activity");
	const [rows, setRows] = createSignal<TerminalRow[]>([]);

	onMount(() => {
		void initPanelWindow();
	});

	createEffect(() => {
		const snap = state();
		if (snap) setRows(snapshotToRows(snap));
	});

	return (
		<ActivityDashboard
			embedded
			terminals={rows}
			onSelect={(id) => void emitAction("navigate", { termId: id })}
			onPromote={(id) => void emitAction("promote", { termId: id })}
		/>
	);
};

export const activityPanelAdapter: PanelAdapter = {
	id: "activity",
	title: "Activity Dashboard",
	defaultSize: { width: 550, height: 650 },
	toggle: () => activityDashboardStore.toggle(),
	onDetach: () => activityDashboardStore.close(),
	syncIntervalMs: 1000,
	serialize: buildActivitySnapshot,
	handleAction(action: string, data: unknown) {
		const d = data as Record<string, unknown> | null;
		if (typeof d?.termId !== "string") return;
		const termId = d.termId;
		if (action === "navigate") {
			void invoke("focus_main_window");
			navigateToTerminal(termId);
		} else if (action === "promote") {
			globalWorkspaceStore.togglePromote(termId);
		}
	},
	Component: DetachedActivityDashboard,
};
