import { createSignal } from "solid-js";
import type { Disposable, SidebarItem } from "../plugins/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface SidebarPanelOptions {
  /** Unique panel identifier (scoped to the plugin) */
  id: string;
  /** Section header text */
  label: string;
  /** Inline SVG for header icon */
  icon?: string;
  /** Lower = higher in sidebar (default 100) */
  priority?: number;
  /** Initial collapsed state (default true) */
  collapsed?: boolean;
}

export interface SidebarPanelHandle extends Disposable {
  /** Replace all items in the panel */
  setItems(items: SidebarItem[]): void;
  /** Set badge text on the header (e.g. "3"), or null to clear */
  setBadge(text: string | null): void;
}

export interface SidebarPanelState {
  pluginId: string;
  id: string;
  label: string;
  icon: string | null;
  priority: number;
  collapsed: boolean;
  items: SidebarItem[];
  badge: string | null;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

function createSidebarPluginStore() {
  const [panels, setPanels] = createSignal<SidebarPanelState[]>([]);

  function registerPanel(pluginId: string, options: SidebarPanelOptions): SidebarPanelHandle {
    const key = `${pluginId}:${options.id}`;
    const state: SidebarPanelState = {
      pluginId,
      id: options.id,
      label: options.label,
      icon: options.icon ?? null,
      priority: options.priority ?? 100,
      collapsed: options.collapsed ?? true,
      items: [],
      badge: null,
    };

    // Replace existing panel with same key, or add new
    setPanels((prev) => {
      const filtered = prev.filter((p) => `${p.pluginId}:${p.id}` !== key);
      return [...filtered, state].sort((a, b) => a.priority - b.priority);
    });

    return {
      setItems(items: SidebarItem[]) {
        setPanels((prev) =>
          prev.map((p) => `${p.pluginId}:${p.id}` === key ? { ...p, items } : p),
        );
      },
      setBadge(text: string | null) {
        setPanels((prev) =>
          prev.map((p) => `${p.pluginId}:${p.id}` === key ? { ...p, badge: text } : p),
        );
      },
      dispose() {
        setPanels((prev) => prev.filter((p) => `${p.pluginId}:${p.id}` !== key));
      },
    };
  }

  function getPanels(): SidebarPanelState[] {
    return panels();
  }

  function toggleCollapsed(pluginId: string, panelId: string): void {
    const key = `${pluginId}:${panelId}`;
    setPanels((prev) =>
      prev.map((p) => `${p.pluginId}:${p.id}` === key ? { ...p, collapsed: !p.collapsed } : p),
    );
  }

  function clearPlugin(pluginId: string): void {
    setPanels((prev) => prev.filter((p) => p.pluginId !== pluginId));
  }

  function clear(): void {
    setPanels([]);
  }

  return { registerPanel, getPanels, toggleCollapsed, clearPlugin, clear };
}

export const sidebarPluginStore = createSidebarPluginStore();
