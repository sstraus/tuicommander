import { createSignal } from "solid-js";
import type { Disposable, TerminalAction } from "../plugins/types";

interface ActionEntry {
  pluginId: string;
  action: TerminalAction;
}

function createContextMenuActionsStore() {
  const [entries, setEntries] = createSignal<ActionEntry[]>([]);

  function registerAction(pluginId: string, action: TerminalAction): Disposable {
    // Replace existing action with same id from same plugin
    setEntries((prev) => [
      ...prev.filter((e) => !(e.pluginId === pluginId && e.action.id === action.id)),
      { pluginId, action },
    ]);

    return {
      dispose() {
        setEntries((prev) =>
          prev.filter((e) => !(e.pluginId === pluginId && e.action.id === action.id)),
        );
      },
    };
  }

  function getActions(): TerminalAction[] {
    return entries().map((e) => e.action);
  }

  function clearPlugin(pluginId: string): void {
    setEntries((prev) => prev.filter((e) => e.pluginId !== pluginId));
  }

  function clear(): void {
    setEntries([]);
  }

  return { registerAction, getActions, clearPlugin, clear };
}

export const contextMenuActionsStore = createContextMenuActionsStore();
