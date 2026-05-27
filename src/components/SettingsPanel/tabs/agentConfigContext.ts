import { createContext, useContext } from "solid-js";
import { type AgentConfigIO, createAgentConfigsStore } from "../../../stores/agentConfigs";
import { loadRemoteAgentConfig, saveRemoteAgentConfig } from "./remoteAgentConfig";

export type AgentConfigStore = ReturnType<typeof createAgentConfigsStore>;

const AgentConfigContext = createContext<AgentConfigStore>();

export const AgentConfigProvider = AgentConfigContext.Provider;

export function useAgentConfig(): AgentConfigStore {
	const ctx = useContext(AgentConfigContext);
	if (!ctx) throw new Error("useAgentConfig must be used within AgentConfigProvider");
	return ctx;
}

export function createRemoteAgentConfigStore(connectionId: string): AgentConfigStore {
	const io: AgentConfigIO = {
		load: () => loadRemoteAgentConfig(connectionId),
		save: (config) => saveRemoteAgentConfig(connectionId, config),
	};
	return createAgentConfigsStore(io);
}
