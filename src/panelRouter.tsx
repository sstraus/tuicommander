import { Component, JSX } from "solid-js";
import { invoke } from "./invoke";
import { emitTo } from "@tauri-apps/api/event";
import { uiStore } from "./stores/ui";

export interface PanelAdapter {
  id: string;
  title: string;
  defaultSize: { width: number; height: number };
  Component: Component<{ params: URLSearchParams }>;
  handleAction?: (action: string, data: unknown) => void;
  toggle?: () => void;
  detachParams?: () => Record<string, string>;
  onDetach?: () => void;
  /** For projection panels: serializer for cross-window sync snapshots. */
  serialize?: () => unknown;
  /** Sync push interval in ms (default: no sync). Must be set alongside serialize. */
  syncIntervalMs?: number;
}

const panelRegistry: Record<string, PanelAdapter> = {};

export function registerPanel(adapter: PanelAdapter): void {
  panelRegistry[adapter.id] = adapter;
}

export function getPanelParams(): {
  isPanelMode: boolean;
  panelId: string | null;
  params: URLSearchParams;
} {
  const params = new URLSearchParams(window.location.search);
  return {
    isPanelMode: params.get("mode") === "panel",
    panelId: params.get("panel"),
    params,
  };
}

export function renderPanelMode(): JSX.Element | null {
  const { isPanelMode, panelId, params } = getPanelParams();
  if (!isPanelMode || !panelId) return null;

  const adapter = panelRegistry[panelId];
  if (!adapter) return null;

  return (
    <div id="app" class={`panel-mode panel-${panelId}`}>
      <adapter.Component params={params} />
    </div>
  );
}

export async function detachPanel(panelId: string): Promise<void> {
  const adapter = panelRegistry[panelId];
  if (!adapter) return;
  await invoke("open_panel_window", {
    panelId,
    title: adapter.title,
    params: adapter.detachParams?.() ?? {},
    width: adapter.defaultSize.width,
    height: adapter.defaultSize.height,
  });
  uiStore.setDetached(panelId, `panel-${panelId}`);
  adapter.onDetach?.();
}

export function togglePanel(panelId: string): boolean {
  const adapter = panelRegistry[panelId];
  if (!adapter?.toggle) return false;
  if (uiStore.isDetached(panelId)) {
    invoke("focus_panel_window", { panelId }).catch(() => {
      uiStore.clearDetached(panelId);
      adapter.toggle?.();
    });
    return true;
  }
  adapter.toggle();
  return true;
}

export async function reattachPanel(panelId: string): Promise<void> {
  await emitTo("main", "panel-action", { panelId, action: "reattach", data: {} });
  await invoke("close_panel_window", { panelId });
}

export async function closePanel(panelId: string): Promise<void> {
  await invoke("close_panel_window", { panelId });
}

export { panelRegistry };
