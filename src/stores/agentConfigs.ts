import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import type { AgentType, AgentRunConfig, AgentsConfig } from "../agents";
import { appLogger } from "./appLogger";

interface AgentConfigsState {
  agents: Record<string, { run_configs: AgentRunConfig[] }>;
  loaded: boolean;
}

/** Deep-clone a plain object to break SolidJS proxy references */
function clone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj));
}

function createAgentConfigsStore() {
  const [state, setState] = createStore<AgentConfigsState>({
    agents: {},
    loaded: false,
  });

  /** Save full config to Rust */
  async function saveToDisk(): Promise<void> {
    const full: AgentsConfig = { agents: clone(state.agents) };
    await invoke("save_agents_config", { config: full });
  }

  const actions = {
    /** Hydrate store from persisted config */
    async hydrate(): Promise<void> {
      try {
        const config = await invoke<AgentsConfig>("load_agents_config");
        setState(produce((s) => {
          s.agents = config.agents ?? {};
          s.loaded = true;
        }));
      } catch (err) {
        appLogger.error("config", "Failed to hydrate agent configs", err);
        setState("loaded", true);
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
      setState(produce((s) => {
        if (!s.agents[type]) {
          s.agents[type] = { run_configs: [] };
        }
        const newConfig = clone(config);
        if (s.agents[type].run_configs.length === 0) {
          newConfig.is_default = true;
        }
        s.agents[type].run_configs.push(newConfig);
      }));
      try {
        await saveToDisk();
      } catch (err) {
        appLogger.error("config", "Failed to save agent config", err);
      }
    },

    /** Update a run config at a specific index */
    async updateRunConfig(type: AgentType, index: number, config: AgentRunConfig): Promise<void> {
      const current = state.agents[type]?.run_configs ?? [];
      if (index < 0 || index >= current.length) return;
      setState(produce((s) => {
        s.agents[type].run_configs[index] = clone(config);
      }));
      try {
        await saveToDisk();
      } catch (err) {
        appLogger.error("config", "Failed to save agent config", err);
      }
    },

    /** Remove a run config at a specific index */
    async removeRunConfig(type: AgentType, index: number): Promise<void> {
      const current = state.agents[type]?.run_configs ?? [];
      if (index < 0 || index >= current.length) return;
      setState(produce((s) => {
        const configs = s.agents[type].run_configs;
        const wasDefault = configs[index].is_default;
        configs.splice(index, 1);
        if (wasDefault && configs.length > 0) {
          configs[0].is_default = true;
        }
      }));
      try {
        await saveToDisk();
      } catch (err) {
        appLogger.error("config", "Failed to save agent config", err);
      }
    },

    /** Set a specific config as the default (unset others) */
    async setDefaultConfig(type: AgentType, index: number): Promise<void> {
      const current = state.agents[type]?.run_configs ?? [];
      if (index < 0 || index >= current.length) return;
      setState(produce((s) => {
        const configs = s.agents[type].run_configs;
        for (let i = 0; i < configs.length; i++) {
          configs[i].is_default = i === index;
        }
      }));
      try {
        await saveToDisk();
      } catch (err) {
        appLogger.error("config", "Failed to save agent config", err);
      }
    },
  };

  return { state, ...actions };
}

export const agentConfigsStore = createAgentConfigsStore();
