import { type Component, createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import {
	AGENT_DISPLAY,
	AGENT_TYPES,
	AGENTS,
	type AgentHookState,
	type AgentRunConfig,
	type AgentType,
	HOOK_SUPPORT,
	MCP_SUPPORT,
} from "../../../agents";
import {
	CATEGORY_ORDER,
	CC_ENV_FLAGS,
	ENV_FLAG_CATEGORIES,
	type EnvFlagCategory,
	type EnvFlagDef,
} from "../../../data/ccEnvFlags";
import { type AgentAvailability, useAgentDetection } from "../../../hooks/useAgentDetection";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { setClaudeUsageEnabled } from "../../../plugins";
import { isPluginDisabled, setPluginEnabled } from "../../../plugins/pluginLoader";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { aiPromptsStore, DEFAULT_DIFF_TRIAGE_PROMPT } from "../../../stores/aiPrompts";
import { appLogger } from "../../../stores/appLogger";
import { editorTabsStore } from "../../../stores/editorTabs";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { isTauri } from "../../../transport";
import { onClickKeyDown } from "../../../utils/a11y";
import { buildEnvFromEntries, findDuplicateEnvKeys } from "../../../utils/envVars";
import { AgentIcon } from "../../ui/AgentIcon";
import { SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";
import { AgentConfigProvider, createRemoteAgentConfigStore, useAgentConfig } from "./agentConfigContext";

const ALL_AGENT_TYPES = AGENT_TYPES.filter((t): t is AgentType => t !== "git" && t !== "api");

/**
 * Build the set of lowercased run-config names across all agent groups,
 * optionally excluding one name (case-insensitive). Pure: no store access,
 * no Solid reactivity — callers pass pre-fetched configs so this is trivially
 * testable and safe to reuse from a future rename UI without the duplicate
 * check flagging the row's own name.
 */
export function collectRunConfigNames(
	configsByAgent: Array<Array<{ name: string }>>,
	excludeName?: string,
): Set<string> {
	const names = new Set<string>();
	const excluded = excludeName?.toLowerCase();
	let excludedSeen = false;
	for (const configs of configsByAgent) {
		for (const cfg of configs) {
			const n = cfg.name.toLowerCase();
			if (!excludedSeen && n === excluded) {
				excludedSeen = true;
				continue;
			}
			names.add(n);
		}
	}
	return names;
}

interface McpStatus {
	supported: boolean;
	installed: boolean;
	config_path: string | null;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Key-value row for env var editing */
const EnvVarRow: Component<{
	key: string;
	value: string;
	duplicate?: boolean;
	onChange: (key: string, value: string) => void;
	onRemove: () => void;
}> = (props) => (
	<div class={a.envVarRow}>
		<input
			class={`${a.formInput} ${a.mono} ${a.envVarKey}`}
			classList={{ [a.inputError]: !!props.duplicate }}
			placeholder="KEY"
			value={props.key}
			onInput={(e) => props.onChange(e.currentTarget.value, props.value)}
		/>
		<span class={a.envVarEquals}>=</span>
		<input
			class={`${a.formInput} ${a.mono} ${a.envVarValue}`}
			placeholder="value"
			value={props.value}
			onInput={(e) => props.onChange(props.key, e.currentTarget.value)}
		/>
		<button class={`${a.smallBtn} ${a.danger}`} onClick={props.onRemove} title="Remove">
			×
		</button>
	</div>
);

/** Inline form for adding a new run config */
const AddConfigForm: Component<{
	agentType: AgentType;
	onClose: () => void;
}> = (props) => {
	const configStore = useAgentConfig();
	const [name, setName] = createSignal("");
	const [command, setCommand] = createSignal(AGENTS[props.agentType].binary);
	const [args, setArgs] = createSignal("");
	const [envVars, setEnvVars] = createSignal<Array<{ key: string; value: string }>>([]);

	// Cross-agent duplicate name detection (case-insensitive). AddConfigForm
	// passes no excludeName — every existing name is a duplicate for a NEW
	// config. A future rename UI will pass the row's current name so the user
	// isn't flagged as duplicating themselves (story 1278-365e).
	const allExistingNames = createMemo(() =>
		collectRunConfigNames(ALL_AGENT_TYPES.map((t) => configStore.getRunConfigs(t))),
	);
	const isDuplicate = () => {
		const n = name().trim().toLowerCase();
		return n.length > 0 && allExistingNames().has(n);
	};

	const addEnvVar = () => setEnvVars([...envVars(), { key: "", value: "" }]);
	const removeEnvVar = (idx: number) => setEnvVars(envVars().filter((_, i) => i !== idx));
	const updateEnvVar = (idx: number, key: string, value: string) => {
		setEnvVars(envVars().map((v, i) => (i === idx ? { key, value } : v)));
	};

	const duplicateEnvKeys = createMemo(() => findDuplicateEnvKeys(envVars()));
	const duplicateEnvKeysSet = createMemo(() => new Set(duplicateEnvKeys()));

	const handleSave = async () => {
		const n = name().trim();
		if (!n || isDuplicate() || duplicateEnvKeys().length > 0) return;
		const config: AgentRunConfig = {
			name: n,
			command: command().trim() || AGENTS[props.agentType].binary,
			args: args().trim() ? args().trim().split(/\s+/) : [],
			env: buildEnvFromEntries(envVars()),
			is_default: false,
		};
		await configStore.addRunConfig(props.agentType, config);
		props.onClose();
	};

	return (
		<div class={a.addConfigForm}>
			<div class={a.formRow}>
				<input
					class={a.formInput}
					classList={{ [a.inputError]: isDuplicate() }}
					placeholder="Configuration name"
					value={name()}
					onInput={(e) => setName(e.currentTarget.value)}
					onKeyDown={(e) => {
						if (e.key === "Enter") handleSave();
						if (e.key === "Escape") props.onClose();
					}}
				/>
			</div>
			<Show when={isDuplicate()}>
				<div class={a.validationError}>Name "{name().trim()}" already exists</div>
			</Show>
			<div class={a.formRow}>
				<input
					class={`${a.formInput} ${a.mono}`}
					placeholder="Command (binary)"
					value={command()}
					onInput={(e) => setCommand(e.currentTarget.value)}
				/>
				<input
					class={`${a.formInput} ${a.mono}`}
					placeholder="Arguments (space-separated)"
					value={args()}
					onInput={(e) => setArgs(e.currentTarget.value)}
					onKeyDown={(e) => {
						if (e.key === "Enter") handleSave();
						if (e.key === "Escape") props.onClose();
					}}
				/>
			</div>
			{/* Env vars section */}
			<div class={a.envVarsSection}>
				<div class={a.envVarsHeader}>
					<span class={a.envVarsLabel}>Environment Variables</span>
					<button class={a.smallBtn} onClick={addEnvVar} type="button">
						+ Add
					</button>
				</div>
				<Show when={envVars().length > 0}>
					<div class={a.envVarsList}>
						<For each={envVars()}>
							{(v, i) => (
								<EnvVarRow
									key={v.key}
									value={v.value}
									duplicate={duplicateEnvKeysSet().has(v.key.trim())}
									onChange={(k, val) => updateEnvVar(i(), k, val)}
									onRemove={() => removeEnvVar(i())}
								/>
							)}
						</For>
					</div>
				</Show>
				<Show when={duplicateEnvKeys().length > 0}>
					<div class={a.validationError}>Duplicate env keys: {duplicateEnvKeys().join(", ")}</div>
				</Show>
			</div>
			<div class={a.formRow}>
				<button
					class={a.smallBtn}
					onClick={handleSave}
					disabled={isDuplicate() || !name().trim() || duplicateEnvKeys().length > 0}
				>
					Save
				</button>
				<button class={a.smallBtn} onClick={props.onClose}>
					Cancel
				</button>
			</div>
		</div>
	);
};

/** Single run config row with inline env var editing and config editing */
const RunConfigRow: Component<{
	config: AgentRunConfig;
	index: number;
	agentType: AgentType;
}> = (props) => {
	const configStore = useAgentConfig();
	const [editingEnv, setEditingEnv] = createSignal(false);
	const [editingConfig, setEditingConfig] = createSignal(false);
	const [menuOpen, setMenuOpen] = createSignal(false);
	const [envVars, setEnvVars] = createSignal<Array<{ key: string; value: string }>>([]);
	const [editName, setEditName] = createSignal("");
	const [editCommand, setEditCommand] = createSignal("");
	const [editArgs, setEditArgs] = createSignal("");
	let menuRef: HTMLDivElement | undefined;

	const cmdPreview = () => {
		const parts = [props.config.command, ...props.config.args];
		return parts.join(" ");
	};

	const envCount = () => Object.keys(props.config.env).length;

	const startEnvEdit = () => {
		const entries = Object.entries(props.config.env).map(([key, value]) => ({ key, value }));
		setEnvVars(entries);
		setEditingEnv(true);
	};

	const startConfigEdit = () => {
		setEditName(props.config.name);
		setEditCommand(props.config.command);
		setEditArgs(props.config.args.join(" "));
		setEditingConfig(true);
		setMenuOpen(false);
	};

	const allExistingNames = createMemo(() =>
		collectRunConfigNames(
			ALL_AGENT_TYPES.map((t) => configStore.getRunConfigs(t)),
			props.config.name,
		),
	);
	const isDuplicateName = () => {
		const n = editName().trim().toLowerCase();
		return n.length > 0 && allExistingNames().has(n);
	};

	const saveConfig = async () => {
		const n = editName().trim();
		if (!n || isDuplicateName()) return;
		const updated: AgentRunConfig = {
			...props.config,
			name: n,
			command: editCommand().trim() || props.config.command,
			args: editArgs().trim() ? editArgs().trim().split(/\s+/) : [],
		};
		await configStore.updateRunConfig(props.agentType, props.index, updated);
		setEditingConfig(false);
	};

	const handleDelete = () => {
		setMenuOpen(false);
		configStore.removeRunConfig(props.agentType, props.index);
	};

	const addEnvVar = () => setEnvVars([...envVars(), { key: "", value: "" }]);
	const removeEnvVar = (idx: number) => setEnvVars(envVars().filter((_, i) => i !== idx));
	const updateEnvVar = (idx: number, key: string, value: string) => {
		setEnvVars(envVars().map((v, i) => (i === idx ? { key, value } : v)));
	};

	const duplicateEnvKeys = createMemo(() => findDuplicateEnvKeys(envVars()));
	const duplicateEnvKeysSet = createMemo(() => new Set(duplicateEnvKeys()));

	const saveEnv = async () => {
		if (duplicateEnvKeys().length > 0) return;
		await configStore.updateRunConfigEnv(props.agentType, props.index, envVars());
		setEditingEnv(false);
	};

	const handleClickOutside = (e: MouseEvent) => {
		if (menuRef && !menuRef.contains(e.target as Node)) setMenuOpen(false);
	};
	onMount(() => document.addEventListener("mousedown", handleClickOutside));
	onCleanup(() => document.removeEventListener("mousedown", handleClickOutside));

	return (
		<div class={a.configRowWrap}>
			<div class={a.configRow}>
				<span class={a.configName}>{props.config.name}</span>
				<span class={a.configCommand}>{cmdPreview()}</span>
				<Show when={envCount() > 0}>
					<span class={a.envBadge}>{envCount()} env</span>
				</Show>
				<Show when={props.config.is_default}>
					<span class={a.defaultBadge}>Default</span>
				</Show>
				<div class={a.configActions}>
					<button class={a.smallBtn} onClick={startEnvEdit} title="Edit environment variables">
						Env
					</button>
					<Show when={!props.config.is_default}>
						<button
							class={a.smallBtn}
							onClick={() => configStore.setDefaultConfig(props.agentType, props.index)}
							title="Set as default"
						>
							Set Default
						</button>
					</Show>
					<div class={a.menuWrap} ref={menuRef}>
						<button class={a.smallBtn} onClick={() => setMenuOpen(!menuOpen())} title="More actions">
							···
						</button>
						<Show when={menuOpen()}>
							<div class={a.menuDropdown}>
								<button class={a.menuItem} onClick={startConfigEdit}>
									Edit
								</button>
								<button class={`${a.menuItem} ${a.danger}`} onClick={handleDelete}>
									Delete
								</button>
							</div>
						</Show>
					</div>
				</div>
			</div>
			<Show when={editingConfig()}>
				<div class={a.envEditPanel}>
					<div class={a.envVarsHeader}>
						<span class={a.envVarsLabel}>Edit Configuration</span>
					</div>
					<div class={a.formRow}>
						<input
							class={a.formInput}
							classList={{ [a.inputError]: isDuplicateName() }}
							placeholder="Configuration name"
							value={editName()}
							onInput={(e) => setEditName(e.currentTarget.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter") saveConfig();
								if (e.key === "Escape") setEditingConfig(false);
							}}
						/>
					</div>
					<Show when={isDuplicateName()}>
						<div class={a.validationError}>Name "{editName().trim()}" already exists</div>
					</Show>
					<div class={a.formRow}>
						<input
							class={`${a.formInput} ${a.mono}`}
							placeholder="Command (binary)"
							value={editCommand()}
							onInput={(e) => setEditCommand(e.currentTarget.value)}
						/>
						<input
							class={`${a.formInput} ${a.mono}`}
							placeholder="Arguments (space-separated)"
							value={editArgs()}
							onInput={(e) => setEditArgs(e.currentTarget.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter") saveConfig();
								if (e.key === "Escape") setEditingConfig(false);
							}}
						/>
					</div>
					<div class={a.formRow}>
						<button class={a.smallBtn} onClick={saveConfig} disabled={isDuplicateName() || !editName().trim()}>
							Save
						</button>
						<button class={a.smallBtn} onClick={() => setEditingConfig(false)}>
							Cancel
						</button>
					</div>
				</div>
			</Show>
			<Show when={editingEnv()}>
				<div class={a.envEditPanel}>
					<div class={a.envVarsHeader}>
						<span class={a.envVarsLabel}>Environment Variables</span>
						<button class={a.smallBtn} onClick={addEnvVar} type="button">
							+ Add
						</button>
					</div>
					<div class={a.envVarsList}>
						<For each={envVars()}>
							{(v, i) => (
								<EnvVarRow
									key={v.key}
									value={v.value}
									duplicate={duplicateEnvKeysSet().has(v.key.trim())}
									onChange={(k, val) => updateEnvVar(i(), k, val)}
									onRemove={() => removeEnvVar(i())}
								/>
							)}
						</For>
					</div>
					<Show when={duplicateEnvKeys().length > 0}>
						<div class={a.validationError}>Duplicate env keys: {duplicateEnvKeys().join(", ")}</div>
					</Show>
					<div class={a.formRow}>
						<button class={a.smallBtn} onClick={saveEnv} disabled={duplicateEnvKeys().length > 0}>
							Save
						</button>
						<button class={a.smallBtn} onClick={() => setEditingEnv(false)}>
							Cancel
						</button>
					</div>
				</div>
			</Show>
		</div>
	);
};

/** Environment flags panel — categorized toggles/inputs for CC env vars */
const EnvFlagsSection: Component<{ agentType: AgentType }> = (props) => {
	const configStore = useAgentConfig();
	const [expanded, setExpanded] = createSignal(false);
	const flags = () => configStore.getEnvFlags(props.agentType);

	const flagsByCategory = () => {
		const grouped: Partial<Record<EnvFlagCategory, EnvFlagDef[]>> = {};
		for (const flag of CC_ENV_FLAGS) {
			if (!grouped[flag.category]) grouped[flag.category] = [];
			grouped[flag.category]!.push(flag);
		}
		return grouped;
	};

	const isFlagEnabled = (key: string): boolean => key in flags();

	const getFlagValue = (key: string): string => flags()[key] ?? "";

	const handleBoolToggle = (flag: EnvFlagDef) => {
		if (isFlagEnabled(flag.key)) {
			configStore.setEnvFlag(props.agentType, flag.key, undefined);
		} else {
			configStore.setEnvFlag(props.agentType, flag.key, flag.type === "boolean_inverted" ? "false" : "1");
		}
	};

	const handleValueChange = (key: string, value: string) => {
		if (value) {
			configStore.setEnvFlag(props.agentType, key, value);
		} else {
			configStore.setEnvFlag(props.agentType, key, undefined);
		}
	};

	const activeCount = () => Object.keys(flags()).length;

	return (
		<div class={a.expandedSection}>
			<div
				class={a.expandedLabel}
				style={{ cursor: "pointer", display: "flex", "align-items": "center", gap: "6px" }}
				onClick={() => setExpanded(!expanded())}
			>
				<span class={a.expandIcon} classList={{ [a.expanded]: expanded() }}>
					&#9654;
				</span>
				Environment Flags
				<Show when={activeCount() > 0}>
					<span class={a.badge} data-type="available">
						{activeCount()}
					</span>
				</Show>
			</div>
			<p class={s.hint}>Feature flags injected into new terminal sessions</p>

			<Show when={expanded()}>
				<div class={a.envFlagsGrid}>
					<For each={CATEGORY_ORDER}>
						{(cat) => {
							const catFlags = () => flagsByCategory()[cat];
							return (
								<Show when={catFlags()?.length}>
									<div class={a.envFlagCategory}>
										<div class={a.envCategoryLabel}>{ENV_FLAG_CATEGORIES[cat]}</div>
										<For each={catFlags()}>
											{(flag) => (
												<div class={a.envFlagRow}>
													<Show when={flag.type === "boolean" || flag.type === "boolean_inverted"}>
														<input
															type="checkbox"
															class={a.envFlagToggle}
															checked={isFlagEnabled(flag.key)}
															onChange={() => handleBoolToggle(flag)}
														/>
													</Show>
													<Show when={flag.type === "enum"}>
														<select
															class={a.envFlagSelect}
															value={getFlagValue(flag.key)}
															onChange={(e) => handleValueChange(flag.key, e.currentTarget.value)}
														>
															<option value="">off</option>
															<For each={flag.options ?? []}>{(opt) => <option value={opt}>{opt}</option>}</For>
														</select>
													</Show>
													<Show when={flag.type === "number"}>
														<input
															type="number"
															class={a.envFlagInput}
															value={getFlagValue(flag.key)}
															placeholder={flag.defaultValue ?? ""}
															onInput={(e) => handleValueChange(flag.key, e.currentTarget.value)}
														/>
													</Show>
													<Show when={flag.type === "string"}>
														<input
															type="text"
															class={a.envFlagInput}
															value={getFlagValue(flag.key)}
															placeholder={flag.defaultValue ?? ""}
															onInput={(e) => handleValueChange(flag.key, e.currentTarget.value)}
														/>
													</Show>
													<span class={a.envFlagKey}>{flag.key}</span>
													<span class={a.envFlagDesc} title={flag.description}>
														{flag.description}
													</span>
												</div>
											)}
										</For>
									</div>
								</Show>
							);
						}}
					</For>
				</div>
			</Show>
		</div>
	);
};

/** Toggle for the native Claude Usage Dashboard feature */
const ClaudeUsageToggle: Component = () => {
	const [enabled, setEnabled] = createSignal(!isPluginDisabled("claude-usage"));

	const handleToggle = async () => {
		const newState = !enabled();
		setEnabled(newState);
		try {
			await setPluginEnabled("claude-usage", newState);
			setClaudeUsageEnabled(newState);
		} catch (err) {
			appLogger.error("config", "Failed to toggle Claude Usage Dashboard", err);
			setEnabled(!newState); // revert on failure
		}
	};

	return (
		<div class={a.expandedSection}>
			<div class={a.expandedLabel}>Features</div>
			<div class={a.actionsRow}>
				<label class={a.toggleRow}>
					<input type="checkbox" checked={enabled()} onChange={handleToggle} />
					<span>Usage Dashboard</span>
				</label>
				<span class={s.hint}>
					Rate limits, session analytics, and activity heatmap in status bar and Activity Center
				</span>
			</div>
		</div>
	);
};

/** Expandable agent row */
const AgentRow: Component<{
	agentType: AgentType;
	detection: AgentAvailability | undefined;
	onExpand?: (type: AgentType) => void;
}> = (props) => {
	const configStore = useAgentConfig();
	const [expanded, setExpanded] = createSignal(false);
	const [addingConfig, setAddingConfig] = createSignal(false);
	const [mcpStatus, setMcpStatus] = createSignal<McpStatus | null>(null);
	const [mcpLoading, setMcpLoading] = createSignal(false);
	const [hookState, setHookState] = createSignal<AgentHookState | null>(null);
	const [hookLoading, setHookLoading] = createSignal(false);

	const agent = () => AGENTS[props.agentType];
	const display = () => AGENT_DISPLAY[props.agentType];
	const configs = () => configStore.getRunConfigs(props.agentType);
	const supportsMcp = () => MCP_SUPPORT[props.agentType];
	const supportsHooks = () => HOOK_SUPPORT[props.agentType];

	const loadMcpStatus = async () => {
		if (!supportsMcp() || !isTauri()) return;
		try {
			const status = await invoke<McpStatus>("get_agent_mcp_status", { agentType: props.agentType });
			setMcpStatus(status);
		} catch (err) {
			appLogger.error("config", `Failed to get MCP status for ${props.agentType}`, err);
		}
	};

	const loadHookState = async () => {
		if (!supportsHooks() || !isTauri()) return;
		try {
			const st = await invoke<AgentHookState>("get_agent_hook_state", { agentType: props.agentType });
			setHookState(st);
		} catch (err) {
			appLogger.error("config", `Failed to get hook state for ${props.agentType}`, err);
		}
	};

	const handleHookToggle = async () => {
		if (hookLoading()) return;
		setHookLoading(true);
		try {
			const next = !(configStore.getHookInstrumentation(props.agentType) ?? false);
			// The command persists the flag AND installs/removes the hooks.
			await invoke("set_agent_hook_instrumentation", { agentType: props.agentType, enabled: next });
			configStore.syncHookInstrumentation(props.agentType, next);
			await loadHookState();
		} catch (err) {
			appLogger.error("config", `Hook instrumentation toggle failed for ${props.agentType}`, err);
		} finally {
			setHookLoading(false);
		}
	};

	const handleExpand = () => {
		const newVal = !expanded();
		setExpanded(newVal);
		if (newVal) {
			loadMcpStatus();
			loadHookState();
			props.onExpand?.(props.agentType);
		}
	};

	const handleMcpToggle = async () => {
		if (mcpLoading()) return;
		setMcpLoading(true);
		try {
			const status = mcpStatus();
			if (status?.installed) {
				await invoke("remove_agent_mcp", { agentType: props.agentType });
			} else {
				await invoke("install_agent_mcp", { agentType: props.agentType });
			}
			await loadMcpStatus();
		} catch (err) {
			appLogger.error("config", `MCP toggle failed for ${props.agentType}`, err);
		} finally {
			setMcpLoading(false);
		}
	};

	const handleEditConfig = async () => {
		try {
			const configPath = await invoke<string | null>("get_agent_config_path", { agentType: props.agentType });
			if (configPath) {
				const repoPath = repositoriesStore.state.activeRepoPath ?? "";
				editorTabsStore.add(repoPath, configPath, undefined, { externalEditable: true });
			}
		} catch (err) {
			appLogger.error("config", `Failed to open config for ${props.agentType}`, err);
		}
	};

	const isEnabled = () => settingsStore.isAgentEnabled(props.agentType);

	return (
		<div class={a.agentRow}>
			<div class={a.agentHeader} role="button" tabIndex={0} onClick={handleExpand} onKeyDown={onClickKeyDown(handleExpand)}>
				<div class={a.agentInfo}>
					<div class={a.agentNameRow}>
						<div class={a.agentIcon} style={{ background: display().color, opacity: isEnabled() ? 1 : 0.4 }}>
							<AgentIcon agent={props.agentType} size={16} />
						</div>
						<span class={a.agentName} style={{ opacity: isEnabled() ? 1 : 0.5 }}>
							{agent().name}
						</span>
						<Show when={props.detection?.version}>
							<span class={a.agentVersion}>{props.detection!.version}</span>
						</Show>
						<Show
							when={props.detection?.available}
							fallback={
								<span class={a.badge} data-type="notfound">
									Not found
								</span>
							}
						>
							<Show
								when={isEnabled()}
								fallback={
									<span class={a.badge} data-type="disabled">
										Disabled
									</span>
								}
							>
								<span class={a.badge} data-type="available">
									Available
								</span>
							</Show>
						</Show>
						<Show when={mcpStatus()?.installed}>
							<span class={a.badge} data-type="mcp">
								MCP
							</span>
						</Show>
					</div>
				</div>
				<span class={a.expandIcon} classList={{ [a.expanded]: expanded() }}>
					&#9654;
				</span>
			</div>

			<Show when={expanded()}>
				<div class={a.agentExpanded}>
					{/* Enable/Disable toggle */}
					<div class={a.expandedSection}>
						<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
							<input
								type="checkbox"
								checked={isEnabled()}
								onChange={() => settingsStore.toggleAgent(props.agentType)}
							/>
							<span>Enabled</span>
						</label>
					</div>

					{/* Auto-retry on server errors */}
					<div class={a.expandedSection}>
						<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
							<input
								type="checkbox"
								checked={configStore.isAutoRetryEnabled(props.agentType)}
								onChange={() =>
									configStore.setAutoRetry(props.agentType, !configStore.isAutoRetryEnabled(props.agentType))
								}
							/>
							<span>Auto-retry on server errors</span>
						</label>
						<p class={s.hint}>Inject "continue" on 5xx errors with backoff (5s, 15s, 30s)</p>
					</div>

					{/* Native-hook state instrumentation (Claude/Gemini) */}
					<Show when={supportsHooks()}>
						<div class={a.expandedSection}>
							<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
								<input
									type="checkbox"
									checked={configStore.getHookInstrumentation(props.agentType) ?? false}
									disabled={hookLoading()}
									onChange={handleHookToggle}
								/>
								<span>Use native agent hooks for status</span>
								<Show when={hookState() === "installed" || hookState() === "outdated"}>
									<span class={a.badge} data-type={hookState() === "outdated" ? "notfound" : "mcp"}>
										{hookState() === "outdated" ? "Hooks: re-enable" : "Hooks installed"}
									</span>
								</Show>
							</label>
							<p class={s.hint}>
								Drive busy/idle/waiting from {agent().name}'s own hooks instead of output heuristics. TUIC installs and
								removes the hooks cleanly and never touches your own. Applies on next launch.
							</p>
						</div>
					</Show>

					{/* Per-agent TUIC protocol markers — visible when MCP bridge is installed */}
					<Show when={mcpStatus()?.installed}>
						<div class={a.expandedSection}>
							<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
								<input
									type="checkbox"
									checked={configStore.getIntentTabTitle(props.agentType) ?? true}
									onChange={(e) => configStore.setIntentTabTitle(props.agentType, e.currentTarget.checked)}
								/>
								<span>Show intent as tab title</span>
							</label>
							<p class={s.hint}>
								Emit <code>intent:</code> markers to update the tab name with current work phase. Turn off if parsing
								misbehaves on this agent.
							</p>
						</div>

						<div class={a.expandedSection}>
							<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
								<input
									type="checkbox"
									checked={configStore.getSuggestFollowups(props.agentType) ?? settingsStore.state.suggestFollowups}
									onChange={(e) => configStore.setSuggestFollowups(props.agentType, e.currentTarget.checked)}
								/>
								<span>Show suggested follow-ups</span>
							</label>
							<p class={s.hint}>
								Emit <code>suggest:</code> markers for clickable follow-up actions
							</p>
						</div>
					</Show>

					{/* Headless Command Template */}
					<div class={a.expandedSection}>
						<div class={a.expandedLabel}>Headless Command Template</div>
						<input
							class={`${a.formInput} ${a.mono}`}
							placeholder={`${AGENTS[props.agentType].binary} -p "{prompt}" --no-input`}
							value={configStore.getHeadlessTemplate(props.agentType) ?? ""}
							onInput={(e) => configStore.setHeadlessTemplate(props.agentType, e.currentTarget.value)}
							onClick={(e) => e.stopPropagation()}
						/>
						<p class={s.hint}>
							Command template for one-shot execution. Use {"{prompt}"} as placeholder for the prompt text.
						</p>
					</div>

					{/* Run Configurations */}
					<div class={a.expandedSection}>
						<div class={a.expandedLabel}>Run Configurations</div>
						<Show
							when={configs().length > 0}
							fallback={<p class={s.hint}>No custom run configurations. The agent will run with default settings.</p>}
						>
							<div class={a.configList}>
								<For each={configs()}>
									{(config, i) => <RunConfigRow config={config} index={i()} agentType={props.agentType} />}
								</For>
							</div>
						</Show>
						<Show
							when={addingConfig()}
							fallback={
								<button class={a.smallBtn} style={{ "margin-top": "8px" }} onClick={() => setAddingConfig(true)}>
									Add Configuration...
								</button>
							}
						>
							<AddConfigForm agentType={props.agentType} onClose={() => setAddingConfig(false)} />
						</Show>
					</div>

					{/* Actions */}
					<div class={a.expandedSection}>
						<div class={a.expandedLabel}>Actions</div>
						<div class={a.actionsRow}>
							<button class={a.actionBtn} onClick={handleEditConfig}>
								Edit Agent Config
							</button>
							<Show when={supportsMcp() && isTauri()}>
								<button
									class={a.actionBtn}
									classList={{ [a.installed]: mcpStatus()?.installed }}
									onClick={handleMcpToggle}
									disabled={mcpLoading()}
								>
									{mcpLoading() ? "..." : mcpStatus()?.installed ? "Remove TUIC MCP" : "Install TUIC MCP"}
								</button>
								<Show when={mcpStatus()}>
									<span class={a.mcpStatus}>
										<span
											class={a.mcpDot}
											classList={{ [a.on]: mcpStatus()!.installed, [a.off]: !mcpStatus()!.installed }}
										/>
										{mcpStatus()!.installed ? "MCP bridge installed" : "MCP bridge not installed"}
									</span>
								</Show>
							</Show>
						</div>
					</div>

					{/* Claude-specific: Env flags and Usage Dashboard */}
					<Show when={props.agentType === "claude"}>
						<EnvFlagsSection agentType={props.agentType} />
						<ClaudeUsageToggle />
					</Show>
				</div>
			</Show>
		</div>
	);
};

// ---------------------------------------------------------------------------
// AI Prompts section (embedded in Agents tab)
// ---------------------------------------------------------------------------

const AiPromptsSection: Component = () => {
	const [expanded, setExpanded] = createSignal(false);
	const [draft, setDraft] = createSignal("");
	const [dirty, setDirty] = createSignal(false);

	const handleExpand = () => {
		if (!expanded()) {
			aiPromptsStore
				.hydrate()
				.then(() => {
					setDraft(aiPromptsStore.getEffectivePrompt("diff_triage"));
					setDirty(false);
				})
				.catch((err: unknown) => appLogger.error("ai-agent", "Failed to hydrate AI prompts", err));
		}
		setExpanded(!expanded());
	};

	const handleSave = () => {
		const val = draft();
		const isDefault = val.trim() === DEFAULT_DIFF_TRIAGE_PROMPT.trim();
		aiPromptsStore.setDiffTriagePrompt(isDefault ? null : val);
		setDirty(false);
	};

	const handleReset = () => {
		aiPromptsStore.resetToDefault("diff_triage");
		setDraft(DEFAULT_DIFF_TRIAGE_PROMPT);
		setDirty(false);
	};

	return (
		<div class={s.group} style={{ "margin-top": "16px" }}>
			<h3 style={{ cursor: "pointer", display: "flex", "align-items": "center", gap: "6px" }} onClick={handleExpand}>
				<span class={a.expandIcon} classList={{ [a.expanded]: expanded() }}>
					&#9654;
				</span>
				{t("aiPrompts.heading.title", "AI Prompts")}
			</h3>
			<p class={s.hint}>{t("aiPrompts.hint.description", "Customize system prompts sent to AI services.")}</p>

			<Show when={expanded()}>
				<div style={{ "margin-top": "8px" }}>
					<label>{t("aiPrompts.heading.diffTriage", "Diff Triage")}</label>
					<p class={s.hint}>
						{t("aiPrompts.hint.diffTriage", "System prompt sent to the LLM when classifying changed files.")}
					</p>
					<div class={s.group}>
						<textarea
							rows={12}
							value={draft()}
							onInput={(e) => {
								setDraft(e.currentTarget.value);
								setDirty(true);
							}}
						/>
					</div>
					<div class={s.actions}>
						<button disabled={!dirty()} onClick={handleSave}>
							{t("aiPrompts.save", "Save")}
						</button>
						<button disabled={!aiPromptsStore.isCustom("diff_triage") && !dirty()} onClick={handleReset}>
							{t("aiPrompts.resetDefault", "Revert to Default")}
						</button>
					</div>
				</div>
			</Show>
		</div>
	);
};

// ---------------------------------------------------------------------------
// Main tab
// ---------------------------------------------------------------------------

interface AgentsTabProps {
	connectionId?: string;
}

export const AgentsTab: Component<AgentsTabProps> = (props) => {
	const detection = useAgentDetection();
	const [remoteLoading, setRemoteLoading] = createSignal(false);
	const [remoteError, setRemoteError] = createSignal<string | null>(null);
	const [activeStore, setActiveStore] = createSignal(agentConfigsStore);

	createEffect(
		on(
			() => props.connectionId,
			(cid) => {
				const store = cid ? createRemoteAgentConfigStore(cid) : agentConfigsStore;
				setActiveStore(() => store);
				setRemoteLoading(!!cid);
				setRemoteError(null);
				store
					.hydrate()
					.catch((e: Error) => setRemoteError(e.message ?? "Failed to load remote config"))
					.finally(() => setRemoteLoading(false));
			},
		),
	);

	onMount(() => {
		detection.detectAll();
	});

	/** Agents sorted: available first, then not-found — each group alphabetically by display name */
	const sortedAgents = () => {
		const types = ALL_AGENT_TYPES.filter((t) => t !== "api");
		const byName = (a: AgentType, b: AgentType) => AGENTS[a].name.localeCompare(AGENTS[b].name);
		const available = types.filter((t) => detection.isAvailable(t)).sort(byName);
		const unavailable = types.filter((t) => !detection.isAvailable(t)).sort(byName);
		return [...available, ...unavailable];
	};

	return (
		<AgentConfigProvider value={activeStore()}>
			<div class={s.section}>
				<Show when={props.connectionId}>
					{(cid) => {
						const conn = () => remoteConnectionsStore.getConnectionState(cid());
						return (
							<div class={a.remoteBanner}>
								<span class={a.remoteBannerIcon}>&#x27D0;</span>
								Configuring remote: <strong>{conn()?.connection.name ?? cid()}</strong>
							</div>
						);
					}}
				</Show>
				<Show when={remoteLoading()}>
					<div class={a.remoteLoading}>Loading remote configuration...</div>
				</Show>
				<Show when={remoteError()}>
					<div class={a.remoteError}>Remote config unavailable: {remoteError()}</div>
				</Show>

				<h3>Agents</h3>
				<p class={s.hint} style={{ "margin-bottom": "12px" }}>
					Configure AI coding agents, manage run configurations, and install MCP bridge integrations.
				</p>

				<SettingToggle
					checked={settingsStore.state.intentTabTitle}
					onChange={(v) => settingsStore.setIntentTabTitle(v)}
					label="Show agent intent as tab title"
					hint="When agents declare their current work phase, update the tab name with a short title"
				/>

				<SettingToggle
					checked={settingsStore.state.suggestFollowups}
					onChange={(v) => settingsStore.setSuggestFollowups(v)}
					label="Show suggested follow-up actions"
					hint="Display actionable suggestions from agents after completing a task"
				/>

				<div class={a.agentList}>
					<For each={sortedAgents()}>
						{(type) => (
							<AgentRow agentType={type} detection={detection.getDetection(type)} onExpand={detection.detectVersion} />
						)}
					</For>
				</div>

				<AiPromptsSection />
			</div>
		</AgentConfigProvider>
	);
};
