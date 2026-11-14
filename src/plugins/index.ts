import { initAgentActivity } from "../features/agentActivity";
import { destroyAgentUsage, initAgentUsage } from "../features/agentUsage";
import { isPluginDisabled, loadUserPlugins, syncDisabledList } from "./pluginLoader";

/**
 * Discover and load external plugins, then initialize native features.
 * Also initializes native features that use the disabled_plugin_ids toggle.
 * Call once at app startup.
 */
export async function initPlugins(): Promise<void> {
	// Sync disabled state before initializing native and external features.
	await syncDisabledList();

	// Native agent usage ticker (Claude + Codex) — uses the same
	// disabled_plugin_ids toggle, still keyed "claude-usage".
	if (!isPluginDisabled("claude-usage")) {
		initAgentUsage();
	}

	initAgentActivity();

	await loadUserPlugins(false);
}

/**
 * Toggle the native agent usage ticker (Claude + Codex).
 * Called from AgentsTab when user flips the dashboard toggle.
 */
export function setClaudeUsageEnabled(enabled: boolean): void {
	if (enabled) {
		initAgentUsage();
	} else {
		destroyAgentUsage();
	}
}
