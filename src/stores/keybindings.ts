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
 * Dynamic action registered by a plugin. Coexists with static ACTION_NAMES.
 * Plugins register these via host.registerCommand(); they participate in
 * conflict detection, persistence, and the KeyboardShortcutsTab UI just like
 * built-in actions, but are scoped under `plugin:<pluginId>:<id>` names.
 */
export interface DynamicAction {
  action: string;
  label: string;
  pluginId: string;
  defaultKey: string | undefined;
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
  // Dynamic actions keyed by action name (`plugin:<id>:<cmd>`)
  const dynamicActions = new Map<string, DynamicAction>();
  // Reactivity: bump version when bindings change so SolidJS re-renders
  const [version, setVersion] = createSignal(0);

  /** Merge static defaults with dynamic plugin defaults */
  function allDefaults(): Record<string, string | undefined> {
    const merged: Record<string, string | undefined> = { ...DEFAULT_BINDINGS };
    for (const [action, dyn] of dynamicActions.entries()) {
      merged[action] = dyn.defaultKey;
    }
    return merged;
  }

  /** Rebuild both lookup maps from a merged action→key record */
  function rebuildMaps(bindings: Record<string, string | undefined>) {
    actionToKey = new Map();
    comboToAction = new Map();

    for (const [action, key] of Object.entries(bindings)) {
      if (!key) continue; // unbound
      // Conflict: another action already owns this combo. First writer wins;
      // subsequent bindings are skipped for the combo map but kept in actionToKey
      // so the UI can display them (with a conflict warning).
      actionToKey.set(action, key);
      const normalized = normalizeCombo(key);
      if (!normalized) continue;
      if (!comboToAction.has(normalized)) {
        comboToAction.set(normalized, action);
      } else {
        const winner = comboToAction.get(normalized);
        appLogger.warn(
          "config",
          `Keybinding conflict on "${key}": "${action}" skipped (already bound to "${winner}")`,
        );
      }
    }
  }

  /** Get the current merged bindings record */
  function getMergedBindings(): Record<string, string | undefined> {
    const merged = allDefaults();
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

  /** True if `action` is a known static or dynamic action */
  function isKnownAction(action: string): boolean {
    return (
      (ACTION_NAMES as readonly string[]).includes(action) ||
      dynamicActions.has(action)
    );
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

        userOverrides = new Map();
        for (const entry of overrides) {
          if (!entry.action) continue;
          // Accept both static and plugin: namespaced actions. Dynamic plugin
          // overrides loaded here will bind once the plugin registers the action
          // via registerDynamicAction (see rebuild below).
          if (
            !(ACTION_NAMES as readonly string[]).includes(entry.action) &&
            !entry.action.startsWith("plugin:")
          ) {
            continue;
          }
          userOverrides.set(entry.action, entry.key);
        }

        rebuildMaps(getMergedBindings());
        setVersion((v) => v + 1);
      } catch (err) {
        appLogger.debug("config", "Failed to load keybindings overrides", err);
        // Keep defaults
      }
    },

    /**
     * Register a dynamic action for a plugin command.
     * `action` should be namespaced as `plugin:<pluginId>:<commandId>`.
     * If the default combo collides with an existing binding, logs a warning
     * and leaves the action unbound — user can remap it via Settings.
     */
    registerDynamicAction(dyn: DynamicAction): void {
      dynamicActions.set(dyn.action, dyn);
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
    },

    /** Remove a dynamic action (called on plugin unload) */
    unregisterDynamicAction(action: string): void {
      if (!dynamicActions.delete(action)) return;
      // Leave any user override in place so re-registering later restores it,
      // but drop from the active maps.
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
    },

    /** Get all currently registered dynamic actions (sorted by pluginId, label) */
    getDynamicActions(): DynamicAction[] {
      // Read version for reactivity
      version();
      return Array.from(dynamicActions.values()).sort((a, b) => {
        if (a.pluginId !== b.pluginId) return a.pluginId.localeCompare(b.pluginId);
        return a.label.localeCompare(b.label);
      });
    },

    /** Set a user override for an action and persist */
    async setOverride(action: string, key: string): Promise<void> {
      const defaults = allDefaults();
      // If same as default, remove the override instead
      if (defaults[action] === key) {
        userOverrides.delete(action);
      } else {
        userOverrides.set(action, key);
      }
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
      await this.save();
    },

    /**
     * Explicitly unbind an action: persist a null override so hydrate restores
     * it as unbound. Used by the conflict-replace flow so the displaced action
     * is removed from both lookups instead of relying on rebuildMaps' implicit
     * first-writer-wins resolution (story 1279-ff10).
     */
    async unbind(action: string): Promise<void> {
      userOverrides.set(action, null);
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Reset a single action to its default binding and persist */
    async resetAction(action: string): Promise<void> {
      userOverrides.delete(action);
      rebuildMaps(getMergedBindings());
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Reset all overrides to defaults and persist */
    async resetAll(): Promise<void> {
      userOverrides.clear();
      rebuildMaps(allDefaults());
      setVersion((v) => v + 1);
      await this.save();
    },

    /** Remove all overrides whose action name matches a plugin prefix */
    async removeOverridesForPlugin(pluginId: string): Promise<void> {
      const prefix = `plugin:${pluginId}:`;
      let changed = false;
      for (const action of Array.from(userOverrides.keys())) {
        if (action.startsWith(prefix)) {
          userOverrides.delete(action);
          changed = true;
        }
      }
      if (!changed) return;
      rebuildMaps(getMergedBindings());
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
    isOverridden(action: string): boolean {
      // Read version for reactivity
      version();
      return userOverrides.has(action);
    },

    /** Get the action name for a normalized key combo, or undefined */
    getActionForCombo(normalizedCombo: string): string | undefined {
      // Read version for reactivity
      version();
      return comboToAction.get(normalizedCombo);
    },

    /** Get the display key string for an action, or undefined if unbound */
    getKeyForAction(action: string): string | undefined {
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
        if ((ACTION_NAMES as readonly string[]).includes(action)) {
          result[action as ActionName] = key;
        }
      }
      return result;
    },

    /** True if the given action is known to the store (static or dynamic) */
    isKnownAction,
  };
}

/** Singleton keybindings store */
export const keybindingsStore = createKeybindingsStore();

// Debug registry — expose effective keybindings for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";
registerDebugSnapshot("keybindings", () => {
  return keybindingsStore.getAllBindings();
});
