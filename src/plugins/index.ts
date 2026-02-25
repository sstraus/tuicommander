import { pluginRegistry } from "./pluginRegistry";
import { pluginStore } from "../stores/pluginStore";
import { loadUserPlugins, isPluginDisabled, syncDisabledList, registerBuiltInPlugin } from "./pluginLoader";
import { planPlugin } from "./planPlugin";
import { initClaudeUsage, destroyClaudeUsage } from "../features/claudeUsage";
import type { TuiPlugin } from "./types";

/**
 * Built-in plugins shipped with TUICommander.
 * Session prompts moved to native Rust (last_prompts in AppState, displayed in Activity Dashboard).
 * Claude Usage Dashboard was moved from plugin to native feature (src/features/claudeUsage.ts).
 */
const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin];

/**
 * Register all built-in plugins, then discover and load user plugins.
 * Also initializes native features that use the disabled_plugin_ids toggle.
 * Call once at app startup.
 */
export async function initPlugins(): Promise<void> {
  // Sync disabled list before checking built-in plugin state
  await syncDisabledList();

  for (const plugin of BUILTIN_PLUGINS) {
    registerBuiltInPlugin(plugin);
    const enabled = !isPluginDisabled(plugin.id);
    pluginStore.registerPlugin(plugin.id, { builtIn: true, enabled });
    if (enabled) {
      pluginRegistry.register(plugin);
    }
  }

  // Native Claude Usage feature — uses same disabled_plugin_ids toggle
  if (!isPluginDisabled("claude-usage")) {
    initClaudeUsage();
  }

  await loadUserPlugins();
}

/**
 * Toggle the native Claude Usage feature.
 * Called from AgentsTab when user flips the dashboard toggle.
 */
export function setClaudeUsageEnabled(enabled: boolean): void {
  if (enabled) {
    initClaudeUsage();
  } else {
    destroyClaudeUsage();
  }
}
