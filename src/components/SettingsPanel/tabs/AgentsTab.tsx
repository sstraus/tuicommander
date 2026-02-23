import { Component, For, Show, createSignal, onMount } from "solid-js";
import { AGENTS, AGENT_DISPLAY, MCP_SUPPORT, type AgentType, type AgentRunConfig } from "../../../agents";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { useAgentDetection, type AgentAvailability } from "../../../hooks/useAgentDetection";
import { invoke } from "../../../invoke";
import { settingsStore } from "../../../stores/settings";
import { isTauri } from "../../../transport";
import { isPluginDisabled, setPluginEnabled } from "../../../plugins/pluginLoader";
import { setClaudeUsageEnabled } from "../../../plugins";
import { AgentIcon } from "../../ui/AgentIcon";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

// All agent types in display order
const AGENT_TYPES: AgentType[] = ["claude", "cursor", "gemini", "amp", "codex", "aider", "opencode", "jules", "warp", "ona"];

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
      console.error("Failed to toggle Claude Usage Dashboard:", err);
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
      console.error(`Failed to get MCP status for ${props.agentType}:`, err);
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
      console.error(`MCP toggle failed for ${props.agentType}:`, err);
    } finally {
      setMcpLoading(false);
    }
  };

  const handleEditConfig = async () => {
    try {
      const configPath = await invoke<string | null>("get_agent_config_path", { agentType: props.agentType });
      if (configPath) {
        await invoke("open_in_app", { path: configPath, app: settingsStore.state.ide });
      }
    } catch (err) {
      console.error(`Failed to open config for ${props.agentType}:`, err);
    }
  };

  return (
    <div class={a.agentRow}>
      <div class={a.agentHeader} onClick={handleExpand}>
        <div class={a.agentInfo}>
          <div class={a.agentNameRow}>
            <div class={a.agentIcon} style={{ background: display().color }}>
              <AgentIcon agent={props.agentType} size={16} />
            </div>
            <span class={a.agentName}>{agent().name}</span>
            <Show when={props.detection?.version}>
              <span class={a.agentVersion}>{props.detection!.version}</span>
            </Show>
            <Show
              when={props.detection?.available}
              fallback={<span class={a.badge} data-type="notfound">Not found</span>}
            >
              <span class={a.badge} data-type="available">Available</span>
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

          {/* Claude-specific: Usage Dashboard toggle */}
          <Show when={props.agentType === "claude"}>
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

  return (
    <div class={s.section}>
      <h3>Agents</h3>
      <p class={s.hint} style={{ "margin-bottom": "12px" }}>
        Configure AI coding agents, manage run configurations, and install MCP bridge integrations.
      </p>

      <div class={a.agentList}>
        <For each={AGENT_TYPES}>
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
