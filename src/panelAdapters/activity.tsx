import { Component, createSignal, createEffect, onMount } from "solid-js";
import { invoke } from "../invoke";
import { buildActivitySnapshot, type ActivitySnapshot, type ActivityTerminalRow } from "../utils/activitySnapshot";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { ActivityDashboard, type TerminalRow } from "../components/ActivityDashboard/ActivityDashboard";
import s from "../components/ActivityDashboard/ActivityDashboard.module.css";
import type { PanelAdapter } from "../panelRouter";

function getTerminalStatus(
  row: ActivityTerminalRow,
): { label: string; className: string } {
  if (row.isRateLimited) {
    return { label: "Rate limited", className: s.statusRateLimited };
  } else if (row.awaitingInput) {
    return { label: "Waiting for input", className: s.statusWaiting };
  } else if (row.shellState === "busy") {
    return { label: "Working", className: s.statusWorking };
  } else if (row.shellState === "idle") {
    return { label: "Idle", className: s.statusIdle };
  }
  return { label: "—", className: s.statusIdle };
}

function projectName(cwd: string | null): string | null {
  if (!cwd) return null;
  const segments = cwd.replace(/\/+$/, "").split("/");
  return segments[segments.length - 1] || null;
}

function snapshotToRows(snap: ActivitySnapshot): TerminalRow[] {
  return snap.terminals.map((t) => ({
    id: t.id,
    name: t.name,
    project: projectName(t.cwd),
    agent: t.agentType || "shell",
    status: getTerminalStatus(t),
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
