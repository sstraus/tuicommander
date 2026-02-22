import {
  DEFAULT_BINDINGS,
  normalizeCombo,
  ACTION_NAMES,
  type ActionName,
} from "../keybindingDefaults";
import { invoke } from "../invoke";

interface KeybindingOverride {
  action: string;
  key: string | null;
}

/**
 * Keybindings store — merges user overrides with defaults to produce a lookup table.
 *
 * Exported as a factory for testability. The singleton is exported at the bottom.
 */
export function createKeybindingsStore() {
  // Action → display key (e.g. "Cmd+Shift+D"). Undefined means unbound.
  let actionToKey = new Map<string, string>();
  // Normalized combo → action name (for event dispatch)
  let comboToAction = new Map<string, string>();

  /** Rebuild both lookup maps from a merged action→key record */
  function rebuildMaps(bindings: Record<string, string | undefined>) {
    actionToKey = new Map();
    comboToAction = new Map();

    for (const [action, key] of Object.entries(bindings)) {
      if (!key) continue; // unbound
      actionToKey.set(action, key);
      const normalized = normalizeCombo(key);
      if (normalized) {
        comboToAction.set(normalized, action);
      }
    }
  }

  // Initialize with defaults
  rebuildMaps({ ...DEFAULT_BINDINGS });

  return {
    /** Load user overrides from keybindings.json, merge with defaults */
    async hydrate(): Promise<void> {
      try {
        const overrides = await invoke<KeybindingOverride[]>("load_keybindings");
        if (!Array.isArray(overrides) || overrides.length === 0) return;

        const validActions = new Set<string>(ACTION_NAMES);
        const merged: Record<string, string | undefined> = { ...DEFAULT_BINDINGS };

        for (const entry of overrides) {
          if (!entry.action || !validActions.has(entry.action)) continue;
          if (entry.key === null || entry.key === "") {
            // Unbind this action
            merged[entry.action] = undefined;
          } else {
            merged[entry.action] = entry.key;
          }
        }

        rebuildMaps(merged);
      } catch (err) {
        console.debug("Failed to load keybindings overrides:", err);
        // Keep defaults
      }
    },

    /** Get the action name for a normalized key combo, or undefined */
    getActionForCombo(normalizedCombo: string): ActionName | undefined {
      return comboToAction.get(normalizedCombo) as ActionName | undefined;
    },

    /** Get the display key string for an action, or undefined if unbound */
    getKeyForAction(action: ActionName): string | undefined {
      return actionToKey.get(action);
    },

    /** Get all current bindings as a record (for HelpPanel display) */
    getAllBindings(): Partial<Record<ActionName, string>> {
      const result: Partial<Record<ActionName, string>> = {};
      for (const [action, key] of actionToKey.entries()) {
        result[action as ActionName] = key;
      }
      return result;
    },
  };
}

/** Singleton keybindings store */
export const keybindingsStore = createKeybindingsStore();
