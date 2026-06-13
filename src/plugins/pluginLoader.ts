/**
 * User plugin discovery, validation, and dynamic loading.
 *
 * Discovers plugins via `list_user_plugins` Tauri command, validates manifests
 * and module shapes, and loads them with `import("plugin://id/main.js")`.
 * Also listens for `plugin-changed` events for hot reload.
 */

import { invoke, listen } from "../invoke";
import { isWindows } from "../platform";
import { appLogger } from "../stores/appLogger";
import { pluginStore } from "../stores/pluginStore";
import { terminalsStore } from "../stores/terminals";
import { isTauri } from "../transport";
import { pluginRegistry } from "./pluginRegistry";
import type { TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// App version (from tauri.conf.json at build time via Vite define)
// ---------------------------------------------------------------------------

/** Current app version for minAppVersion checks */
const APP_VERSION = __APP_VERSION__;

// ---------------------------------------------------------------------------
// Built-in plugin registry (populated by initPlugins to avoid circular imports)
// ---------------------------------------------------------------------------

const builtInPluginMap = new Map<string, TuiPlugin>();

/** Register a built-in plugin so it can be toggled on/off */
export function registerBuiltInPlugin(plugin: TuiPlugin): void {
	builtInPluginMap.set(plugin.id, plugin);
}

// ---------------------------------------------------------------------------
// Manifest type (matches Rust PluginManifest serialization)
// ---------------------------------------------------------------------------

export interface PluginManifest {
	id: string;
	name: string;
	version: string;
	minAppVersion: string;
	main: string;
	description?: string;
	author?: string;
	capabilities: string[];
	allowedUrls?: string[];
	/** Agent types this plugin targets (e.g. ["claude"]). Empty/omitted = universal. */
	agentTypes?: string[];
	/** CLI binaries this plugin may execute via exec:cli (e.g. ["rtk", "mdkb"]). */
	binaries?: string[];
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/** Simple semver comparison: returns -1, 0, or 1 */
export function compareSemver(a: string, b: string): number {
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

	if (!manifest.minAppVersion) return "manifest: minAppVersion is required";
	if (compareSemver(manifest.minAppVersion, APP_VERSION) > 0) {
		return `plugin "${manifest.id}" requires app version ${manifest.minAppVersion}, current is ${APP_VERSION}`;
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
// Disabled plugin tracking
// ---------------------------------------------------------------------------

/** Cached set of disabled plugin IDs (synced from AppConfig) */
let disabledPluginIds = new Set<string>();

/** Fetch the disabled plugin list from Rust config */
export async function syncDisabledList(): Promise<void> {
	try {
		const config = await invoke<{ disabled_plugin_ids?: string[] }>("load_config");
		disabledPluginIds = new Set(config.disabled_plugin_ids ?? []);
	} catch (e) {
		appLogger.warn("plugin", "Failed to load plugin config — treating all as enabled", { error: String(e) });
		disabledPluginIds = new Set();
	}
}

/** Check if a plugin ID is disabled in config */
export function isPluginDisabled(id: string): boolean {
	return disabledPluginIds.has(id);
}

/**
 * Enable or disable a plugin. Updates config and loads/unloads immediately.
 */
export async function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
	// Update Rust config
	const config = await invoke<Record<string, unknown>>("load_config");
	const list = new Set<string>((config.disabled_plugin_ids as string[]) ?? []);

	if (enabled) {
		list.delete(id);
	} else {
		list.add(id);
	}

	await invoke("save_config", {
		config: { ...config, disabled_plugin_ids: [...list] },
	});

	disabledPluginIds = list;
	pluginStore.updatePlugin(id, { enabled });

	// Check if this is a built-in plugin
	const builtIn = builtInPluginMap.get(id);

	if (enabled) {
		if (builtIn) {
			// Re-register built-in plugin
			pluginRegistry.register(builtIn);
			pluginStore.updatePlugin(id, { loaded: true, error: null });
		} else {
			// Load external plugin from disk
			const manifests = await invoke<PluginManifest[]>("list_user_plugins");
			const manifest = manifests.find((m) => m.id === id);
			if (manifest) {
				const error = validateManifest(manifest);
				if (error) {
					pluginStore.getLogger(id).error(error);
					pluginStore.updatePlugin(id, { error, loaded: false });
				} else {
					await loadPlugin(manifest);
				}
			}
		}
	} else {
		// Unload the plugin
		if (builtIn) {
			pluginRegistry.unregister(id);
			pluginStore.updatePlugin(id, { loaded: false });
		} else if (loadedPluginIds.has(id)) {
			pluginRegistry.unregister(id);
			loadedPluginIds.delete(id);
			invoke("unregister_loaded_plugin", { pluginId: id }).catch(() => {});
		}
	}
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/** Track loaded plugin IDs for hot-reload unregister/re-register */
const loadedPluginIds = new Set<string>();

/**
 * Build the base import URL for a plugin module.
 *
 * Tauri/wry exposes custom URI schemes differently per platform: macOS/Linux
 * serve them under the raw `plugin://` scheme, but WebView2 on Windows only
 * serves them as `http://plugin.localhost/...`. `register_plugin_protocol()`
 * in plugins.rs already parses both forms — this just emits the right one.
 */
export function pluginModuleBaseUrl(id: string, main: string, windows: boolean): string {
	const relPath = `${id}/${main}`;
	return windows ? `http://plugin.localhost/${relPath}` : `plugin://${relPath}`;
}

async function loadPlugin(manifest: PluginManifest): Promise<void> {
	const logger = pluginStore.getLogger(manifest.id);

	// Register in pluginStore so UI can see it even before load completes
	pluginStore.registerPlugin(manifest.id, {
		manifest,
		builtIn: false,
		enabled: true,
		loaded: false,
	});

	// `?t=` cache-busts for hot reload.
	const url = `${pluginModuleBaseUrl(manifest.id, manifest.main, isWindows())}?t=${Date.now()}`;
	let mod: unknown;
	try {
		mod = await import(/* @vite-ignore */ url);
	} catch (err) {
		const msg = err instanceof Error ? err.message : String(err);
		appLogger.error("plugin", `Failed to import plugin "${manifest.id}"`, err);
		logger.error(`Import failed: ${msg}`, err);
		pluginStore.updatePlugin(manifest.id, { error: msg });
		return;
	}

	const moduleError = validateModule(mod, manifest.id);
	if (moduleError) {
		appLogger.error("plugin", `Invalid module for plugin "${manifest.id}": ${moduleError}`);
		logger.error(`Module validation failed: ${moduleError}`);
		pluginStore.updatePlugin(manifest.id, { error: moduleError });
		return;
	}

	const plugin = (mod as { default: TuiPlugin }).default;
	pluginRegistry.register(plugin, manifest.capabilities, manifest.allowedUrls, manifest.agentTypes);
	loadedPluginIds.add(manifest.id);

	// Rust-side registration is already handled by pluginRegistry.register()

	logger.info(`Loaded v${manifest.version}`);
	appLogger.info("plugin", `Loaded plugin "${manifest.id}" v${manifest.version}`);
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

	let anyReloaded = false;

	for (const pluginId of changedIds) {
		// Skip disabled plugins early — no IPC, no store churn
		if (disabledPluginIds.has(pluginId)) {
			appLogger.debug("plugin", `Plugin "${pluginId}" changed but disabled, skipping`);
			continue;
		}

		appLogger.info("plugin", `Plugin "${pluginId}" changed, reloading...`);

		// Unregister if previously loaded
		if (loadedPluginIds.has(pluginId)) {
			pluginRegistry.unregister(pluginId);
			loadedPluginIds.delete(pluginId);
			invoke("unregister_loaded_plugin", { pluginId }).catch(() => {});
		}

		// Re-discover this specific plugin's manifest
		let manifests: PluginManifest[];
		try {
			manifests = await invoke<PluginManifest[]>("list_user_plugins");
		} catch (err) {
			appLogger.error("plugin", "Failed to list plugins for reload", err);
			return;
		}

		const manifest = manifests.find((m) => m.id === pluginId);
		if (!manifest) {
			appLogger.warn("plugin", `Plugin "${pluginId}" not found after change event`);
			pluginStore.removePlugin(pluginId);
			continue;
		}

		const manifestError = validateManifest(manifest);
		if (manifestError) {
			appLogger.error("plugin", manifestError);
			pluginStore.getLogger(pluginId).error(manifestError);
			pluginStore.updatePlugin(pluginId, { error: manifestError, loaded: false });
			continue;
		}

		await loadPlugin(manifest);
		anyReloaded = true;
	}

	if (anyReloaded) {
		replayActiveAgents(terminalsStore);
	}
}

/** Replay agent-started events for terminals that already have a running agent. */
function replayActiveAgents(store: typeof terminalsStore): void {
	for (const id of store.getIds()) {
		const t = store.get(id);
		if (t?.agentType && t.sessionId) {
			pluginRegistry.notifyStateChange({
				type: "agent-started",
				sessionId: t.sessionId,
				terminalId: id,
			});
			if (t.shellState) {
				pluginRegistry.dispatchStructuredEvent("shell-state", { state: t.shellState }, t.sessionId);
			}
		}
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
	if (!isTauri()) {
		appLogger.debug("plugin", "User plugin loading skipped in browser mode");
		return;
	}

	// Sync disabled list from config
	await syncDisabledList();

	// Set up hot reload listener
	try {
		await listen("plugin-changed", handlePluginChanged);
	} catch (err) {
		appLogger.warn("plugin", "Failed to listen for plugin-changed events", err);
	}

	// Discover plugins
	let manifests: PluginManifest[];
	try {
		manifests = await invoke<PluginManifest[]>("list_user_plugins");
	} catch (err) {
		appLogger.error("plugin", "Failed to discover user plugins", err);
		return;
	}

	// Load each valid plugin (register disabled ones in store but don't load)
	for (const manifest of manifests) {
		const error = validateManifest(manifest);
		if (error) {
			appLogger.error("plugin", `Skipping plugin: ${error}`);
			continue;
		}

		if (disabledPluginIds.has(manifest.id)) {
			pluginStore.registerPlugin(manifest.id, {
				manifest,
				builtIn: false,
				enabled: false,
				loaded: false,
			});
			appLogger.info("plugin", `Plugin "${manifest.id}" is disabled, skipping`);
			continue;
		}

		await loadPlugin(manifest);
	}
}
