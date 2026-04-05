import { createSignal } from "solid-js";
import type { Disposable } from "./types";

/**
 * Dashboard entry registered by a plugin via host.registerDashboard().
 * Powers the "Dashboard" button in Settings → Plugins for plugins that have
 * a primary view (markdown tab or HTML panel) worth opening with one click.
 */
export interface DashboardEntry {
  pluginId: string;
  label: string;
  icon?: string;
  open: () => void | Promise<void>;
}

function createDashboardRegistry() {
  const entries = new Map<string, DashboardEntry>();
  const [version, setVersion] = createSignal(0);

  return {
    /** Reactivity accessor — read in createMemo/Show to track changes */
    get version() {
      return version();
    },

    /**
     * Register a dashboard for a plugin. A second call from the same plugin
     * replaces the previous entry (last-write-wins). Returns a Disposable that
     * removes the entry on dispose.
     */
    register(entry: DashboardEntry): Disposable {
      entries.set(entry.pluginId, entry);
      setVersion((v) => v + 1);
      return {
        dispose: () => {
          const current = entries.get(entry.pluginId);
          if (current === entry) {
            entries.delete(entry.pluginId);
            setVersion((v) => v + 1);
          }
        },
      };
    },

    /** Look up a plugin's dashboard entry, or undefined if none registered */
    get(pluginId: string): DashboardEntry | undefined {
      // Read version for reactivity
      version();
      return entries.get(pluginId);
    },
  };
}

export const dashboardRegistry = createDashboardRegistry();
