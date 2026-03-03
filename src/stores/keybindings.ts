import { createSignal } from "solid-js";
import {
  DEFAULT_BINDINGS,
  normalizeCombo,
  ACTION_NAMES,
  type ActionName,
} from "../keybindingDefaults";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

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
  // User overrides (only non-default bindings)
  let userOverrides = new Map<string, string | null>();
  // Reactivity: bump version when bindings change so SolidJS re-renders
  const [version, setVersion] = createSignal(0);

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

  /** Get the current merged bindings record */
  function getMergedBindings(): Record<string, string | undefined> {
    const merged: Record<string, string | undefined> = { ...DEFAULT_BINDINGS };
    for (const [action, key] of userOverrides.entries()) {
      if (key === null || key === "") {
        merged[action] = undefined;
      } else {
        merged[action] = key;
      }
    }
    return merged;
  }

  /** Compute overrides array for persistence (only non-default bindings) */
  function toOverridesArray(): KeybindingOverride[] {
    const overrides: KeybindingOverride[] = [];
    for (const [action, key] of userOverrides.entries()) {
      overrides.push({ action, key });
    }
    return overrides;
  }

  // Initialize with defaults
  rebuildMaps({ ...DEFAULT_BINDINGS });

  return {
    /** Reactivity accessor — read this in createEffect/createMemo to track changes */
    get version() { return version(); },

    /** Load user overrides from keybindings.json, merge with defaults */
    async hydrate(): Promise<void> {
      try {
        const overrides = await invoke<KeybindingOverride[]>("load_keybindings");
        if (!Array.isArray(overrides) || overrides.length === 0) return;

        const validActions = new Set<string>(ACTION_NAMES);
        userOverrides = new Map();

        for (const entry of overrides) {
          if (!entry.action || !validActions.has(entry.action)) continue;
          userOverrides.set(entry.action, entry.key);
        }

        rebuildMaps(getMergedBindings());
        setVersion((v) => v + 1);
      } catch (err) {
        appLogger.debug("config", "Failed to load keybindings overrides", err);
        // Keep defaults
      }
    },

    /** Set a user override for an action and persist */
    async setOverride(action: ActionName, key: string): Promise<void> {
      // If same as default, remove the override instead
      if (DEFAULT_BINDINGS[action] === key) {
        userOverrides.delete(action);
      } else {
        userOverrides.set(action, key);
      }
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Reset a single action to its default binding and persist */
    async resetAction(action: ActionName): Promise<void> {
      userOverrides.delete(action);
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Reset all overrides to defaults and persist */
    async resetAll(): Promise<void> {
      userOverrides.clear();
      rebuildMaps({ ...DEFAULT_BINDINGS });
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Persist current overrides to keybindings.json */
    async save(): Promise<void> {
      try {
        await invoke("save_keybindings", { config: toOverridesArray() });
      } catch (err) {
        appLogger.error("config", "Failed to save keybindings", err);
      }
    },

    /** Check if an action has a user override (differs from default) */
    isOverridden(action: ActionName): boolean {
      // Read version for reactivity
      version();
      return userOverrides.has(action);
    },

    /** Get the action name for a normalized key combo, or undefined */
    getActionForCombo(normalizedCombo: string): ActionName | undefined {
      // Read version for reactivity
      version();
      return comboToAction.get(normalizedCombo) as ActionName | undefined;
    },

    /** Get the display key string for an action, or undefined if unbound */
    getKeyForAction(action: ActionName): string | undefined {
      // Read version for reactivity
      version();
      return actionToKey.get(action);
    },

    /** Get all current bindings as a record (for HelpPanel display) */
    getAllBindings(): Partial<Record<ActionName, string>> {
      // Read version for reactivity
      version();
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
