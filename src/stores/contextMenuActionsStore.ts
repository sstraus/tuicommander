import { createSignal } from "solid-js";
import type { Disposable, TerminalAction } from "../plugins/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ContextMenuTarget = "terminal" | "branch" | "repo" | "tab";

export interface ContextMenuAction {
  id: string;
  label: string;
  icon?: string;
  target: ContextMenuTarget;
  action: (ctx: ContextMenuContext) => void;
  disabled?: (ctx: ContextMenuContext) => boolean;
}

export interface ContextMenuContext {
  target: ContextMenuTarget;
  sessionId?: string;
  repoPath?: string;
  branchName?: string;
  tabId?: string;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface ActionEntry {
  pluginId: string;
  action: TerminalAction;
}

interface ContextMenuEntry {
  pluginId: string;
  action: ContextMenuAction;
}

function createContextMenuActionsStore() {
  // Legacy: terminal-only actions (backward compat)
  const [entries, setEntries] = createSignal<ActionEntry[]>([]);
  // New: multi-target actions
  const [contextEntries, setContextEntries] = createSignal<ContextMenuEntry[]>([]);

  // --- Legacy terminal actions (backward compat) ---

  function registerAction(pluginId: string, action: TerminalAction): Disposable {
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

  // --- New multi-target context menu actions ---

  function registerContextAction(pluginId: string, action: ContextMenuAction): Disposable {
    const key = `${pluginId}:${action.id}`;
    setContextEntries((prev) => [
      ...prev.filter((e) => `${e.pluginId}:${e.action.id}` !== key),
      { pluginId, action },
    ]);
    return {
      dispose() {
        setContextEntries((prev) =>
          prev.filter((e) => `${e.pluginId}:${e.action.id}` !== key),
        );
      },
    };
  }

  function getContextActions(target: ContextMenuTarget): ContextMenuAction[] {
    return contextEntries()
      .filter((e) => e.action.target === target)
      .map((e) => e.action);
  }

  // --- Shared ---

  function clearPlugin(pluginId: string): void {
    setEntries((prev) => prev.filter((e) => e.pluginId !== pluginId));
    setContextEntries((prev) => prev.filter((e) => e.pluginId !== pluginId));
  }

  function clear(): void {
    setEntries([]);
    setContextEntries([]);
  }

  return { registerAction, getActions, registerContextAction, getContextActions, clearPlugin, clear };
}

export const contextMenuActionsStore = createContextMenuActionsStore();
