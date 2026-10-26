import {
	type Component,
	createEffect,
	createMemo,
	createResource,
	createSignal,
	createUniqueId,
	For,
	on,
	onCleanup,
	onMount,
	Show,
} from "solid-js";
import {
	AGENT_DISPLAY,
	AGENT_TYPES,
	AGENTS,
	type AgentHookState,
	type AgentRunConfig,
	type AgentType,
	HOOK_SUPPORT,
	LIMITED_SUPPORT_AGENTS,
	LIMITED_SUPPORT_LABEL,
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
import { settingOptions } from "../../../services/settingsCatalogue";
import { agentConfigsStore, ensureAgentConfigs } from "../../../stores/agentConfigs";
import { appLogger } from "../../../stores/appLogger";
import { editorTabsStore } from "../../../stores/editorTabs";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { type ConfigDefaults, settingsExpertStore } from "../../../stores/settingsExpert";
import { isTauri, rpc } from "../../../transport";
import { onClickKeyDown } from "../../../utils/a11y";
import { buildEnvFromEntries, findDuplicateEnvKeys } from "../../../utils/envVars";
import {
	type ArgConflict,
	egoProfileChips,
	findArgConflicts,
	hasBlockingConflict,
	type ProfileChip,
	splitRawArgs,
} from "../../../utils/runConfigArgs";
import { AgentIcon } from "../../ui/AgentIcon";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { ContextualHelp } from "../ContextualHelp";
import { ExpertSetting } from "../ExpertSetting";
import { MachineSelector } from "../MachineSelector";
import { SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../SettingsPrimitives";
import a from "./AgentsTab.module.css";
import {
	AgentConfigProvider,
	createRemoteAgentConfigStore,
	LaunchPreviewProvider,
	MachineProvider,
	useAgentConfig,
	useLaunchPreview,
	useMachine,
} from "./agentConfigContext";
import { LaunchInstructionsEditor } from "./LaunchInstructionsEditor";
import { LaunchModuleSettings } from "./LaunchModuleSettings";

const ALL_AGENT_TYPES = AGENT_TYPES.filter((t): t is AgentType => t !== "git" && t !== "api");

/** Agents with a launch-time instruction channel (Rust `launch_instructions::AGENTS`). */
const LAUNCH_INSTRUCTION_AGENTS: readonly AgentType[] = ["claude", "codex", "grok", "opencode", "goose", "pi", "ego"];

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
	/** Bridge entry lives in the tool's general settings file, so it is never installed automatically. */
	shared_settings_file: boolean;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Key-value row for env var editing */
const EnvVarRow: Component<{
	key: string;
	value: string;
	duplicate?: boolean;
	errorId: string;
	onChange: (key: string, value: string) => void;
	onRemove: () => void;
}> = (props) => (
	<div class={a.envVarRow}>
		<Input
			data-non-setting="Environment variable record key, paired with the catalogue value control"
			class={`${a.formInput} ${a.mono} ${a.envVarKey}`}
			aria-invalid={!!props.duplicate}
			aria-describedby={props.duplicate ? props.errorId : undefined}
			aria-label="KEY"
			placeholder="KEY"
			value={props.key}
			onInput={(e) => props.onChange(e.currentTarget.value, props.value)}
		/>
		<span class={a.envVarEquals}>=</span>
		<Input
			data-setting-id="agents.agents.{key}.run_configs.{name}.env.{key}"
			class={`${a.formInput} ${a.mono} ${a.envVarValue}`}
			aria-label="value"
			placeholder="value"
			value={props.value}
			onInput={(e) => props.onChange(props.key, e.currentTarget.value)}
		/>
		<Button
			data-non-setting="Add or remove a draft environment row; the editor Save commits it"
			variant="danger"
			onClick={props.onRemove}
			title="Remove"
		>
			×
		</Button>
	</div>
);

/** Chips for args the permissions profile adds at launch, then the raw args as an advanced field. */
const RunArgsFields: Component<{
	chips: ProfileChip[];
	conflicts: ArgConflict[];
	value: string;
	onInput: (value: string) => void;
	onKeyDown: (e: KeyboardEvent) => void;
}> = (props) => {
	const argsId = createUniqueId();
	return (
		<>
			<Show when={props.chips.length > 0}>
				<div class={a.formRow}>
					<span class={a.envVarsLabel}>From permissions profile</span>
					<For each={props.chips}>
						{(chip) => (
							<span class={a.argChip} title="Set in the ego permissions section; applied at every launch">
								{chip.flag} {chip.value}
							</span>
						)}
					</For>
				</div>
			</Show>
			<div class={a.formRow}>
				<label class={a.editorFieldLabel} for={argsId}>
					Arguments
				</label>
				<Input
					data-setting-id="agents.agents.{key}.run_configs.{name}.args"
					id={argsId}
					class={`${a.formInput} ${a.mono}`}
					aria-invalid={hasBlockingConflict(props.conflicts)}
					aria-describedby={props.conflicts.length ? `${argsId}-errors` : undefined}
					aria-label="Raw arguments (advanced)"
					placeholder="Raw arguments (advanced, space-separated)"
					value={props.value}
					onInput={(e) => props.onInput(e.currentTarget.value)}
					onKeyDown={props.onKeyDown}
				/>
			</div>
			<div id={`${argsId}-errors`}>
				<For each={props.conflicts}>
					{(c) => (
						<div
							classList={{
								[a.validationError]: c.kind === "duplicate",
								[a.validationWarning]: c.kind === "overridden",
							}}
						>
							{c.message}
						</div>
					)}
				</For>
			</div>
		</>
	);
};

/** Inline form for adding a new run config */
const AddConfigForm: Component<{
	agentType: AgentType;
	onClose: () => void;
}> = (props) => {
	const configStore = useAgentConfig();
	const editorId = createUniqueId();
	const [name, setName] = createSignal("");
	const [command, setCommand] = createSignal(AGENTS[props.agentType].binary);
	const [args, setArgs] = createSignal("");
	const [model, setModel] = createSignal("");
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
	const egoProfile = () => configStore.state.agents.ego;
	const argConflicts = createMemo(() => findArgConflicts(props.agentType, splitRawArgs(args()), egoProfile()));

	const handleSave = async () => {
		const n = name().trim();
		if (!n || isDuplicate() || duplicateEnvKeys().length > 0 || hasBlockingConflict(argConflicts())) return;
		const config: AgentRunConfig = {
			name: n,
			command: command().trim() || AGENTS[props.agentType].binary,
			args: splitRawArgs(args()),
			model: model().trim() || undefined,
			env: buildEnvFromEntries(envVars()),
			is_default: false,
		};
		await configStore.addRunConfig(props.agentType, config);
		props.onClose();
	};

	return (
		<div class={a.addConfigForm} role="group" aria-label="New run configuration">
			<strong class={a.editorTitle}>New run configuration</strong>
			<div class={a.formRow}>
				<label class={a.editorFieldLabel} for={`new-${editorId}-name`}>
					Configuration name
				</label>
				<Input
					data-setting-id="agents.agents.{key}.run_configs.{name}.name"
					class={a.formInput}
					aria-invalid={isDuplicate()}
					aria-describedby={isDuplicate() ? `new-${editorId}-name-error` : undefined}
					aria-label="Configuration name"
					id={`new-${editorId}-name`}
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
				<div id={`new-${editorId}-name-error`} class={a.validationError}>
					Name "{name().trim()}" already exists
				</div>
			</Show>
			<div class={a.formRow}>
				<label class={a.editorFieldLabel} for={`new-${editorId}-command`}>
					Command (binary)
				</label>
				<Input
					data-setting-id="agents.agents.{key}.run_configs.{name}.command"
					class={`${a.formInput} ${a.mono}`}
					aria-label="Command (binary)"
					id={`new-${editorId}-command`}
					placeholder="Command (binary)"
					value={command()}
					onInput={(e) => setCommand(e.currentTarget.value)}
				/>
			</div>
			<RunArgsFields
				chips={props.agentType === "ego" ? egoProfileChips(egoProfile()) : []}
				conflicts={argConflicts()}
				value={args()}
				onInput={setArgs}
				onKeyDown={(e) => {
					if (e.key === "Enter") handleSave();
					if (e.key === "Escape") props.onClose();
				}}
			/>
			<div class={a.formRow}>
				<label class={a.editorFieldLabel} for={`new-${editorId}-model`}>
					Model (optional)
				</label>
				<Input
					data-setting-id="agents.agents.{key}.run_configs.{name}.model"
					class={`${a.formInput} ${a.mono}`}
					aria-label="Model"
					id={`new-${editorId}-model`}
					placeholder="Model (optional)"
					value={model()}
					onInput={(e) => setModel(e.currentTarget.value)}
				/>
			</div>
			{/* Env vars section */}
			<div class={a.envVarsSection}>
				<div class={a.envVarsHeader}>
					<span class={a.envVarsLabel}>Environment Variables</span>
					<Button
						data-non-setting="Add or remove a draft environment row; the editor Save commits it"
						onClick={addEnvVar}
						type="button"
					>
						+ Add
					</Button>
				</div>
				<Show when={envVars().length > 0}>
					<div class={a.envVarsList}>
						<For each={envVars()}>
							{(v, i) => (
								<EnvVarRow
									errorId={`new-${editorId}-env-error`}
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
					<div id={`new-${editorId}-env-error`} class={a.validationError}>
						Duplicate env keys: {duplicateEnvKeys().join(", ")}
					</div>
				</Show>
			</div>
			<div class={a.formRow}>
				<Button
					data-non-setting="Commit the existing run-configuration editor draft through its owner"
					onClick={handleSave}
					disabled={
						isDuplicate() || !name().trim() || duplicateEnvKeys().length > 0 || hasBlockingConflict(argConflicts())
					}
				>
					Save
				</Button>
				<Button data-non-setting="Open, close or cancel the run-configuration editor" onClick={props.onClose}>
					Cancel
				</Button>
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
	const editorId = createUniqueId();
	const [editingEnv, setEditingEnv] = createSignal(false);
	const [editingConfig, setEditingConfig] = createSignal(false);
	const [menuOpen, setMenuOpen] = createSignal(false);
	const [envVars, setEnvVars] = createSignal<Array<{ key: string; value: string }>>([]);
	const [editName, setEditName] = createSignal("");
	const [editCommand, setEditCommand] = createSignal("");
	const [editArgs, setEditArgs] = createSignal("");
	const [editModel, setEditModel] = createSignal("");
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
		setEditModel(props.config.model ?? "");
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

	const egoProfile = () => configStore.state.agents.ego;
	const profileChips = () => (props.agentType === "ego" ? egoProfileChips(egoProfile()) : []);
	const editConflicts = createMemo(() => findArgConflicts(props.agentType, splitRawArgs(editArgs()), egoProfile()));
	// Already-saved args may predate the profile; the row warns before launch.
	const savedConflicts = createMemo(() => findArgConflicts(props.agentType, props.config.args, egoProfile()));

	const saveConfig = async () => {
		const n = editName().trim();
		if (!n || isDuplicateName() || hasBlockingConflict(editConflicts())) return;
		const updated: AgentRunConfig = {
			...props.config,
			name: n,
			command: editCommand().trim() || props.config.command,
			args: splitRawArgs(editArgs()),
			model: editModel().trim() || undefined,
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
				<Show
					when={props.config.args
						.slice(0, props.config.args.indexOf("--") < 0 ? undefined : props.config.args.indexOf("--"))
						.includes("--dangerously-bypass-approvals-and-sandbox")}
				>
					<span
						class={a.envBadge}
						title="Approvals and sandbox are bypassed. Remove the flag in Edit configuration to disable bypass."
						aria-label="Approvals and sandbox bypassed"
					>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
							<path d="M12 2 1 22h22L12 2zm-1 7h2v6h-2V9zm0 8h2v2h-2v-2z" />
						</svg>
					</span>
				</Show>
				<Show when={savedConflicts().length > 0}>
					<span
						class={a.envBadge}
						title={savedConflicts()
							.map((c) => c.message)
							.join("\n")}
						aria-label="Conflicting arguments"
					>
						args conflict
					</span>
				</Show>
				<Show when={envCount() > 0}>
					<span class={a.envBadge}>{envCount()} env</span>
				</Show>
				<Show when={props.config.is_default}>
					<span class={a.defaultBadge}>Default</span>
				</Show>
				<div class={a.configActions}>
					<Button
						data-non-setting="Open, close or cancel the run-configuration editor"
						onClick={startEnvEdit}
						title="Edit environment variables"
					>
						Env
					</Button>
					<Show when={!props.config.is_default}>
						<Button
							data-setting-id="agents.agents.{key}.run_configs.{name}.is_default"
							onClick={() => configStore.setDefaultConfig(props.agentType, props.index)}
							title="Set as default"
						>
							Set Default
						</Button>
					</Show>
					<div class={a.menuWrap} ref={menuRef}>
						<Button
							data-non-setting="Open, close or cancel the run-configuration editor"
							onClick={() => setMenuOpen(!menuOpen())}
							title="More actions"
						>
							···
						</Button>
						<Show when={menuOpen()}>
							<div class={a.menuDropdown}>
								<Button
									data-non-setting="Open, close or cancel the run-configuration editor"
									class={a.menuItem}
									onClick={startConfigEdit}
								>
									Edit
								</Button>
								<Button
									data-non-setting="Remove the stable run-configuration record through its owner"
									class={a.menuItem}
									variant="danger"
									onClick={handleDelete}
								>
									Delete
								</Button>
							</div>
						</Show>
					</div>
				</div>
			</div>
			<Show when={editingConfig()}>
				<div class={a.envEditPanel}>
					<div class={a.envVarsHeader}>
						<span class={a.envVarsLabel}>Edit Configuration · {props.config.name}</span>
					</div>
					<div class={a.formRow}>
						<label class={a.editorFieldLabel} for={`edit-${editorId}-name`}>
							Configuration name
						</label>
						<Input
							data-setting-id="agents.agents.{key}.run_configs.{name}.name"
							class={a.formInput}
							aria-invalid={isDuplicateName()}
							aria-describedby={isDuplicateName() ? `edit-${editorId}-name-error` : undefined}
							aria-label="Configuration name"
							id={`edit-${editorId}-name`}
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
						<div id={`edit-${editorId}-name-error`} class={a.validationError}>
							Name "{editName().trim()}" already exists
						</div>
					</Show>
					<div class={a.formRow}>
						<label class={a.editorFieldLabel} for={`edit-${editorId}-command`}>
							Command (binary)
						</label>
						<Input
							data-setting-id="agents.agents.{key}.run_configs.{name}.command"
							class={`${a.formInput} ${a.mono}`}
							aria-label="Command (binary)"
							id={`edit-${editorId}-command`}
							placeholder="Command (binary)"
							value={editCommand()}
							onInput={(e) => setEditCommand(e.currentTarget.value)}
						/>
					</div>
					<RunArgsFields
						chips={profileChips()}
						conflicts={editConflicts()}
						value={editArgs()}
						onInput={setEditArgs}
						onKeyDown={(e) => {
							if (e.key === "Enter") saveConfig();
							if (e.key === "Escape") setEditingConfig(false);
						}}
					/>
					<div class={a.formRow}>
						<label class={a.editorFieldLabel} for={`edit-${editorId}-model`}>
							Model (optional)
						</label>
						<Input
							data-setting-id="agents.agents.{key}.run_configs.{name}.model"
							class={`${a.formInput} ${a.mono}`}
							aria-label="Model"
							id={`edit-${editorId}-model`}
							placeholder="Model (optional)"
							value={editModel()}
							onInput={(e) => setEditModel(e.currentTarget.value)}
						/>
					</div>
					<div class={a.formRow}>
						<Button
							data-non-setting="Commit the existing run-configuration editor draft through its owner"
							onClick={saveConfig}
							disabled={isDuplicateName() || !editName().trim() || hasBlockingConflict(editConflicts())}
						>
							Save
						</Button>
						<Button
							data-non-setting="Open, close or cancel the run-configuration editor"
							onClick={() => setEditingConfig(false)}
						>
							Cancel
						</Button>
					</div>
				</div>
			</Show>
			<Show when={editingEnv()}>
				<div class={a.envEditPanel}>
					<div class={a.envVarsHeader}>
						<span class={a.envVarsLabel}>Environment Variables</span>
						<Button
							data-non-setting="Add or remove a draft environment row; the editor Save commits it"
							onClick={addEnvVar}
							type="button"
						>
							+ Add
						</Button>
					</div>
					<div class={a.envVarsList}>
						<For each={envVars()}>
							{(v, i) => (
								<EnvVarRow
									errorId={`edit-${editorId}-env-error`}
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
						<div id={`edit-${editorId}-env-error`} class={a.validationError}>
							Duplicate env keys: {duplicateEnvKeys().join(", ")}
						</div>
					</Show>
					<div class={a.formRow}>
						<Button
							data-non-setting="Commit the existing run-configuration editor draft through its owner"
							onClick={saveEnv}
							disabled={duplicateEnvKeys().length > 0}
						>
							Save
						</Button>
						<Button
							data-non-setting="Open, close or cancel the run-configuration editor"
							onClick={() => setEditingEnv(false)}
						>
							Cancel
						</Button>
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
														<Switch
															role="checkbox"
															data-setting-id="agents.agents.{key}.env_flags.{key}"
															class={a.envFlagToggle}
															checked={isFlagEnabled(flag.key)}
															onChange={() => handleBoolToggle(flag)}
														/>
													</Show>
													<Show when={flag.type === "enum"}>
														<Select
															data-setting-id="agents.agents.{key}.env_flags.{key}"
															compact
															class={a.envFlagSelect}
															value={getFlagValue(flag.key)}
															onChange={(e) => handleValueChange(flag.key, e.currentTarget.value)}
														>
															<option value="">off</option>
															<For each={flag.options ?? []}>{(opt) => <option value={opt}>{opt}</option>}</For>
														</Select>
													</Show>
													<Show when={flag.type === "number"}>
														<Input
															data-setting-id="agents.agents.{key}.env_flags.{key}"
															compact
															type="number"
															class={a.envFlagInput}
															value={getFlagValue(flag.key)}
															placeholder={flag.defaultValue ?? ""}
															onInput={(e) => handleValueChange(flag.key, e.currentTarget.value)}
														/>
													</Show>
													<Show when={flag.type === "string"}>
														<Input
															data-setting-id="agents.agents.{key}.env_flags.{key}"
															compact
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
					<Switch
						role="checkbox"
						data-setting-id="app.disabled_plugin_ids"
						checked={enabled()}
						onChange={handleToggle}
					/>
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
	const launchPreview = useLaunchPreview();
	const machine = useMachine();
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
	const supportsLaunchSignals = () => props.agentType === "claude" || props.agentType === "codex";
	const supportsGlobalHooks = () => ["gemini", "grok", "opencode"].includes(props.agentType);

	/**
	 * The MCP bridge is installed on this machine only.
	 *
	 * `get_agent_mcp_status`, `install_agent_mcp`, `remove_agent_mcp` and
	 * `get_agent_config_path` have no HTTP route (`INTENTIONALLY_UNMAPPED`), so
	 * they cannot be sent to a daemon. Showing them while a remote machine is
	 * selected would report the Mac's bridge as the remote machine's.
	 */
	const mcpEditable = () => supportsMcp() && isTauri() && !machine();

	const loadMcpStatus = async () => {
		if (!mcpEditable()) return;
		try {
			const status = await invoke<McpStatus>("get_agent_mcp_status", { agentType: props.agentType });
			setMcpStatus(status);
		} catch (err) {
			appLogger.error("config", `Failed to get MCP status for ${props.agentType}`, err);
		}
	};

	const loadHookState = async () => {
		if (!supportsHooks() || (!isTauri() && !machine())) return;
		try {
			const st = await rpc<AgentHookState>("get_agent_hook_state", { agentType: props.agentType }, machine());
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
			await rpc("set_agent_hook_instrumentation", { agentType: props.agentType, enabled: next }, machine());
			configStore.syncHookInstrumentation(props.agentType, next);
			await loadHookState();
		} catch (err) {
			appLogger.error("config", `Hook instrumentation toggle failed for ${props.agentType}`, err);
		} finally {
			setHookLoading(false);
		}
	};

	const handleNativeStatusToggle = async () => {
		if (hookLoading()) return;
		setHookLoading(true);
		try {
			const next = !configStore.getNativeStatusSignals(props.agentType);
			await rpc("set_agent_native_status_signals", { agentType: props.agentType, enabled: next }, machine());
			configStore.syncNativeStatusSignals(props.agentType, next);
		} catch (err) {
			appLogger.error("config", `Native status signal toggle failed for ${props.agentType}`, err);
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
	/** The stored env flags in the serialized shape: an empty map is not written. */
	const envFlagsOverride = () => {
		const flags = configStore.getEnvFlags(props.agentType);
		return Object.keys(flags).length > 0 ? flags : null;
	};

	return (
		<div class={a.agentRow}>
			<div
				class={a.agentHeader}
				role="button"
				tabIndex={0}
				onClick={handleExpand}
				onKeyDown={onClickKeyDown(handleExpand)}
			>
				<div class={a.agentInfo}>
					<div class={a.agentNameRow}>
						<div
							class={a.agentIcon}
							style={{ background: display().color, color: display().foreground, opacity: isEnabled() ? 1 : 0.4 }}
						>
							<AgentIcon agent={props.agentType} size={16} />
						</div>
						<span class={a.agentName} style={{ opacity: isEnabled() ? 1 : 0.5 }}>
							{agent().name}
						</span>
						<Show when={LIMITED_SUPPORT_AGENTS.includes(props.agentType)}>
							<span class={a.badge} data-type="limited">
								{LIMITED_SUPPORT_LABEL}
							</span>
						</Show>
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
					<ExpertSetting
						configKey="agent_settings.idle_close_minutes"
						value={configStore.getIdleCloseMinutes(props.agentType)}
					>
						<SettingRow label="Close idle managed child after" hint="Set to 0 to keep managed children open.">
							{(id, descriptionId) => (
								<div class={a.minutesControl}>
									<Input
										data-setting-id="agents.agents.{key}.idle_close_minutes"
										id={id}
										aria-describedby={descriptionId()}
										class={a.formInput}
										type="number"
										min="0"
										max="1440"
										step="1"
										value={configStore.getIdleCloseMinutes(props.agentType)}
										onChange={(event) => {
											const minutes = event.currentTarget.valueAsNumber;
											if (Number.isInteger(minutes) && minutes >= 0 && minutes <= 1440) {
												void configStore.setIdleCloseMinutes(props.agentType, minutes);
											}
										}}
									/>
									<span>min</span>
								</div>
							)}
						</SettingRow>
					</ExpertSetting>
					{/* Enable/Disable toggle */}
					<SettingRow label="Enabled">
						{(id, descriptionId) => (
							<Switch
								data-setting-id="app.disabled_agents"
								id={id}
								aria-describedby={descriptionId()}
								role="switch"
								checked={isEnabled()}
								onChange={() => settingsStore.toggleAgent(props.agentType)}
							/>
						)}
					</SettingRow>

					{/* Auto-retry on server errors */}
					<ExpertSetting
						configKey="agent_settings.auto_retry_on_error"
						value={configStore.isAutoRetryEnabled(props.agentType)}
					>
						<SettingRow
							label="Auto-retry on server errors"
							hint="Inject &quot;continue&quot; on 5xx errors with backoff (5s, 15s, 30s)"
						>
							{(id, descriptionId) => (
								<Switch
									data-setting-id="agents.agents.{key}.auto_retry_on_error"
									id={id}
									aria-describedby={descriptionId()}
									role="switch"
									checked={configStore.isAutoRetryEnabled(props.agentType)}
									onChange={() =>
										configStore.setAutoRetry(props.agentType, !configStore.isAutoRetryEnabled(props.agentType))
									}
								/>
							)}
						</SettingRow>
					</ExpertSetting>

					<ExpertSetting
						configKey="agent_settings.prevent_alt_screen"
						value={configStore.state.agents[props.agentType]?.prevent_alt_screen ?? null}
					>
						<SettingRow
							label="Prevent alternate screen"
							hint="Use native terminal scrollback when this agent supports it. Applies to new launches."
						>
							{(id, descriptionId) => (
								<Switch
									data-setting-id="agents.agents.{key}.prevent_alt_screen"
									id={id}
									aria-describedby={descriptionId()}
									role="switch"
									checked={configStore.getPreventAltScreen(props.agentType)}
									onChange={() =>
										configStore.setPreventAltScreen(props.agentType, !configStore.getPreventAltScreen(props.agentType))
									}
								/>
							)}
						</SettingRow>
					</ExpertSetting>

					<Show when={props.agentType === "claude" || props.agentType === "codex"}>
						<ExpertSetting
							configKey="agent_settings.skip_trust_dialog"
							value={configStore.state.agents[props.agentType]?.skip_trust_dialog ?? null}
						>
							<SettingRow
								label="Accept workspace trust for managed spawns"
								hint="Start agents in a new folder without stopping at their workspace trust question. Applies only to agent-to-agent spawns."
							>
								{(id, descriptionId) => (
									<Switch
										data-setting-id="agents.agents.{key}.skip_trust_dialog"
										id={id}
										aria-describedby={descriptionId()}
										role="switch"
										checked={configStore.getSkipTrustDialog(props.agentType)}
										onChange={() =>
											configStore.setSkipTrustDialog(props.agentType, !configStore.getSkipTrustDialog(props.agentType))
										}
									/>
								)}
							</SettingRow>
						</ExpertSetting>
					</Show>

					<Show when={supportsLaunchSignals()}>
						<ExpertSetting
							configKey="agent_settings.native_status_signals"
							value={configStore.state.agents[props.agentType]?.native_status_signals ?? null}
						>
							<SettingRow
								label="Native status signals"
								hint="Adds process-scoped status integration only to agents launched inside TUIC. Applies on next launch."
							>
								{(id, descriptionId) => (
									<Switch
										data-setting-id="agents.agents.{key}.native_status_signals"
										id={id}
										aria-describedby={descriptionId()}
										role="switch"
										checked={configStore.getNativeStatusSignals(props.agentType)}
										disabled={hookLoading()}
										onChange={handleNativeStatusToggle}
									/>
								)}
							</SettingRow>
						</ExpertSetting>
					</Show>

					{/* Explicit global installation remains available only where launch-scoped integration is unavailable. */}
					<Show when={supportsHooks() && supportsGlobalHooks()}>
						<ExpertSetting
							configKey="agent_settings.hook_instrumentation"
							value={configStore.state.agents[props.agentType]?.hook_instrumentation ?? null}
						>
							<div class={a.expandedSection}>
								<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
									<Switch
										role="checkbox"
										data-setting-id="agents.agents.{key}.hook_instrumentation"
										checked={configStore.getHookInstrumentation(props.agentType) ?? false}
										disabled={hookLoading()}
										onChange={handleHookToggle}
									/>
									<span>Install hooks globally</span>
									<Show when={hookState() === "installed" || hookState() === "outdated"}>
										<span class={a.badge} data-type={hookState() === "outdated" ? "notfound" : "mcp"}>
											{hookState() === "outdated" ? "Hooks: re-enable" : "Hooks installed"}
										</span>
									</Show>
								</label>
								<p class={s.hint}>
									Drive busy/idle/waiting from {agent().name}'s own hooks instead of output heuristics. TUIC installs
									and removes the hooks cleanly and never touches your own. Applies on next launch.
								</p>
							</div>
						</ExpertSetting>
					</Show>

					{/* Per-agent TUIC protocol markers — visible when MCP bridge is installed */}
					<Show when={mcpStatus()?.installed}>
						<ExpertSetting
							configKey="agent_settings.intent_tab_title"
							value={configStore.state.agents[props.agentType]?.intent_tab_title ?? null}
						>
							<div class={a.expandedSection}>
								<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
									<Switch
										role="checkbox"
										data-setting-id="agents.agents.{key}.intent_tab_title"
										checked={configStore.getIntentTabTitle(props.agentType) ?? true}
										onChange={(e) => configStore.setIntentTabTitle(props.agentType, e.currentTarget.checked)}
									/>
									<span>Track agent intent</span>
								</label>
								<p class={s.hint}>
									Ask the model to emit <code>intent:</code> markers at task start and phase changes. The current intent
									appears in the terminal Context bar and may update the tab name. Turn off if parsing misbehaves on
									this agent.
								</p>
							</div>
						</ExpertSetting>

						<ExpertSetting
							configKey="agent_settings.progress_tracking"
							value={configStore.state.agents[props.agentType]?.progress_tracking ?? null}
						>
							<div class={a.expandedSection}>
								<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
									<Switch
										role="checkbox"
										data-setting-id="agents.agents.{key}.progress_tracking"
										disabled={!settingsStore.state.progressTracking}
										checked={
											settingsStore.state.progressTracking && (configStore.getProgressTracking(props.agentType) ?? true)
										}
										onChange={(e) => configStore.setProgressTracking(props.agentType, e.currentTarget.checked)}
									/>
									<span>Collect progress</span>
								</label>
								<p class={s.hint}>
									Let {agent().name} report <code>done</code> and <code>blocked</code> entries into the project journal,
									and record its <code>intent:</code> markers there. Off here keeps the journal clear of this agent even
									while collection is on everywhere else; off globally wins over both.
								</p>
							</div>
						</ExpertSetting>

						<ExpertSetting
							configKey="agent_settings.suggest_followups"
							value={configStore.state.agents[props.agentType]?.suggest_followups ?? null}
						>
							<div class={a.expandedSection}>
								<label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
									<Switch
										role="checkbox"
										data-setting-id="agents.agents.{key}.suggest_followups"
										checked={configStore.getSuggestFollowups(props.agentType) ?? settingsStore.state.suggestFollowups}
										onChange={(e) => configStore.setSuggestFollowups(props.agentType, e.currentTarget.checked)}
									/>
									<span>Show suggested follow-ups</span>
								</label>
								<p class={s.hint}>
									Emit <code>suggest:</code> markers for clickable follow-up actions
								</p>
							</div>
						</ExpertSetting>
					</Show>

					<Show when={LAUNCH_INSTRUCTION_AGENTS.includes(props.agentType) && settingsExpertStore.launchInstructions()}>
						{(defaults) => (
							<>
								<LaunchModuleSettings agentType={props.agentType} />
								<LaunchInstructionsEditor
									settingId="agents.agents.{key}.launch_instructions"
									id={`launch-instructions-${props.agentType}`}
									label={`Launch instructions for ${agent().name}`}
									value={configStore.getAgentLaunchInstructions(props.agentType)}
									fallback={launchPreview()?.effective_text[props.agentType]}
									inheritanceLabel="Generated modules and global additions"
									customizeLabel={`Add instructions for ${agent().name}`}
									resetLabel="Clear agent additions"
									requiredMarkers={defaults().required_markers}
									onSave={(text) => void configStore.setAgentLaunchInstructions(props.agentType, text)}
								/>
							</>
						)}
					</Show>

					{/* Headless Command Template — compared by the stored override, not
					    the built-in template the input falls back to. */}
					<ExpertSetting
						configKey="agent_settings.headless_template"
						value={configStore.state.agents[props.agentType]?.headless_template ?? null}
					>
						<div class={a.expandedSection}>
							<div class={a.expandedLabel}>Headless Command Template</div>
							<Input
								data-setting-id="agents.agents.{key}.headless_template"
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
					</ExpertSetting>

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
								<Button
									data-non-setting="Open, close or cancel the run-configuration editor"
									style={{ "margin-top": "8px" }}
									onClick={() => setAddingConfig(true)}
								>
									Add Configuration...
								</Button>
							}
						>
							<AddConfigForm agentType={props.agentType} onClose={() => setAddingConfig(false)} />
						</Show>
					</div>

					{/* Actions */}
					<div class={a.expandedSection}>
						<div class={a.expandedLabel}>Actions</div>
						<div class={a.actionsRow}>
							{/* Opens the file in this app's editor, so it can only be a local path. */}
							<Show when={!machine()}>
								<Button data-non-setting="Open the agent-owned configuration editor" onClick={handleEditConfig}>
									Edit Agent Config
								</Button>
							</Show>
							<Show when={mcpEditable()}>
								<Button
									data-non-setting="Explicit external-agent MCP installation workflow; not TUIC configuration"
									variant="removalToggle"
									aria-pressed={!!mcpStatus()?.installed}
									onClick={handleMcpToggle}
									disabled={mcpLoading()}
								>
									{mcpLoading() ? "..." : mcpStatus()?.installed ? "Remove TUIC MCP" : "Install TUIC MCP"}
								</Button>
								<Show when={mcpStatus()}>
									<span class={a.mcpStatus}>
										<span
											class={a.mcpDot}
											classList={{ [a.on]: mcpStatus()!.installed, [a.off]: !mcpStatus()!.installed }}
										/>
										{mcpStatus()!.installed ? "MCP bridge installed" : "MCP bridge not installed"}
									</span>
								</Show>
								<Show when={mcpStatus() && !mcpStatus()!.installed && mcpStatus()!.shared_settings_file}>
									<span class={a.mcpStatus}>
										Not installed automatically — the entry goes in this tool's own settings file.
									</span>
								</Show>
							</Show>
						</div>
					</div>

					{/* Claude-specific: Env flags and Usage Dashboard */}
					<Show when={props.agentType === "claude"}>
						<ExpertSetting configKey="agent_settings.env_flags" value={envFlagsOverride()}>
							<EnvFlagsSection agentType={props.agentType} />
						</ExpertSetting>
						<ClaudeUsageToggle />
					</Show>
				</div>
			</Show>
		</div>
	);
};

// ---------------------------------------------------------------------------
// MCP integrations cleanup (embedded in Agents tab)
// ---------------------------------------------------------------------------

/**
 * One place to see — and undo — every MCP bridge entry TUICommander wrote.
 *
 * Without it, uninstalling TUIC leaves a dangling `tuic-bridge` entry in each
 * client it ever configured, and the user has to know which ones those were to
 * clean up (issue #115).
 */
const McpIntegrationsSection: Component = () => {
	const [installed, setInstalled] = createSignal<string[]>([]);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const refresh = async () => {
		if (!isTauri()) return;
		try {
			setInstalled(await invoke<string[]>("list_installed_mcp_integrations"));
		} catch (err) {
			appLogger.error("config", "Failed to list MCP integrations", err);
		}
	};

	onMount(refresh);

	const handleRemoveAll = async () => {
		if (busy()) return;
		setBusy(true);
		setError(null);
		try {
			await invoke<string[]>("remove_all_mcp_integrations");
		} catch (err) {
			// The sweep removes what it can and reports the rest, so refresh
			// regardless — some entries are gone even on a partial failure.
			setError(String(err));
			appLogger.error("config", "Failed to remove MCP integrations", err);
		} finally {
			await refresh();
			setBusy(false);
		}
	};

	return (
		<Show when={isTauri() && installed().length > 0}>
			<div class={a.cleanupCard}>
				<div class={a.expandedLabel}>MCP integrations · This machine</div>
				<p class={s.hint}>Remove the bridge entries that TUICommander installed in local agent clients.</p>
				<p class={s.hint} style={{ "margin-bottom": "8px" }}>
					The TUICommander bridge is configured in: <strong>{installed().join(", ")}</strong>. Remove them before
					uninstalling TUICommander, or each client will report a missing MCP server.
				</p>
				<div class={a.actionsRow}>
					<Button
						data-non-setting="Explicit external-agent MCP installation workflow; not TUIC configuration"
						onClick={handleRemoveAll}
						disabled={busy()}
					>
						{busy() ? "Removing..." : "Remove all MCP integrations"}
					</Button>
				</div>
				<Show when={error()}>
					<p class={a.remoteError}>{error()}</p>
				</Show>
			</div>
		</Show>
	);
};

// ---------------------------------------------------------------------------
// AI Prompts section (embedded in Agents tab)
// ---------------------------------------------------------------------------

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
	/**
	 * `undefined` until the user picks: the tab then follows the repository the
	 * settings nav is standing on, which is the common case. An explicit pick
	 * wins, so a remote machine stays editable from a local repo.
	 */
	const [pickedMachine, setPickedMachine] = createSignal<string | undefined>(undefined);
	const [machinePicked, setMachinePicked] = createSignal(false);
	const machine = () => (machinePicked() ? pickedMachine() : props.connectionId);

	createEffect(
		on(machine, (cid) => {
			const store = cid ? createRemoteAgentConfigStore(cid) : agentConfigsStore;
			setActiveStore(() => store);
			setRemoteLoading(!!cid);
			setRemoteError(null);
			ensureAgentConfigs(cid)
				.catch((e: Error) => setRemoteError(e.message ?? "Failed to load remote config"))
				.finally(() => setRemoteLoading(false));
		}),
	);

	// Pass the current edits, not a delayed disk snapshot: app saves are debounced.
	// Remote machines keep their own global marker preferences on the backend.
	const [preview] = createResource(
		() =>
			activeStore().state.loaded
				? {
						connectionId: machine(),
						preview: {
							agents: {
								agents: JSON.parse(JSON.stringify(activeStore().state.agents)),
								launch_instructions: activeStore().getLaunchInstructions(),
								launch_modules: JSON.parse(JSON.stringify(activeStore().state.launch_modules)),
								launch_instructions_disabled: activeStore().state.launch_instructions_disabled,
								launch_additions_migrated: true,
							},
							intent_tab_title: machine() ? undefined : settingsStore.state.intentTabTitle,
							suggest_followups: machine() ? undefined : settingsStore.state.suggestFollowups,
						},
					}
				: false,
		async ({ connectionId, preview }) => {
			try {
				const defaults = connectionId
					? await rpc<ConfigDefaults>("get_config_defaults", { preview }, connectionId)
					: await invoke<ConfigDefaults>("get_config_defaults", { preview });
				return { connectionId, instructions: defaults.launch_instructions };
			} catch (err) {
				appLogger.error("config", "Failed to load launch instruction previews", err);
				return undefined;
			}
		},
	);
	const launchPreview = () => {
		const snapshot = preview();
		return activeStore().state.loaded && !preview.loading && snapshot?.connectionId === machine()
			? snapshot?.instructions
			: undefined;
	};

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
		<LaunchPreviewProvider value={launchPreview}>
			<AgentConfigProvider value={activeStore()}>
				<MachineProvider value={machine}>
					<div class={`${s.section} ${a.page}`}>
						<SettingPageHeader
							title="Terminal agents"
							scope={
								machine()
									? `Machine: ${remoteConnectionsStore.getConnectionState(machine()!)?.connection.name ?? machine()}`
									: "This machine"
							}
							breadcrumb="Agents / Terminal agents"
						/>
						<div class={a.machineScope}>
							<MachineSelector
								value={machine()}
								label="Configure agents on"
								onChange={(id) => {
									setMachinePicked(true);
									setPickedMachine(id);
								}}
							/>
							<Show when={machine()}>
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
						</div>
						<SectionCard title="Agent behavior">
							<SettingToggle
								settingId="app.intent_tab_title"
								row
								checked={settingsStore.state.intentTabTitle}
								onChange={(v) => settingsStore.setIntentTabTitle(v)}
								label="Show agent intent as tab title"
								hint="When agents declare their current work phase, update the tab name with a short title"
							/>

							<SettingToggle
								settingId="app.suggest_followups"
								row
								checked={settingsStore.state.suggestFollowups}
								onChange={(v) => settingsStore.setSuggestFollowups(v)}
								label="Show suggested follow-up actions"
								hint="Display actionable suggestions from agents after completing a task"
							/>

							<ExpertSetting configKey="app.progress_tracking" value={settingsStore.state.progressTracking}>
								<SettingToggle
									settingId="app.progress_tracking"
									row
									checked={settingsStore.state.progressTracking}
									onChange={(v) => settingsStore.setProgressTracking(v)}
									label="Collect project progress"
									hint="Keep a per-project journal of what agents finished, what blocked them, and what they set out to do. Off removes the progress tool from every agent."
								/>
							</ExpertSetting>

							<Show when={settingsExpertStore.launchInstructions()}>
								{(defaults) => (
									<>
										<LaunchModuleSettings />
										<LaunchInstructionsEditor
											settingId="agents.launch_instructions"
											id="launch-instructions-global"
											label="Launch instructions"
											value={activeStore().getLaunchInstructions()}
											fallback={launchPreview()?.default_text}
											inheritanceLabel="Generated modules"
											customizeLabel="Add instructions"
											resetLabel="Clear global additions"
											requiredMarkers={defaults().required_markers}
											onSave={(text) => void activeStore().setLaunchInstructions(text)}
										/>
									</>
								)}
							</Show>

							<div class={a.agentList}>
								<For each={sortedAgents()}>
									{(type) => (
										<AgentRow
											agentType={type}
											detection={detection.getDetection(type)}
											onExpand={detection.detectVersion}
										/>
									)}
								</For>
							</div>
						</SectionCard>
						<HibernationSection />
						<EgoPermissionsSection />

						<McpIntegrationsSection />
					</div>
				</MachineProvider>
			</AgentConfigProvider>
		</LaunchPreviewProvider>
	);
};

const EgoPermissionsSection: Component = () => {
	const configStore = useAgentConfig();
	return (
		<SectionCard title="ego permissions" description="Permissions for new terminal sessions.">
			<div class={a.permissionRows}>
				<div class={s.helpLabel}>
					<label for="ego-permission-mode">Permissions</label>
					<ContextualHelp label="Permissions" />
				</div>
				<Select
					data-setting-id="agents.agents.{key}.ego_mode"
					id="ego-permission-mode"
					class={a.formInput}
					value={configStore.state.agents.ego?.ego_mode ?? ""}
					onChange={(event) =>
						void configStore.setEgoPermissions({
							ego_mode: (event.currentTarget.value as import("../../../agents").EgoPermissionMode) || undefined,
						})
					}
				>
					<For
						each={settingOptions("agents.agents.{key}.ego_mode", {
							"": "Use ego configuration",
							plan: "Read-only (plan)",
							default: "Ask (default)",
							edits: "Auto-edit (edits)",
							auto: "Auto (reviewed)",
							yolo: "Full access (yolo)",
						})}
					>
						{(option) => <option value={option.value}>{option.label}</option>}
					</For>
				</Select>
				<div class={s.helpLabel}>
					<label for="ego-permission-sandbox">Filesystem sandbox</label>
					<ContextualHelp label="Filesystem sandbox" />
				</div>
				<Select
					data-setting-id="agents.agents.{key}.ego_sandbox"
					id="ego-permission-sandbox"
					class={a.formInput}
					value={configStore.state.agents.ego?.ego_sandbox ?? ""}
					onChange={(event) =>
						void configStore.setEgoPermissions({
							ego_sandbox: (event.currentTarget.value as import("../../../agents").EgoSandbox) || undefined,
						})
					}
				>
					<For
						each={settingOptions("agents.agents.{key}.ego_sandbox", {
							"": "Use ego configuration",
							ro: "Read-only (ro)",
							workspace: "Workspace writes (workspace)",
						})}
					>
						{(option) => <option value={option.value}>{option.label}</option>}
					</For>
				</Select>
				<p class={s.hint}>Applies to new terminal launches and managed spawns, including resumes.</p>
			</div>
		</SectionCard>
	);
};

const HibernationSection: Component = () => {
	const store = useAgentConfig();
	return (
		<SectionCard title={t("settings.agents.hibernateHeading", "Idle agent hibernation")}>
			<ExpertSetting configKey="agents.abandoned_shell_hours" value={store.state.abandoned_shell_hours}>
				<div class={a.fieldRow}>
					<div class={s.helpLabel}>
						<label for="abandoned-shell-hours">
							{t("settings.agents.abandonedHours", "Close abandoned shells after (hours)")}
						</label>
					</div>
					<Input
						data-setting-id="agents.abandoned_shell_hours"
						id="abandoned-shell-hours"
						class={a.formInput}
						type="number"
						min="0"
						max="65535"
						value={store.state.abandoned_shell_hours}
						onChange={(event) => void store.setAbandonedShellHours(event.currentTarget.valueAsNumber)}
					/>
				</div>
				<p class={s.hint}>
					{t(
						"settings.agents.abandonedHint",
						"Zero disables automatic closure. Plain shells are marked abandoned after one idle hour. Running processes, managed sessions and uncertain process observations are never closed.",
					)}
				</p>
			</ExpertSetting>
			<ExpertSetting configKey="agents.hibernate_idle_minutes" value={store.state.hibernate_idle_minutes}>
				<div class={a.fieldRow}>
					<div class={s.helpLabel}>
						<label for="hibernate-minutes">
							{t("settings.agents.hibernateMinutes", "Hibernate idle agents after")}
						</label>
						<ContextualHelp label={t("settings.agents.hibernateMinutes", "Hibernate idle agents after")} />
					</div>
					<Input
						data-setting-id="agents.hibernate_idle_minutes"
						id="hibernate-minutes"
						class={a.formInput}
						type="number"
						min="0"
						max="1440"
						value={store.state.hibernate_idle_minutes}
						onChange={(event) =>
							void store.setHibernate(event.currentTarget.valueAsNumber, store.state.hibernate_scope)
						}
					/>
				</div>
			</ExpertSetting>
			<ExpertSetting configKey="agents.hibernate_scope" value={store.state.hibernate_scope}>
				<div class={a.fieldRow}>
					<div class={s.helpLabel}>
						<label for="hibernate-scope">{t("settings.agents.hibernateScope", "Hibernate scope")}</label>
						<ContextualHelp label={t("settings.agents.hibernateScope", "Hibernate scope")} />
					</div>
					<Select
						data-setting-id="agents.hibernate_scope"
						id="hibernate-scope"
						class={a.formInput}
						value={store.state.hibernate_scope}
						onChange={(event) =>
							void store.setHibernate(
								store.state.hibernate_idle_minutes,
								event.currentTarget.value as "background_only" | "always",
							)
						}
					>
						<For
							each={settingOptions("agents.hibernate_scope", {
								background_only: t("settings.agents.hibernateBackground", "Only in background"),
								always: t("settings.agents.hibernateAlways", "Always"),
							})}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
				</div>
			</ExpertSetting>
			<p class={s.hint}>
				{t(
					"settings.agents.hibernateHint",
					"Minutes of awake idle time. Set to 0 to disable. Only unattended resumable agents hibernate; managed children keep their idle-close policy.",
				)}
			</p>
		</SectionCard>
	);
};
