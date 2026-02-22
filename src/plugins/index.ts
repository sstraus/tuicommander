import { pluginRegistry } from "./pluginRegistry";
import { pluginStore } from "../stores/pluginStore";
import { loadUserPlugins } from "./pluginLoader";
import { planPlugin } from "./planPlugin";
import { wizStoriesPlugin } from "./wizStoriesPlugin";
import type { TuiPlugin } from "./types";

/**
 * All built-in plugins shipped with TUI Commander.
 * Order does not matter — plugins are independent.
 */
const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin, wizStoriesPlugin];

/**
 * Register all built-in plugins, then discover and load user plugins.
 * Call once at app startup.
 */
export async function initPlugins(): Promise<void> {
  for (const plugin of BUILTIN_PLUGINS) {
    pluginStore.registerPlugin(plugin.id, { builtIn: true, enabled: true });
    pluginRegistry.register(plugin);
  }
  await loadUserPlugins();
}
