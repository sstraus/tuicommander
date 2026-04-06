import { Component, For, Show, createSignal, onMount } from "solid-js";
import { AGENTS, AGENT_DISPLAY, MCP_SUPPORT, type AgentType, type AgentRunConfig } from "../../../agents";
import { appLogger } from "../../../stores/appLogger";
import { agentConfigsStore, llmApiStore } from "../../../stores/agentConfigs";
import { useAgentDetection, type AgentAvailability } from "../../../hooks/useAgentDetection";
import { invoke } from "../../../invoke";
import { settingsStore } from "../../../stores/settings";
import { editorTabsStore } from "../../../stores/editorTabs";
import { repositoriesStore } from "../../../stores/repositories";
import { isTauri } from "../../../transport";
import { isPluginDisabled, setPluginEnabled } from "../../../plugins/pluginLoader";
import { setClaudeUsageEnabled } from "../../../plugins";
import { AgentIcon } from "../../ui/AgentIcon";
import { CC_ENV_FLAGS, ENV_FLAG_CATEGORIES, CATEGORY_ORDER, type EnvFlagDef, type EnvFlagCategory } from "../../../data/ccEnvFlags";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

// All agent types — sorted dynamically by availability then name
const ALL_AGENT_TYPES: AgentType[] = ["claude", "cursor", "gemini", "amp", "codex", "aider", "opencode", "warp", "droid"];

interface McpStatus {
  supported: boolean;
  installed: boolean;
  config_path: string | null;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Inline form for adding a new run config */
const AddConfigForm: Component<{
  agentType: AgentType;
  onClose: () => void;
}> = (props) => {
  const [name, setName] = createSignal("");
  const [command, setCommand] = createSignal(AGENTS[props.agentType].binary);
  const [args, setArgs] = createSignal("");

  const handleSave = async () => {
    const n = name().trim();
    if (!n) return;
    const config: AgentRunConfig = {
      name: n,
      command: command().trim() || AGENTS[props.agentType].binary,
      args: args().trim() ? args().trim().split(/\s+/) : [],
      env: {},
      is_default: false,
    };
    await agentConfigsStore.addRunConfig(props.agentType, config);
    props.onClose();
  };

  return (
    <div class={a.addConfigForm}>
      <div class={a.formRow}>
        <input
          class={a.formInput}
          placeholder="Configuration name"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleSave(); if (e.key === "Escape") props.onClose(); }}
        />
      </div>
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
          onKeyDown={(e) => { if (e.key === "Enter") handleSave(); if (e.key === "Escape") props.onClose(); }}
        />
      </div>
      <div class={a.formRow}>
        <button class={a.smallBtn} onClick={handleSave}>Save</button>
        <button class={a.smallBtn} onClick={props.onClose}>Cancel</button>
      </div>
    </div>
  );
};

/** Single run config row */
const RunConfigRow: Component<{
  config: AgentRunConfig;
  index: number;
  agentType: AgentType;
}> = (props) => {
  const cmdPreview = () => {
    const parts = [props.config.command, ...props.config.args];
    return parts.join(" ");
  };

  return (
    <div class={a.configRow}>
      <span class={a.configName}>{props.config.name}</span>
      <span class={a.configCommand}>{cmdPreview()}</span>
      <Show when={props.config.is_default}>
        <span class={a.defaultBadge}>Default</span>
      </Show>
      <div class={a.configActions}>
        <Show when={!props.config.is_default}>
          <button
            class={a.smallBtn}
            onClick={() => agentConfigsStore.setDefaultConfig(props.agentType, props.index)}
            title="Set as default"
          >
            Set Default
          </button>
        </Show>
        <button
          class={`${a.smallBtn} ${a.danger}`}
          onClick={() => agentConfigsStore.removeRunConfig(props.agentType, props.index)}
          title="Delete configuration"
        >
          Delete
        </button>
      </div>
    </div>
  );
};

/** Environment flags panel — categorized toggles/inputs for CC env vars */
const EnvFlagsSection: Component<{ agentType: AgentType }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const flags = () => agentConfigsStore.getEnvFlags(props.agentType);

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
      agentConfigsStore.setEnvFlag(props.agentType, flag.key, undefined);
    } else {
      agentConfigsStore.setEnvFlag(props.agentType, flag.key, flag.type === "boolean_inverted" ? "false" : "1");
    }
  };

  const handleValueChange = (key: string, value: string) => {
    if (value) {
      agentConfigsStore.setEnvFlag(props.agentType, key, value);
    } else {
      agentConfigsStore.setEnvFlag(props.agentType, key, undefined);
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
        <span class={a.expandIcon} classList={{ [a.expanded]: expanded() }}>&#9654;</span>
        Environment Flags
        <Show when={activeCount() > 0}>
          <span class={a.badge} data-type="available">{activeCount()}</span>
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
                              <For each={flag.options ?? []}>
                                {(opt) => <option value={opt}>{opt}</option>}
                              </For>
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
                          <span class={a.envFlagKey}>{flag.key}</span>
                          <span class={a.envFlagDesc} title={flag.description}>{flag.description}</span>
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
          <input
            type="checkbox"
            checked={enabled()}
            onChange={handleToggle}
          />
          <span>Usage Dashboard</span>
        </label>
        <span class={s.hint}>Rate limits, session analytics, and activity heatmap in status bar and Activity Center</span>
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
  const [expanded, setExpanded] = createSignal(false);
  const [addingConfig, setAddingConfig] = createSignal(false);
  const [mcpStatus, setMcpStatus] = createSignal<McpStatus | null>(null);
  const [mcpLoading, setMcpLoading] = createSignal(false);

  const agent = () => AGENTS[props.agentType];
  const display = () => AGENT_DISPLAY[props.agentType];
  const configs = () => agentConfigsStore.getRunConfigs(props.agentType);
  const supportsMcp = () => MCP_SUPPORT[props.agentType];

  const loadMcpStatus = async () => {
    if (!supportsMcp() || !isTauri()) return;
    try {
      const status = await invoke<McpStatus>("get_agent_mcp_status", { agentType: props.agentType });
      setMcpStatus(status);
    } catch (err) {
      appLogger.error("config", `Failed to get MCP status for ${props.agentType}`, err);
    }
  };

  const handleExpand = () => {
    const newVal = !expanded();
    setExpanded(newVal);
    if (newVal) {
      loadMcpStatus();
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
        editorTabsStore.add(repoPath, configPath);
      }
    } catch (err) {
      appLogger.error("config", `Failed to open config for ${props.agentType}`, err);
    }
  };

  const isEnabled = () => settingsStore.isAgentEnabled(props.agentType);

  return (
    <div class={a.agentRow}>
      <div class={a.agentHeader} onClick={handleExpand}>
        <div class={a.agentInfo}>
          <div class={a.agentNameRow}>
            <div class={a.agentIcon} style={{ background: display().color, opacity: isEnabled() ? 1 : 0.4 }}>
              <AgentIcon agent={props.agentType} size={16} />
            </div>
            <span class={a.agentName} style={{ opacity: isEnabled() ? 1 : 0.5 }}>{agent().name}</span>
            <Show when={props.detection?.version}>
              <span class={a.agentVersion}>{props.detection!.version}</span>
            </Show>
            <Show
              when={props.detection?.available}
              fallback={<span class={a.badge} data-type="notfound">Not found</span>}
            >
              <Show
                when={isEnabled()}
                fallback={<span class={a.badge} data-type="disabled">Disabled</span>}
              >
                <span class={a.badge} data-type="available">Available</span>
              </Show>
            </Show>
            <Show when={mcpStatus()?.installed}>
              <span class={a.badge} data-type="mcp">MCP</span>
            </Show>
          </div>
        </div>
        <span class={a.expandIcon} classList={{ [a.expanded]: expanded() }}>&#9654;</span>
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
                checked={agentConfigsStore.isAutoRetryEnabled(props.agentType)}
                onChange={() => agentConfigsStore.setAutoRetry(
                  props.agentType,
                  !agentConfigsStore.isAutoRetryEnabled(props.agentType),
                )}
              />
              <span>Auto-retry on server errors</span>
            </label>
            <p class={s.hint}>Inject "continue" on 5xx errors with backoff (5s, 15s, 30s)</p>
          </div>

          {/* Per-agent TUIC protocol markers — visible when MCP bridge is installed */}
          <Show when={mcpStatus()?.installed}>
            <div class={a.expandedSection}>
              <label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  checked={agentConfigsStore.getIntentTabTitle(props.agentType) ?? !AGENTS[props.agentType]?.managesOwnTabTitle}
                  onChange={(e) => agentConfigsStore.setIntentTabTitle(props.agentType, e.currentTarget.checked)}
                />
                <span>Show intent as tab title</span>
              </label>
              <p class={s.hint}>
                Emit <code>intent:</code> markers to update the tab name with current work phase
                {AGENTS[props.agentType]?.managesOwnTabTitle ? " (off by default — agent manages its own title)" : ""}
              </p>
            </div>

            <div class={a.expandedSection}>
              <label class={a.toggleRow} onClick={(e) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  checked={agentConfigsStore.getSuggestFollowups(props.agentType) ?? settingsStore.state.suggestFollowups}
                  onChange={(e) => agentConfigsStore.setSuggestFollowups(props.agentType, e.currentTarget.checked)}
                />
                <span>Show suggested follow-ups</span>
              </label>
              <p class={s.hint}>Emit <code>suggest:</code> markers for clickable follow-up actions</p>
            </div>
          </Show>

          {/* Headless Command Template */}
          <div class={a.expandedSection}>
            <div class={a.expandedLabel}>Headless Command Template</div>
            <input
              class={`${a.formInput} ${a.mono}`}
              placeholder={`${AGENTS[props.agentType].binary} -p "{prompt}" --no-input`}
              value={agentConfigsStore.getHeadlessTemplate(props.agentType) ?? ""}
              onInput={(e) => agentConfigsStore.setHeadlessTemplate(props.agentType, e.currentTarget.value)}
              onClick={(e) => e.stopPropagation()}
            />
            <p class={s.hint}>Command template for one-shot execution. Use {"{prompt}"} as placeholder for the prompt text.</p>
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
                  {(config, i) => (
                    <RunConfigRow config={config} index={i()} agentType={props.agentType} />
                  )}
                </For>
              </div>
            </Show>
            <Show when={addingConfig()} fallback={
              <button
                class={a.smallBtn}
                style={{ "margin-top": "8px" }}
                onClick={() => setAddingConfig(true)}
              >
                Add Configuration...
              </button>
            }>
              <AddConfigForm
                agentType={props.agentType}
                onClose={() => setAddingConfig(false)}
              />
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
                    <span class={a.mcpDot} classList={{ [a.on]: mcpStatus()!.installed, [a.off]: !mcpStatus()!.installed }} />
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
// Main tab
// ---------------------------------------------------------------------------

export const AgentsTab: Component = () => {
  const detection = useAgentDetection();

  onMount(() => {
    detection.detectAll();
    agentConfigsStore.hydrate();
  });

  /** Agents sorted: available first, then not-found — each group alphabetically by display name */
  const sortedAgents = () => {
    const types = ALL_AGENT_TYPES.filter((t) => t !== "api");
    const byName = (a: AgentType, b: AgentType) =>
      AGENTS[a].name.localeCompare(AGENTS[b].name);
    const available = types.filter((t) => detection.isAvailable(t)).sort(byName);
    const unavailable = types.filter((t) => !detection.isAvailable(t)).sort(byName);
    return [...available, ...unavailable];
  };

  return (
    <div class={s.section}>
      <h3>Agents</h3>
      <p class={s.hint} style={{ "margin-bottom": "12px" }}>
        Configure AI coding agents, manage run configurations, and install MCP bridge integrations.
      </p>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.intentTabTitle}
            onChange={(e) => settingsStore.setIntentTabTitle(e.currentTarget.checked)}
          />
          <span>Show agent intent as tab title</span>
        </div>
        <p class={s.hint}>When agents declare their current work phase, update the tab name with a short title</p>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.suggestFollowups}
            onChange={(e) => settingsStore.setSuggestFollowups(e.currentTarget.checked)}
          />
          <span>Show suggested follow-up actions</span>
        </div>
        <p class={s.hint}>Display actionable suggestions from agents after completing a task</p>
      </div>

      <div class={s.group}>
        <label>Headless Agent</label>
        <select
          value={agentConfigsStore.getHeadlessAgent() ?? ""}
          onChange={(e) => {
            const val = e.currentTarget.value;
            agentConfigsStore.setHeadlessAgent(val ? val as AgentType : null);
          }}
        >
          <option value="">— Not configured —</option>
          <For each={ALL_AGENT_TYPES.filter((t) => t !== "api" && detection.isAvailable(t) && AGENTS[t]?.defaultHeadlessTemplate)}>
            {(type) => <option value={type}>{AGENTS[type]?.name ?? type}</option>}
          </For>
          <option value="api">External API</option>
        </select>
        <p class={s.hint}>
          Agent CLI for headless prompts, or External API for direct LLM calls
          {detection.loading() ? " — detecting..." : ""}
        </p>
      </div>

      {/* LLM API Configuration — visible only when headless agent is set to External API */}
      <Show when={agentConfigsStore.getHeadlessAgent() === "api"}>
        <LlmApiSection />
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
    </div>
  );
};

// ---------------------------------------------------------------------------
// LLM API Section
// ---------------------------------------------------------------------------

interface LlmProvider {
  value: string;
  label: string;
  placeholder: string;
  needsUrl?: boolean;
  defaultUrl?: string;
}

const LLM_PROVIDERS: LlmProvider[] = [
  { value: "openai", label: "OpenAI", placeholder: "gpt-4o-mini" },
  { value: "anthropic", label: "Anthropic", placeholder: "claude-sonnet-4-5-20241022" },
  { value: "gemini", label: "Google Gemini", placeholder: "gemini-2.0-flash" },
  { value: "openrouter", label: "OpenRouter", placeholder: "openai/gpt-4o-mini", needsUrl: true, defaultUrl: "https://openrouter.ai/api/v1/" },
  { value: "ollama", label: "Ollama (local)", placeholder: "llama3.2", needsUrl: true, defaultUrl: "http://localhost:11434/v1/" },
  { value: "custom", label: "Custom (OpenAI-compatible)", placeholder: "model-name", needsUrl: true },
];

const LlmApiSection: Component = () => {
  const [apiKey, setApiKey] = createSignal("");
  const [testResult, setTestResult] = createSignal<{ ok: boolean; msg: string } | null>(null);
  const [testing, setTesting] = createSignal(false);

  onMount(() => { llmApiStore.hydrate(); });

  const config = () => llmApiStore.state.config;
  const providerInfo = () => LLM_PROVIDERS.find((p) => p.value === config().provider);
  const needsUrl = () => providerInfo()?.needsUrl ?? false;

  const handleProviderChange = (provider: string) => {
    const info = LLM_PROVIDERS.find((p) => p.value === provider);
    const base_url = info?.needsUrl ? (info.defaultUrl ?? "") : undefined;
    llmApiStore.saveConfig({ provider, model: config().model, base_url });
  };

  const handleModelChange = (model: string) => {
    llmApiStore.saveConfig({ ...config(), model });
  };

  const handleBaseUrlChange = (base_url: string) => {
    llmApiStore.saveConfig({ ...config(), base_url: base_url || undefined });
  };

  const handleSaveKey = async () => {
    const key = apiKey().trim();
    if (!key) return;
    try {
      await llmApiStore.saveKey(key);
      setApiKey("");
      setTestResult(null);
      appLogger.info("config", "LLM API key saved to keyring");
    } catch (err) {
      appLogger.error("config", "Failed to save API key", err);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const msg = await llmApiStore.testConnection();
      setTestResult({ ok: true, msg });
    } catch (err) {
      setTestResult({ ok: false, msg: String(err) });
    } finally {
      setTesting(false);
    }
  };

  return (
      <div class={s.section} style={{ "border-top": "1px solid var(--border)", "padding-top": "20px", "margin-top": "20px" }}>
        <h3>LLM API</h3>
        <p class={s.hint} style={{ "margin-top": "-12px", "margin-bottom": "16px" }}>Direct LLM API for Smart Prompts in "API" execution mode</p>

        <div class={s.group}>
          <label>Provider</label>
          <select
            value={config().provider}
            onChange={(e) => handleProviderChange(e.currentTarget.value)}
          >
            <option value="">— Select provider —</option>
            <For each={LLM_PROVIDERS}>
              {(p) => <option value={p.value}>{p.label}</option>}
            </For>
          </select>
        </div>

        <div class={s.group}>
          <label>Model</label>
          <input
            type="text"
            value={config().model}
            placeholder={providerInfo()?.placeholder ?? "model-name"}
            onInput={(e) => handleModelChange(e.currentTarget.value)}
          />
        </div>

        <Show when={needsUrl()}>
          <div class={s.group}>
            <label>Base URL</label>
            <input
              type="text"
              value={config().base_url ?? ""}
              placeholder="https://..."
              onInput={(e) => handleBaseUrlChange(e.currentTarget.value)}
            />
          </div>
        </Show>

        <div class={s.group}>
          <label>API Key</label>
          <Show
            when={!llmApiStore.state.hasKey}
            fallback={
              <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
                <span style={{ color: "var(--success)", "font-size": "var(--font-sm)", "flex-shrink": "0" }}>Stored</span>
                <input
                  type="text"
                  style={{ flex: "1" }}
                  value={apiKey()}
                  placeholder="Enter new key to replace"
                  onInput={(e) => setApiKey(e.currentTarget.value)}
                />
                <Show when={apiKey().trim()}>
                  <button class={a.actionBtn} onClick={handleSaveKey}>Save</button>
                </Show>
              </div>
            }
          >
            <div style={{ display: "flex", gap: "8px" }}>
              <input
                type="text"
                style={{ flex: "1" }}
                value={apiKey()}
                placeholder="Paste your API key"
                onInput={(e) => setApiKey(e.currentTarget.value)}
              />
              <Show when={apiKey().trim()}>
                <button class={a.actionBtn} onClick={handleSaveKey}>Save</button>
              </Show>
            </div>
          </Show>
        </div>

        <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
          <button
            class={a.actionBtn}
            disabled={testing() || !config().provider || !config().model || !llmApiStore.state.hasKey}
            onClick={handleTest}
          >
            {testing() ? "Testing..." : "Test Connection"}
          </button>
          <Show when={testResult()}>
            {(result) => (
              <span style={{ color: result().ok ? "var(--success)" : "var(--error)", "font-size": "var(--font-sm)" }}>
                {result().msg.slice(0, 120)}
              </span>
            )}
          </Show>
        </div>
      </div>
  );
};
