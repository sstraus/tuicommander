import { pluginRegistry } from "./pluginRegistry";
import { planPlugin } from "./planPlugin";
import { wizStoriesPlugin } from "./wizStoriesPlugin";
import type { TuiPlugin } from "./types";

/**
 * All built-in plugins shipped with TUI Commander.
 * Order does not matter — plugins are independent.
 */
export const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin, wizStoriesPlugin];

/**
 * Register all built-in plugins with the plugin registry.
 * Call once at app startup.
 */
export function initPlugins(): void {
  for (const plugin of BUILTIN_PLUGINS) {
    pluginRegistry.register(plugin);
  }
}
