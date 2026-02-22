/**
 * Reactive store for plugin states.
 *
 * Tracks all registered plugins (built-in and external) with their
 * manifest, enabled/loaded status, errors, and per-plugin logger.
 * UI components (e.g. PluginsTab) subscribe to this store for rendering.
 */

import { createStore, produce } from "solid-js/store";
import { PluginLogger } from "../plugins/pluginLogger";
import type { PluginManifest } from "../plugins/pluginLoader";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PluginState {
  id: string;
  manifest: PluginManifest | null;
  /** Whether the plugin is built-in (always enabled, no toggle) */
  builtIn: boolean;
  /** Whether the plugin is enabled in config (only meaningful for non-builtIn) */
  enabled: boolean;
  /** Whether the plugin is currently loaded (onload succeeded) */
  loaded: boolean;
  /** Last error message if loading failed */
  error: string | null;
  /** Per-plugin log instance */
  logger: PluginLogger;
}

interface PluginStoreState {
  plugins: PluginState[];
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

function createPluginStore() {
  const [state, setState] = createStore<PluginStoreState>({
    plugins: [],
  });

  // Map of plugin ID → PluginLogger (loggers are mutable objects, not reactive)
  const loggers = new Map<string, PluginLogger>();

  function getOrCreateLogger(id: string): PluginLogger {
    let logger = loggers.get(id);
    if (!logger) {
      logger = new PluginLogger();
      loggers.set(id, logger);
    }
    return logger;
  }

  /** Register a plugin in the store. Upserts by id. */
  function registerPlugin(
    id: string,
    opts: {
      manifest?: PluginManifest | null;
      builtIn?: boolean;
      enabled?: boolean;
      loaded?: boolean;
      error?: string | null;
    } = {},
  ): void {
    const logger = getOrCreateLogger(id);

    setState(
      produce((s) => {
        const idx = s.plugins.findIndex((p) => p.id === id);
        const entry: PluginState = {
          id,
          manifest: opts.manifest ?? null,
          builtIn: opts.builtIn ?? false,
          enabled: opts.enabled ?? true,
          loaded: opts.loaded ?? false,
          error: opts.error ?? null,
          logger,
        };
        if (idx >= 0) {
          s.plugins[idx] = entry;
        } else {
          s.plugins.push(entry);
        }
      }),
    );
  }

  /** Update fields on an existing plugin entry. */
  function updatePlugin(
    id: string,
    updates: Partial<Pick<PluginState, "loaded" | "enabled" | "error" | "manifest">>,
  ): void {
    setState(
      produce((s) => {
        const plugin = s.plugins.find((p) => p.id === id);
        if (!plugin) return;
        if (updates.loaded !== undefined) plugin.loaded = updates.loaded;
        if (updates.enabled !== undefined) plugin.enabled = updates.enabled;
        if (updates.error !== undefined) plugin.error = updates.error;
        if (updates.manifest !== undefined) plugin.manifest = updates.manifest;
      }),
    );
  }

  /** Remove a plugin from the store. */
  function removePlugin(id: string): void {
    setState(
      produce((s) => {
        const idx = s.plugins.findIndex((p) => p.id === id);
        if (idx >= 0) s.plugins.splice(idx, 1);
      }),
    );
    loggers.delete(id);
  }

  /** Get the logger for a plugin (creates one if needed). */
  function getLogger(id: string): PluginLogger {
    return getOrCreateLogger(id);
  }

  /** Get a plugin state by id. */
  function getPlugin(id: string): PluginState | undefined {
    return state.plugins.find((p) => p.id === id);
  }

  /** Get all plugins. */
  function getAll(): readonly PluginState[] {
    return state.plugins;
  }

  /** Clear all plugins (for testing). */
  function clear(): void {
    setState({ plugins: [] });
    loggers.clear();
  }

  return {
    state,
    registerPlugin,
    updatePlugin,
    removePlugin,
    getLogger,
    getPlugin,
    getAll,
    clear,
  };
}

export const pluginStore = createPluginStore();
