import { Component, For } from "solid-js";
import { agentFallbackStore } from "../../../stores/agentFallback";
import type { AgentType } from "../../../agents";

const AGENT_NAMES: Record<AgentType, string> = {
  claude: "Claude Code",
  gemini: "Gemini CLI",
  opencode: "OpenCode",
  aider: "Aider",
  codex: "Codex",
};

export const AgentsTab: Component = () => {
  return (
    <div class="settings-section">
      <h3>Agent Configuration</h3>
      <p class="settings-info">
        Select the AI coding agent to launch in new terminal tabs
        (e.g., Claude Code, Gemini CLI, Aider). When rate-limited,
        TUI Commander can switch to a fallback agent automatically.
      </p>

      <div class="settings-group">
        <label>Primary Agent</label>
        <select
          value={agentFallbackStore.state.primaryAgent}
          onChange={(e) => agentFallbackStore.setPrimary(e.currentTarget.value as AgentType)}
        >
          <For each={Object.entries(AGENT_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class="settings-hint">The agent launched when you open a new terminal tab</p>
      </div>

      <div class="settings-group">
        <label>Recovery</label>
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={agentFallbackStore.state.autoRecovery}
            onChange={(e) =>
              agentFallbackStore.configure({
                primary: agentFallbackStore.state.primaryAgent,
                fallbacks: agentFallbackStore.state.fallbackChain,
                recoveryIntervalMs: agentFallbackStore.state.recoveryIntervalMs,
                autoRecovery: e.currentTarget.checked,
              })
            }
          />
          <span>Automatically recover to primary agent when it becomes available</span>
        </div>
      </div>

      <div class="settings-actions">
        <button onClick={() => agentFallbackStore.forceResetToPrimary()}>
          Reset to Primary Agent
        </button>
      </div>
    </div>
  );
};
