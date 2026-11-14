import { createStore, produce } from "solid-js/store";
import {
	AGENTS,
	type AgentRunConfig,
	type AgentsConfig,
	type AgentType,
	type EgoPermissionMode,
	type EgoSandbox,
	type HeadlessAgentChoice,
	toHeadlessAgentChoice,
} from "../agents";
import { invoke } from "../invoke";
import { reportSettingsConflicts, settingsRefresh } from "../services/settingsRefresh";
import { rpc } from "../transport";
import { getRemoteBaseUrl, getRepoConnection } from "../transportRuntime";
import { createConfigDeltaWriter } from "../utils/configDeltaWriter";
import { buildEnvFromEntries, type EnvVarEntry } from "../utils/envVars";
import { appLogger } from "./appLogger";

export interface AgentConfigIO {
	load: () => Promise<AgentsConfig>;
	save: (base: AgentsConfig, config: AgentsConfig) => Promise<void>;
}

const defaultIO: AgentConfigIO = {
	load: () => invoke<AgentsConfig>("load_agents_config"),
	save: (base, config) => invoke("save_agents_config", { base, config }),
};

/**
 * Read and write the `agents.json` of one remote machine.
 *
 * `load_agents_config` carries no path and no session, so the transport's
 * argument-driven routing cannot place it: the connection has to be named.
 */
function remoteIO(connectionId: string): AgentConfigIO {
	return {
		load: async () => {
			try {
				return await rpc<AgentsConfig>("load_agents_config", {}, connectionId);
			} catch (err) {
				const endpoint = `${getRemoteBaseUrl(connectionId) ?? "disconnected"}/config/agents`;
				throw new Error(`${connectionId} (${endpoint}): ${err instanceof Error ? err.message : String(err)}`);
			}
		},
		save: (base, config) => rpc<void>("save_agents_config", { base, config }, connectionId),
	};
}

interface AgentConfigsState {
	hibernate_idle_minutes: number;
	abandoned_shell_hours: number;
	hibernate_scope: "background_only" | "always";
	agents: Record<
		string,
		{
			run_configs: AgentRunConfig[];
			idle_close_minutes?: number;
			auto_retry_on_error?: boolean;
			headless_template?: string;
			env_flags?: Record<string, string>;
			intent_tab_title?: boolean;
			progress_tracking?: boolean;
			suggest_followups?: boolean;
			hook_instrumentation?: boolean;
			native_status_signals?: boolean;
			prevent_alt_screen?: boolean;
			skip_trust_dialog?: boolean;
			ego_mode?: EgoPermissionMode;
			ego_sandbox?: EgoSandbox;
			launch_instructions?: string;
			launch_modules?: Record<string, boolean>;
			launch_instructions_disabled?: boolean;
		}
	>;
	/** Which agent CLI to use for headless prompt execution (user-chosen in Settings) */
	headless_agent: HeadlessAgentChoice | null;
	/** Run configuration name that workflow Agent steps launch with; null means none chosen. */
	workflow_profile: string | null;
	/** Global user additions; null means no additions. */
	launch_instructions: string | null;
	launch_modules: Record<string, boolean>;
	launch_instructions_disabled: boolean;
	launch_additions_migrated: boolean;
	loaded: boolean;
	loadError: string | null;
}

/** Deep-clone a plain object to break SolidJS proxy references.
 * Must use JSON round-trip because structuredClone cannot handle SolidJS proxies. */
function clone<T>(obj: T): T {
	return JSON.parse(JSON.stringify(obj));
}

export function createAgentConfigsStore(io: AgentConfigIO = defaultIO, connectionId?: string) {
	const writer = createConfigDeltaWriter<AgentsConfig>("save_agents_config", "config", io.save);
	const [state, setState] = createStore<AgentConfigsState>({
		agents: {},
		hibernate_idle_minutes: 60,
		abandoned_shell_hours: 0,
		hibernate_scope: "background_only",
		headless_agent: null,
		workflow_profile: null,
		launch_instructions: null,
		launch_modules: {},
		launch_instructions_disabled: false,
		launch_additions_migrated: true,
		loaded: false,
		loadError: null,
	});

	function wire(): AgentsConfig {
		return {
			agents: clone(state.agents),
			hibernate_idle_minutes: state.hibernate_idle_minutes,
			abandoned_shell_hours: state.abandoned_shell_hours,
			hibernate_scope: state.hibernate_scope,
			headless_agent: state.headless_agent ?? undefined,
			workflow_profile: state.workflow_profile ?? undefined,
			launch_instructions: state.launch_instructions ?? undefined,
			launch_modules: clone(state.launch_modules),
			launch_instructions_disabled: state.launch_instructions_disabled,
			launch_additions_migrated: state.launch_additions_migrated,
		};
	}

	function applyLoaded(config: AgentsConfig): void {
		setState(
			produce((s) => {
				s.agents = config.agents ?? {};
				s.hibernate_idle_minutes = config.hibernate_idle_minutes ?? 60;
				s.abandoned_shell_hours = config.abandoned_shell_hours ?? 0;
				s.hibernate_scope = config.hibernate_scope ?? "background_only";
				s.headless_agent = toHeadlessAgentChoice(config.headless_agent ?? "");
				s.workflow_profile = config.workflow_profile ?? null;
				s.launch_instructions = config.launch_instructions ?? null;
				s.launch_modules = config.launch_modules ?? {};
				s.launch_instructions_disabled = config.launch_instructions_disabled ?? false;
				s.launch_additions_migrated = config.launch_additions_migrated ?? true;
				s.loaded = true;
				s.loadError = null;
			}),
		);
	}

	/** Save changes since the last acknowledged snapshot. */
	async function saveToDisk(): Promise<void> {
		try {
			if (!state.loaded) throw new Error("Agent config has not loaded; save refused");
			await writer.save(wire());
		} catch (err) {
			appLogger.error("config", "Failed to save agent config", err);
			throw err;
		}
	}

	const actions = {
		async setLaunchModule(id: string, enabled: boolean, type?: AgentType): Promise<void> {
			setState(
				produce((s) => {
					if (type) {
						if (!s.agents[type]) s.agents[type] = { run_configs: [] };
						s.agents[type].launch_modules = { ...s.agents[type].launch_modules, [id]: enabled };
					} else s.launch_modules[id] = enabled;
				}),
			);
			await saveToDisk();
		},
		async setLaunchInstructionsDisabled(disabled: boolean, type?: AgentType): Promise<void> {
			setState(
				produce((s) => {
					if (type) {
						if (!s.agents[type]) s.agents[type] = { run_configs: [] };
						s.agents[type].launch_instructions_disabled = disabled;
					} else s.launch_instructions_disabled = disabled;
				}),
			);
			await saveToDisk();
		},

		startRemoteSync(): () => void {
			return settingsRefresh.register({
				section: "agents",
				connectionId,
				read: io.load,
				apply: (remote) => {
					if (!state.loaded) return;
					const merged = writer.adopt(remote, wire());
					applyLoaded(merged.value);
					reportSettingsConflicts(merged.conflicts);
				},
			});
		},
		async setAbandonedShellHours(hours: number): Promise<void> {
			if (!Number.isInteger(hours) || hours < 0 || hours > 65535) return;
			setState({ abandoned_shell_hours: hours });
			await saveToDisk();
		},
		async setHibernate(minutes: number, scope: "background_only" | "always"): Promise<void> {
			if (!Number.isInteger(minutes) || minutes < 0 || minutes > 1440) return;
			setState({ hibernate_idle_minutes: minutes, hibernate_scope: scope });
			await saveToDisk();
		},
		/** Hydrate store from persisted config */
		async hydrate(): Promise<void> {
			try {
				const config = await io.load();
				writer.loaded(config);
				applyLoaded(config);
			} catch (err) {
				appLogger.error("config", "Failed to hydrate agent configs", err);
				setState({ loaded: false, loadError: err instanceof Error ? err.message : String(err) });
				throw err;
			}
		},

		/** Persist ego launch choices through the existing machine-specific config transport. */
		async setEgoPermissions(permissions: { ego_mode?: EgoPermissionMode; ego_sandbox?: EgoSandbox }): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents.ego) s.agents.ego = { run_configs: [] };
					Object.assign(s.agents.ego, permissions);
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				/* saveToDisk logs the error */
			}
		},

		/** Get run configs for an agent type */
		getRunConfigs(type: AgentType): AgentRunConfig[] {
			return state.agents[type]?.run_configs ?? [];
		},

		/** Get the default run config for an agent, or undefined */
		getDefaultConfig(type: AgentType): AgentRunConfig | undefined {
			const configs = state.agents[type]?.run_configs ?? [];
			return configs.find((c) => c.is_default) ?? configs[0];
		},

		/** Add a run config for an agent */
		async addRunConfig(type: AgentType, config: AgentRunConfig): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					const newConfig = clone(config);
					if (s.agents[type].run_configs.length === 0) {
						newConfig.is_default = true;
					}
					s.agents[type].run_configs.push(newConfig);
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Update a run config at a specific index */
		async updateRunConfig(type: AgentType, index: number, config: AgentRunConfig): Promise<void> {
			const current = state.agents[type]?.run_configs ?? [];
			if (index < 0 || index >= current.length) return;
			setState(
				produce((s) => {
					s.agents[type].run_configs[index] = clone(config);
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Remove a run config at a specific index */
		async removeRunConfig(type: AgentType, index: number): Promise<void> {
			const current = state.agents[type]?.run_configs ?? [];
			if (index < 0 || index >= current.length) return;
			setState(
				produce((s) => {
					const configs = s.agents[type].run_configs;
					const wasDefault = configs[index].is_default;
					configs.splice(index, 1);
					if (wasDefault && configs.length > 0) {
						configs[0].is_default = true;
					}
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/**
		 * Update env vars for a run config at a specific index.
		 * Throws if entries contain duplicate keys — callers must validate first
		 * to avoid silently losing data on key collisions.
		 */
		async updateRunConfigEnv(type: AgentType, index: number, entries: readonly EnvVarEntry[]): Promise<void> {
			const current = state.agents[type]?.run_configs ?? [];
			if (index < 0 || index >= current.length) return;
			const env = buildEnvFromEntries(entries);
			setState(
				produce((s) => {
					s.agents[type].run_configs[index].env = env;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Delay before a finished managed child is closed. */
		getIdleCloseMinutes(type: AgentType): number {
			return state.agents[type]?.idle_close_minutes ?? 15;
		},

		async setIdleCloseMinutes(type: AgentType, minutes: number): Promise<void> {
			if (!Number.isInteger(minutes) || minutes < 0 || minutes > 1440)
				throw new Error("Idle close must be 0–1440 minutes");
			setState(
				produce((s) => {
					if (!s.agents[type]) s.agents[type] = { run_configs: [] };
					s.agents[type].idle_close_minutes = minutes;
				}),
			);
			await saveToDisk();
		},

		/** Check if auto-retry on error is enabled for an agent */
		isAutoRetryEnabled(type: AgentType): boolean {
			return state.agents[type]?.auto_retry_on_error === true;
		},

		/** Toggle auto-retry on error for an agent */
		async setAutoRetry(type: AgentType, enabled: boolean): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].auto_retry_on_error = enabled;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get the headless command template for an agent (user override or built-in default) */
		getHeadlessTemplate(type: AgentType): string | undefined {
			return state.agents[type]?.headless_template ?? AGENTS[type]?.defaultHeadlessTemplate;
		},

		/** Global launch instructions override (undefined = shipped default). */
		getLaunchInstructions(): string | undefined {
			return state.launch_instructions ?? undefined;
		},

		/** Set the global launch instructions. Pass undefined to reset to the default. */
		async setLaunchInstructions(text: string | undefined): Promise<void> {
			setState("launch_instructions", text ?? null);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Per-agent launch instructions override (undefined = the global text). */
		getAgentLaunchInstructions(type: AgentType): string | undefined {
			return state.agents[type]?.launch_instructions;
		},

		/** Set per-agent launch instructions. Pass undefined to reset to the global text. */
		async setAgentLaunchInstructions(type: AgentType, text: string | undefined): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].launch_instructions = text;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get the run configuration name workflow Agent steps launch with */
		getWorkflowProfile(): string | null {
			return state.workflow_profile;
		},

		/** Choose the run configuration workflow Agent steps launch with */
		async setWorkflowProfile(name: string | null): Promise<void> {
			setState("workflow_profile", name);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get the globally configured headless agent */
		getHeadlessAgent(): HeadlessAgentChoice | null {
			return state.headless_agent;
		},

		/** Set the globally configured headless agent */
		async setHeadlessAgent(type: HeadlessAgentChoice | null): Promise<void> {
			setState("headless_agent", type);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Set the headless command template for an agent */
		async setHeadlessTemplate(type: AgentType, template: string): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].headless_template = template || undefined;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Set a specific config as the default (unset others) */
		async setDefaultConfig(type: AgentType, index: number): Promise<void> {
			const current = state.agents[type]?.run_configs ?? [];
			if (index < 0 || index >= current.length) return;
			setState(
				produce((s) => {
					const configs = s.agents[type].run_configs;
					for (let i = 0; i < configs.length; i++) {
						configs[i].is_default = i === index;
					}
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get per-agent intent_tab_title override (undefined = use default) */
		getIntentTabTitle(type: AgentType): boolean | undefined {
			return state.agents[type]?.intent_tab_title;
		},

		/** Set per-agent intent_tab_title override. Pass undefined to reset to default. */
		async setIntentTabTitle(type: AgentType, value: boolean | undefined): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].intent_tab_title = value;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get per-agent progress_tracking override (undefined = follow the global flag) */
		getProgressTracking(type: AgentType): boolean | undefined {
			return state.agents[type]?.progress_tracking;
		},

		/** Set per-agent progress_tracking override. Pass undefined to follow the global flag. */
		async setProgressTracking(type: AgentType, value: boolean | undefined): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].progress_tracking = value;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get per-agent suggest_followups override (undefined = use default) */
		getSuggestFollowups(type: AgentType): boolean | undefined {
			return state.agents[type]?.suggest_followups;
		},

		/** Set per-agent suggest_followups override. Pass undefined to reset to default. */
		async setSuggestFollowups(type: AgentType, value: boolean | undefined): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					s.agents[type].suggest_followups = value;
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},

		/** Get per-agent hook_instrumentation flag (undefined/false = off). */
		getHookInstrumentation(type: AgentType): boolean | undefined {
			return state.agents[type]?.hook_instrumentation;
		},

		/**
		 * Mirror the hook_instrumentation flag in memory after the
		 * `set_agent_hook_instrumentation` command has persisted it (and installed/
		 * removed the hooks). Does NOT save to disk — the command owns persistence.
		 * Off is the default and the command stores it as absent, so it is
		 * mirrored as absent: a later whole-file save must not write it back.
		 */
		syncHookInstrumentation(type: AgentType, value: boolean): void {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					if (value) s.agents[type].hook_instrumentation = true;
					else delete s.agents[type].hook_instrumentation;
				}),
			);
		},

		/** Missing is deliberately on: launch-scoped signals are the safe default. */
		getNativeStatusSignals(type: AgentType): boolean {
			return state.agents[type]?.native_status_signals ?? true;
		},

		/** Mirror of `set_agent_native_status_signals`: on is the default and is
		 * stored as absent, so a later whole-file save does not write it back. */
		syncNativeStatusSignals(type: AgentType, value: boolean): void {
			setState(
				produce((s) => {
					if (!s.agents[type]) s.agents[type] = { run_configs: [] };
					if (value) delete s.agents[type].native_status_signals;
					else s.agents[type].native_status_signals = false;
				}),
			);
		},

		getPreventAltScreen(type: AgentType): boolean {
			return state.agents[type]?.prevent_alt_screen ?? true;
		},

		async setPreventAltScreen(type: AgentType, enabled: boolean): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) s.agents[type] = { run_configs: [] };
					if (enabled) delete s.agents[type].prevent_alt_screen;
					else s.agents[type].prevent_alt_screen = false;
				}),
			);
			await saveToDisk();
		},

		getSkipTrustDialog(type: AgentType): boolean {
			return state.agents[type]?.skip_trust_dialog ?? true;
		},

		async setSkipTrustDialog(type: AgentType, enabled: boolean): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) s.agents[type] = { run_configs: [] };
					if (enabled) delete s.agents[type].skip_trust_dialog;
					else s.agents[type].skip_trust_dialog = false;
				}),
			);
			await saveToDisk();
		},

		/** Get all env flags for an agent */
		getEnvFlags(type: AgentType): Record<string, string> {
			return state.agents[type]?.env_flags ?? {};
		},

		/** Set a single env flag (key→value). Removes the flag if value is undefined. */
		async setEnvFlag(type: AgentType, key: string, value: string | undefined): Promise<void> {
			setState(
				produce((s) => {
					if (!s.agents[type]) {
						s.agents[type] = { run_configs: [] };
					}
					if (!s.agents[type].env_flags) {
						s.agents[type].env_flags = {};
					}
					if (value === undefined) {
						delete s.agents[type].env_flags![key];
					} else {
						s.agents[type].env_flags![key] = value;
					}
				}),
			);
			try {
				await saveToDisk();
			} catch (_err) {
				// saveToDisk already logged the error
			}
		},
	};

	return { state, ...actions };
}

export type AgentConfigStore = ReturnType<typeof createAgentConfigsStore>;

// ---------------------------------------------------------------------------
// One config per machine
// ---------------------------------------------------------------------------

/**
 * A run config describes a machine, not this app.
 *
 * `claude`, `grok` and `codex` live on the box that runs them, with their own
 * paths, their own licences and their own config directories, so a tab opened on
 * a remote repository has to launch with that machine's `agents.json`. A single
 * global config could only ever describe the Mac. The registry keys one store
 * per machine and the local one is the entry with no connection id.
 */
const machineStores = new Map<string, AgentConfigStore>();
const machineHydrations = new Map<string, Promise<void>>();

/** The key a machine is cached under. Local is the empty id — there is no connection. */
const LOCAL_MACHINE = "";
let liveSync = false;
const syncStops = new Map<string, () => void>();
export function startAgentSettingsSync(): () => void {
	liveSync = true;
	for (const [key, store] of machineStores) if (!syncStops.has(key)) syncStops.set(key, store.startRemoteSync());
	return () => {
		liveSync = false;
		for (const stop of syncStops.values()) stop();
		syncStops.clear();
	};
}

/**
 * The store holding one machine's run configs, created on first use.
 *
 * Synchronous on purpose: a context menu is built inside the click that opens
 * it. A machine whose config has not been read yet answers with an empty set
 * rather than another machine's — `ensureAgentConfigs` is what fills it, and a
 * connection that comes up prefetches so the menu is warm before it is opened.
 */
export function agentConfigsFor(connectionId?: string | null): AgentConfigStore {
	const key = connectionId ?? LOCAL_MACHINE;
	let store = machineStores.get(key);
	if (!store) {
		store = createAgentConfigsStore(connectionId ? remoteIO(connectionId) : defaultIO, connectionId ?? undefined);
		machineStores.set(key, store);
		if (liveSync) syncStops.set(key, store.startRemoteSync());
	}
	return store;
}

/**
 * The same store, with its first read completed.
 *
 * The local machine is hydrated at boot, so a local launch resolves here with no
 * round trip at all. Concurrent callers share one load.
 */
export function ensureAgentConfigs(connectionId?: string | null): Promise<AgentConfigStore> {
	const key = connectionId ?? LOCAL_MACHINE;
	const store = agentConfigsFor(connectionId);
	if (store.state.loaded) return Promise.resolve(store);
	let pending = machineHydrations.get(key);
	if (!pending) {
		pending = store.hydrate().finally(() => machineHydrations.delete(key));
		machineHydrations.set(key, pending);
	}
	return pending.then(() => store);
}

/**
 * Forget a machine's config so the next reader loads it again.
 *
 * Called when a connection changes state: a daemon that went away and came back
 * may have been reinstalled, reconfigured, or be a different machine entirely.
 * The local entry is never dropped — nothing invalidates it.
 */
export function invalidateAgentConfigs(connectionId: string): void {
	if (!connectionId) return;
	syncStops.get(connectionId)?.();
	syncStops.delete(connectionId);
	machineStores.delete(connectionId);
	machineHydrations.delete(connectionId);
}

/** The run configs of the machine that owns `repoPath`, loaded if needed. */
export function ensureAgentConfigsForRepo(repoPath?: string | null): Promise<AgentConfigStore> {
	return ensureAgentConfigs(getRepoConnection(repoPath));
}

/** The same, without waiting — for the synchronous menu-building path. */
export function agentConfigsForRepo(repoPath?: string | null): AgentConfigStore {
	return agentConfigsFor(getRepoConnection(repoPath));
}

/** The local machine's configs. Kept as a binding because most callers are local-only. */
export const agentConfigsStore = agentConfigsFor();
