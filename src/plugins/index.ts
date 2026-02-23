import { pluginRegistry } from "./pluginRegistry";
import { pluginStore } from "../stores/pluginStore";
import { loadUserPlugins, isPluginDisabled, syncDisabledList, registerBuiltInPlugin } from "./pluginLoader";
import { planPlugin } from "./planPlugin";
import { claudeUsagePlugin } from "./claudeUsagePlugin";
import type { TuiPlugin } from "./types";

/**
 * Built-in plugins shipped with TUICommander.
 * wiz-stories was extracted to an external plugin (examples/plugins/wiz-stories).
 */
const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin, claudeUsagePlugin];

/**
 * Register all built-in plugins, then discover and load user plugins.
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
  await loadUserPlugins();
}
