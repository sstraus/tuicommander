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
import { compareAppVersion } from "../utils/semver";
import { updateAppConfig } from "../utils/updateAppConfig";
import { pluginRegistry } from "./pluginRegistry";
import type { PluginSettingsDefinition } from "./pluginSettings";
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
	/** Complete private-setting schema; legacy first-party plugins are migrated by Rust. */
	settings?: PluginSettingsDefinition;
	id: string;
	name: string;
	version: string;
	/** Backend-derived payload identity; never read from manifest.json. */
	contentSha256?: string;
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

/**
 * Validate a plugin manifest. Returns null if valid, error string if invalid.
 */
export function validateManifest(manifest: PluginManifest): string | null {
	if (!manifest.id) return "manifest: id is required";
	if (!manifest.name) return "manifest: name is required";
	if (!manifest.version) return "manifest: version is required";
	if (!manifest.main) return "manifest: main is required";

	if (!manifest.minAppVersion) return "manifest: minAppVersion is required";
	if (compareAppVersion(manifest.minAppVersion, APP_VERSION) > 0) {
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
	let list = new Set<string>();
	await updateAppConfig<Record<string, unknown>>((config) => {
		list = new Set<string>((config.disabled_plugin_ids as string[]) ?? []);
		if (enabled) list.delete(id);
		else list.add(id);
		config.disabled_plugin_ids = [...list];
	});

	disabledPluginIds = list;
	pluginStore.updatePlugin(id, { enabled });

	// Check if this is a built-in plugin
	const builtIn = builtInPluginMap.get(id);

	if (enabled) {
		if (builtIn) {
			// Re-register built-in plugin
			await pluginRegistry.register(builtIn);
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
			await pluginRegistry.unregister(id);
			pluginStore.updatePlugin(id, { loaded: false });
		} else {
			// pluginRegistry.unregister() already issues the Rust-side unregister.
			await pluginRegistry.unregister(id);
			loadedPluginIds.delete(id);
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

/**
 * Entry URL of one load of a plugin. The version is a path segment (`~v<n>`) right
 * after the id, not a query: the WebView resolves a relative `import "./x.js"`
 * against the importing module's URL, drops the query, and keys its module map on
 * the result. A query would bust `main.js` only and leave every sub-module on the
 * copy from the first load. `register_plugin_protocol()` strips the segment.
 */
export function pluginModuleUrl(id: string, main: string, windows: boolean, version: number): string {
	return pluginModuleBaseUrl(id, `~v${version}/${main}`, windows);
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

	// A bare dynamic import: the plugin runs in the SAME JavaScript realm as the
	// host, with the host's module graph in reach. Capabilities gate what a plugin
	// DECLARES, not what it can IMPERSONATE — a plugin can import `invoke` itself
	// and pass another plugin's id to inherit that plugin's grants, because
	// `plugin_id` is caller-supplied and is the only key the Rust checks consult
	// (see the capability-enforcement note in `src-tauri/src/plugins.rs`). Treat
	// every installed plugin as trusted with the union of all installed
	// capabilities; they are isolated from the host's declared surface, not from
	// each other.
	//
	// DEFERRED (2026-07-28) — real isolation means loading each plugin off-realm
	// (Worker or sandboxed iframe) with a host-created MessagePort as its only
	// channel, so identity is the port it was handed rather than a string it
	// supplies. A per-plugin token was considered and rejected: same-realm JS can
	// read, proxy or monkey-patch it, so it would be a boundary in name only.
	//
	// A fresh `~v<n>` path version per load cache-busts the whole module graph for hot reload.
	const url = pluginModuleUrl(manifest.id, manifest.main, isWindows(), Date.now());
	const loadCycleId = crypto.randomUUID();
	let mod: unknown;
	try {
		mod = await import(/* @vite-ignore */ url);
	} catch (err) {
		// Read error fields across realms too; instanceof can miss a native loader error.
		const importError = err as { name?: unknown; message?: unknown; status?: unknown } | null | undefined;
		const msg = typeof importError?.message === "string" ? importError.message : String(err);
		// DEFERRED (2026-10-31) — the historical all-plugin failure batch recovered,
		// but its cause is unknown. Capture a recurrence before adding retries.
		// Include the load id in the message: appLogger coalesces equal messages, not equal data.
		appLogger.error("plugin", `Failed to import plugin "${manifest.id}" (load ${loadCycleId})`, {
			url,
			errorName: typeof importError?.name === "string" ? importError.name : null,
			errorMessage: msg,
			// Native import normally hides response status; do not issue a second request to guess it.
			httpStatus: typeof importError?.status === "number" ? importError.status : null,
			attempt: 1,
			// timeOrigin identifies this document across HMR, unlike a module-scoped UUID.
			documentId: performance.timeOrigin,
			loadCycleId,
			error: err,
		});
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
	// Await register() — it performs Rust registration + onload asynchronously.
	// Without the await we'd log success and add to loadedPluginIds before those
	// actually complete (race).
	if (!(await pluginRegistry.register(plugin, manifest.capabilities, manifest.agentTypes))) return;
	loadedPluginIds.add(manifest.id);
	replayActiveAgents(terminalsStore);

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
const pendingReloadIds = new Set<string>();
let reloadInFlight: Promise<void> | null = null;

function handlePluginChanged(event: { payload: string[] }): Promise<void> {
	if (!Array.isArray(event.payload) || event.payload.length === 0) return Promise.resolve();
	for (const id of event.payload) pendingReloadIds.add(id);
	if (!reloadInFlight) {
		reloadInFlight = Promise.resolve().then(async () => {
			try {
				while (pendingReloadIds.size > 0) {
					const ids = [...pendingReloadIds];
					pendingReloadIds.clear();
					await reloadChangedPlugins(ids);
				}
			} finally {
				reloadInFlight = null;
			}
		});
	}
	return reloadInFlight;
}

async function reloadChangedPlugins(changedIds: string[]): Promise<void> {
	const enabledIds = [...new Set(changedIds)].filter((id) => !disabledPluginIds.has(id));
	if (enabledIds.length === 0) return;
	let manifests: PluginManifest[];
	try {
		manifests = await invoke<PluginManifest[]>("list_user_plugins", { pluginIds: enabledIds });
	} catch (err) {
		appLogger.error("plugin", "Failed to list plugins for reload", err);
		return;
	}
	for (const pluginId of enabledIds) {
		appLogger.info("plugin", `Plugin "${pluginId}" changed, reloading...`);
		await pluginRegistry.unregister(pluginId);
		loadedPluginIds.delete(pluginId);

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
	}
}

/** Reload one plugin after a change the file watcher does not see (for example its data/config.json). */
export function reloadPlugin(pluginId: string): Promise<void> {
	return handlePluginChanged({ payload: [pluginId] });
}

/** Replay agent-started events for terminals that already have a running agent. */
export function replayActiveAgents(store: typeof terminalsStore): void {
	for (const id of store.getIds()) {
		const t = store.get(id);
		if (t?.agentType && t.sessionId) {
			pluginRegistry.notifyStateChange({
				type: "agent-started",
				sessionId: t.sessionId,
				terminalId: id,
			});
			pluginRegistry.dispatchStructuredEvent(
				"awaiting",
				{ awaiting: t.awaitingInput === "question", confident: t.awaitingInputConfident === true },
				t.sessionId,
			);
			if (t.shellState) {
				pluginRegistry.dispatchStructuredEvent(
					"shell-state",
					{ state: t.shellState, agent_type: t.agentType },
					t.sessionId,
				);
			}
		}
	}
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Discover and load all user plugins from the plugins directory.
 * Call once at app startup after the disabled-plugin state is synchronized.
 */
export async function loadUserPlugins(syncDisabled = true): Promise<void> {
	// DEFERRED (2026-10-20) — Isolated headless/Vite user-plugin E2Es need a browser
	// module-asset transport first. HTTP RPC parity alone cannot load plugin://
	// modules; do not remove this guard or substitute a test loader as live proof.
	if (!isTauri()) {
		appLogger.debug("plugin", "User plugin loading skipped in browser mode");
		return;
	}

	if (syncDisabled) await syncDisabledList();

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

	await loadPluginsConcurrently(manifests);
}

export async function loadPluginsConcurrently(
	manifests: PluginManifest[],
	loader: (manifest: PluginManifest) => Promise<void> = loadPlugin,
): Promise<void> {
	await Promise.all(
		manifests.map(async (manifest) => {
			const error = validateManifest(manifest);
			if (error) {
				appLogger.error("plugin", `Skipping plugin: ${error}`);
				return;
			}

			if (disabledPluginIds.has(manifest.id)) {
				pluginStore.registerPlugin(manifest.id, {
					manifest,
					builtIn: false,
					enabled: false,
					loaded: false,
				});
				appLogger.info("plugin", `Plugin "${manifest.id}" is disabled, skipping`);
				return;
			}

			await loader(manifest);
		}),
	);
}
