/**
 * User plugin discovery, validation, and dynamic loading.
 *
 * Discovers plugins via `list_user_plugins` Tauri command, validates manifests
 * and module shapes, and loads them with `import("plugin://id/main.js")`.
 * Also listens for `plugin-changed` events for hot reload.
 */

import { invoke, listen } from "../invoke";
import { pluginRegistry } from "./pluginRegistry";
import { pluginStore } from "../stores/pluginStore";
import type { TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// App version (from tauri.conf.json at build time via Vite define)
// ---------------------------------------------------------------------------

/** Current app version for minAppVersion checks */
const APP_VERSION = __APP_VERSION__;

// ---------------------------------------------------------------------------
// Manifest type (matches Rust PluginManifest serialization)
// ---------------------------------------------------------------------------

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  min_app_version: string;
  main: string;
  description?: string;
  author?: string;
  capabilities: string[];
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/** Simple semver comparison: returns -1, 0, or 1 */
function compareSemver(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const va = pa[i] ?? 0;
    const vb = pb[i] ?? 0;
    if (va < vb) return -1;
    if (va > vb) return 1;
  }
  return 0;
}

/**
 * Validate a plugin manifest. Returns null if valid, error string if invalid.
 */
export function validateManifest(manifest: PluginManifest): string | null {
  if (!manifest.id) return "manifest: id is required";
  if (!manifest.name) return "manifest: name is required";
  if (!manifest.version) return "manifest: version is required";
  if (!manifest.main) return "manifest: main is required";

  if (compareSemver(manifest.min_app_version, APP_VERSION) > 0) {
    return `plugin "${manifest.id}" requires app version ${manifest.min_app_version}, current is ${APP_VERSION}`;
  }

  return null;
}

/**
 * Validate a dynamically imported module. Returns null if valid, error string if invalid.
 */
export function validateModule(mod: unknown, expectedId: string): string | null {
  if (!mod || typeof mod !== "object") return "module has no default export";
  const defaultExport = (mod as Record<string, unknown>).default;
  if (!defaultExport || typeof defaultExport !== "object") {
    return "module has no default export or default is not an object";
  }

  const plugin = defaultExport as Record<string, unknown>;
  if (typeof plugin.onload !== "function") {
    return "module default export is missing onload function";
  }
  if (typeof plugin.onunload !== "function") {
    return "module default export is missing onunload function";
  }
  if (plugin.id !== expectedId) {
    return `module id mismatch: expected "${expectedId}", got "${String(plugin.id)}"`;
  }

  return null;
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/** Track loaded plugin IDs for hot-reload unregister/re-register */
const loadedPluginIds = new Set<string>();

async function loadPlugin(manifest: PluginManifest): Promise<void> {
  const logger = pluginStore.getLogger(manifest.id);

  // Register in pluginStore so UI can see it even before load completes
  pluginStore.registerPlugin(manifest.id, {
    manifest,
    builtIn: false,
    enabled: true,
    loaded: false,
  });

  // Cache-bust for hot reload
  const url = `plugin://${manifest.id}/${manifest.main}?t=${Date.now()}`;
  let mod: unknown;
  try {
    mod = await import(/* @vite-ignore */ url);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`[pluginLoader] failed to import "${manifest.id}":`, err);
    logger.error(`Import failed: ${msg}`, err);
    pluginStore.updatePlugin(manifest.id, { error: msg });
    return;
  }

  const moduleError = validateModule(mod, manifest.id);
  if (moduleError) {
    console.error(`[pluginLoader] invalid module for "${manifest.id}": ${moduleError}`);
    logger.error(`Module validation failed: ${moduleError}`);
    pluginStore.updatePlugin(manifest.id, { error: moduleError });
    return;
  }

  const plugin = (mod as { default: TuiPlugin }).default;
  pluginRegistry.register(plugin, manifest.capabilities);
  loadedPluginIds.add(manifest.id);
  logger.info(`Loaded v${manifest.version}`);
  console.log(`[pluginLoader] loaded plugin "${manifest.id}" v${manifest.version}`);
}

// ---------------------------------------------------------------------------
// Hot reload
// ---------------------------------------------------------------------------

/**
 * Handle the plugin-changed event from Rust's file watcher.
 * Rust emits payload as string[] (array of changed plugin IDs).
 */
async function handlePluginChanged(event: { payload: string[] }): Promise<void> {
  const changedIds = event.payload;
  if (!Array.isArray(changedIds) || changedIds.length === 0) return;

  for (const pluginId of changedIds) {
    console.log(`[pluginLoader] plugin-changed event for "${pluginId}", reloading...`);

    // Unregister if previously loaded
    if (loadedPluginIds.has(pluginId)) {
      pluginRegistry.unregister(pluginId);
      loadedPluginIds.delete(pluginId);
    }

    // Re-discover this specific plugin's manifest
    let manifests: PluginManifest[];
    try {
      manifests = await invoke<PluginManifest[]>("list_user_plugins");
    } catch (err) {
      console.error("[pluginLoader] failed to list plugins for reload:", err);
      return;
    }

    const manifest = manifests.find((m) => m.id === pluginId);
    if (!manifest) {
      console.warn(`[pluginLoader] plugin "${pluginId}" not found after change event`);
      pluginStore.removePlugin(pluginId);
      continue;
    }

    const manifestError = validateManifest(manifest);
    if (manifestError) {
      console.error(`[pluginLoader] ${manifestError}`);
      pluginStore.getLogger(pluginId).error(manifestError);
      pluginStore.updatePlugin(pluginId, { error: manifestError, loaded: false });
      continue;
    }

    await loadPlugin(manifest);
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Discover and load all user plugins from the plugins directory.
 * Call once at app startup after built-in plugins are registered.
 */
export async function loadUserPlugins(): Promise<void> {
  // Set up hot reload listener
  try {
    await listen("plugin-changed", handlePluginChanged);
  } catch (err) {
    console.warn("[pluginLoader] failed to listen for plugin-changed events:", err);
  }

  // Discover plugins
  let manifests: PluginManifest[];
  try {
    manifests = await invoke<PluginManifest[]>("list_user_plugins");
  } catch (err) {
    console.error("[pluginLoader] failed to discover user plugins:", err);
    return;
  }

  // Load each valid plugin
  for (const manifest of manifests) {
    const error = validateManifest(manifest);
    if (error) {
      console.error(`[pluginLoader] skipping plugin: ${error}`);
      continue;
    }
    await loadPlugin(manifest);
  }
}
