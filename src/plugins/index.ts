import { pluginRegistry } from "./pluginRegistry";
import { pluginStore } from "../stores/pluginStore";
import { loadUserPlugins } from "./pluginLoader";
import { planPlugin } from "./planPlugin";
import type { TuiPlugin } from "./types";

/**
 * Built-in plugins shipped with TUI Commander.
 * wiz-stories was extracted to an external plugin (examples/plugins/wiz-stories).
 */
const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin];

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
