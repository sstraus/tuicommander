import { describe, it, expect, beforeEach, vi } from "vitest";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { activityStore } from "../../stores/activityStore";
import { pluginStore } from "../../stores/pluginStore";
import { mdTabsStore } from "../../stores/mdTabs";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import type { TuiPlugin, PluginHost } from "../../plugins/types";

vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

function makePlugin(id: string, onload?: (host: PluginHost) => void, onunload?: () => void): TuiPlugin {
  return {
    id,
    onload: onload ?? (() => {}),
    onunload: onunload ?? (() => {}),
  };
}

beforeEach(() => {
  pluginRegistry.clear();
  contextMenuActionsStore.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  pluginStore.clear();
  mdTabsStore.clearAll();
});

describe("registerTerminalAction", () => {
  it("external plugin with ui:context-menu capability can register actions", async () => {
    await pluginRegistry.register(
      makePlugin("p1", (host) => {
        host.registerTerminalAction({ id: "act1", label: "Test Action", action: () => {} });
      }),
      ["ui:context-menu"],
    );
    expect(contextMenuActionsStore.getActions()).toHaveLength(1);
    expect(contextMenuActionsStore.getActions()[0].label).toBe("Test Action");
  });

  it("external plugin without ui:context-menu capability fails to load and registers no actions", async () => {
    // register() catches PluginCapabilityError internally (logs + marks plugin as failed)
    await pluginRegistry.register(
      makePlugin("p1", (host) => {
        host.registerTerminalAction({ id: "act1", label: "Test", action: () => {} });
      }),
      ["fs:read"],
    );
    // Action was not registered because onload threw PluginCapabilityError
    expect(contextMenuActionsStore.getActions()).toHaveLength(0);
  });

  it("built-in plugin (no capabilities) can register actions", () => {
    pluginRegistry.register(
      makePlugin("builtin", (host) => {
        host.registerTerminalAction({ id: "act1", label: "Built-in Action", action: () => {} });
      }),
    );
    expect(contextMenuActionsStore.getActions()).toHaveLength(1);
  });

  it("unregistering plugin auto-disposes its actions", async () => {
    await pluginRegistry.register(
      makePlugin("p1", (host) => {
        host.registerTerminalAction({ id: "act1", label: "Will Be Removed", action: () => {} });
      }),
      ["ui:context-menu"],
    );
    expect(contextMenuActionsStore.getActions()).toHaveLength(1);
    pluginRegistry.unregister("p1");
    expect(contextMenuActionsStore.getActions()).toHaveLength(0);
  });

  it("stale action handler is a no-op after plugin unregister", async () => {
    const handler = vi.fn();
    await pluginRegistry.register(
      makePlugin("p1", (host) => {
        host.registerTerminalAction({ id: "act1", label: "Stale", action: handler });
      }),
      ["ui:context-menu"],
    );
    // Capture the action before unregister
    const action = contextMenuActionsStore.getActions()[0];
    pluginRegistry.unregister("p1");
    // The action reference still exists but handler should not fire
    action.action({ sessionId: null, repoPath: null });
    expect(handler).not.toHaveBeenCalled();
  });
});
