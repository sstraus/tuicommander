import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import type { AgentRunConfig, AgentsConfig } from "../../agents";

const { mockInvoke } = vi.hoisted(() => {
  const mockInvoke = vi.fn().mockResolvedValue(undefined);
  return { mockInvoke };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

// Import the store once (it's a singleton)
import { agentConfigsStore as store } from "../../stores/agentConfigs";

const configWithClaude = (): AgentsConfig => ({
  agents: {
    claude: {
      run_configs: [
        { name: "Default", command: "claude", args: [], env: {}, is_default: true },
        { name: "Print", command: "claude", args: ["--print"], env: {}, is_default: false },
      ],
    },
  },
});

/** Helper: hydrate store with a specific config */
async function hydrateWith(config: AgentsConfig): Promise<void> {
  mockInvoke.mockResolvedValueOnce(config);
  await store.hydrate();
}

describe("agentConfigsStore", () => {
  beforeEach(() => {
    mockInvoke.mockReset().mockResolvedValue(undefined);
  });

  describe("hydrate()", () => {
    it("loads agent configs from Rust backend", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        expect(store.state.loaded).toBe(true);
        expect(store.getRunConfigs("claude")).toHaveLength(2);
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");
        dispose();
      });
    });

    it("handles empty config gracefully", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith({ agents: {} });
        expect(store.state.loaded).toBe(true);
        expect(store.getRunConfigs("claude")).toHaveLength(0);
        dispose();
      });
    });

    it("handles hydrate failure gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("load failed"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.loaded).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("getDefaultConfig()", () => {
    it("returns the config marked as default", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        const def = store.getDefaultConfig("claude");
        expect(def?.name).toBe("Default");
        expect(def?.is_default).toBe(true);
        dispose();
      });
    });

    it("returns first config if none marked as default", async () => {
      const noDefault: AgentsConfig = {
        agents: {
          claude: {
            run_configs: [
              { name: "A", command: "claude", args: [], env: {}, is_default: false },
              { name: "B", command: "claude", args: [], env: {}, is_default: false },
            ],
          },
        },
      };

      await createRoot(async (dispose) => {
        await hydrateWith(noDefault);
        const def = store.getDefaultConfig("claude");
        expect(def?.name).toBe("A");
        dispose();
      });
    });

    it("returns undefined for agent with no configs", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith({ agents: {} });
        const def = store.getDefaultConfig("aider");
        expect(def).toBeUndefined();
        dispose();
      });
    });
  });

  describe("addRunConfig()", () => {
    it("adds a config and saves", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith({ agents: {} });
        const newConfig: AgentRunConfig = {
          name: "Test",
          command: "claude",
          args: ["--test"],
          env: {},
          is_default: false,
        };
        await store.addRunConfig("claude", newConfig);
        const configs = store.getRunConfigs("claude");
        expect(configs).toHaveLength(1);
        // First config should be auto-set as default
        expect(configs[0].is_default).toBe(true);
        expect(configs[0].name).toBe("Test");
        dispose();
      });
    });
  });

  describe("updateRunConfig()", () => {
    it("updates a config at index", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        const updated: AgentRunConfig = {
          name: "Updated",
          command: "claude",
          args: ["--verbose"],
          env: {},
          is_default: true,
        };
        await store.updateRunConfig("claude", 0, updated);
        expect(store.getRunConfigs("claude")[0].name).toBe("Updated");
        expect(store.getRunConfigs("claude")[0].args).toEqual(["--verbose"]);
        dispose();
      });
    });

    it("ignores out-of-bounds index", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");

        const updated: AgentRunConfig = {
          name: "X", command: "x", args: [], env: {}, is_default: false,
        };
        await store.updateRunConfig("claude", 99, updated);
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");
        expect(store.getRunConfigs("claude")).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("removeRunConfig()", () => {
    it("removes a config and reassigns default", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        await store.removeRunConfig("claude", 0);
        const configs = store.getRunConfigs("claude");
        expect(configs).toHaveLength(1);
        expect(configs[0].name).toBe("Print");
        expect(configs[0].is_default).toBe(true);
        dispose();
      });
    });
  });

  describe("setDefaultConfig()", () => {
    it("sets a specific config as default", async () => {
      await createRoot(async (dispose) => {
        await hydrateWith(configWithClaude());
        await store.setDefaultConfig("claude", 1);
        const configs = store.getRunConfigs("claude");
        expect(configs[0].is_default).toBe(false);
        expect(configs[1].is_default).toBe(true);
        dispose();
      });
    });
  });
});
