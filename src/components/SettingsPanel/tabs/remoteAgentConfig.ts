import type { AgentsConfig } from "../../../agents";
import { rpc } from "../../../transport";

export async function loadRemoteAgentConfig(connectionId: string): Promise<AgentsConfig> {
	return rpc<AgentsConfig>("load_agents_config", {}, connectionId);
}

export async function saveRemoteAgentConfig(connectionId: string, config: AgentsConfig): Promise<void> {
	await rpc<void>("save_agents_config", { config }, connectionId);
}
