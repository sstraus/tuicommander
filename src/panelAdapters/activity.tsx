import { Component, createSignal, createEffect, onMount } from "solid-js";
import { invoke } from "../invoke";
import { buildActivitySnapshot, projectName, terminalStatusLabel, type ActivitySnapshot } from "../utils/activitySnapshot";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { ActivityDashboard, type TerminalRow } from "../components/ActivityDashboard/ActivityDashboard";
import s from "../components/ActivityDashboard/ActivityDashboard.module.css";
import type { PanelAdapter } from "../panelRouter";
import { activityDashboardStore } from "../stores/activityDashboard";

const statusClasses = {
  rateLimited: s.statusRateLimited,
  waiting: s.statusWaiting,
  working: s.statusWorking,
  idle: s.statusIdle,
};

function snapshotToRows(snap: ActivitySnapshot): TerminalRow[] {
  return snap.terminals.map((t) => ({
    id: t.id,
    name: t.name,
    project: projectName(t.cwd),
    agent: t.agentType || "shell",
    status: terminalStatusLabel(t.shellState, t.awaitingInput, t.isRateLimited, statusClasses),
    lastDataAt: t.lastDataAt,
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

export const activityPanelAdapter: PanelAdapter & {
  syncIntervalMs: number;
  serialize: () => ActivitySnapshot;
  handleAction: (action: string, data: unknown) => void;
} = {
  id: "activity",
  title: "Activity Dashboard",
  defaultSize: { width: 550, height: 650 },
  toggle: () => activityDashboardStore.toggle(),
  onDetach: () => activityDashboardStore.close(),
  syncIntervalMs: 1000,
  serialize: buildActivitySnapshot,
  handleAction(action: string, data: unknown) {
    if (action === "navigate") {
      const { termId } = data as { termId: string };
      void invoke("focus_main_window");
      terminalsStore.setActive(termId);
      requestAnimationFrame(() => terminalsStore.get(termId)?.ref?.focus());
    } else if (action === "promote") {
      const { termId } = data as { termId: string };
      globalWorkspaceStore.togglePromote(termId);
    }
  },
  Component: DetachedActivityDashboard,
};
