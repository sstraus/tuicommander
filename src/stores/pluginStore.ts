/**
 * Reactive store for plugin states.
 *
 * Tracks all registered plugins (built-in and external) with their
 * manifest, enabled/loaded status, errors, and per-plugin logger.
 * UI components (e.g. PluginsTab) subscribe to this store for rendering.
 */

import { createStore, produce } from "solid-js/store";
import { PluginLogger } from "../plugins/pluginLogger";
import { invoke } from "../invoke";
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

  // -------------------------------------------------------------------------
  // IPC wrappers — UI components call these instead of invoke() directly
  // -------------------------------------------------------------------------

  /** Toggle a plugin enabled/disabled. Delegates to pluginLoader.setPluginEnabled. */
  async function setEnabled(id: string, enabled: boolean): Promise<void> {
    // Lazy import to avoid circular dependency (pluginLoader imports pluginStore)
    const { setPluginEnabled } = await import("../plugins/pluginLoader");
    await setPluginEnabled(id, enabled);
  }

  /** Uninstall a plugin: remove files via Rust, then remove from store. */
  async function uninstall(id: string): Promise<void> {
    await invoke("uninstall_plugin", { id });
    removePlugin(id);
  }

  /** Install a plugin from a remote URL. Returns the installed manifest. */
  async function installFromUrl(url: string): Promise<PluginManifest> {
    return invoke<PluginManifest>("install_plugin_from_url", { url });
  }

  /** Install a plugin from a local ZIP file. Returns the installed manifest. */
  async function installFromZip(path: string): Promise<PluginManifest> {
    return invoke<PluginManifest>("install_plugin_from_zip", { path });
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
    setEnabled,
    uninstall,
    installFromUrl,
    installFromZip,
  };
}

export const pluginStore = createPluginStore();
