import { describe, it, expect, vi, beforeEach } from "vitest";
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
import { testInScopeAsync } from "../helpers/store";

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
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        expect(store.state.loaded).toBe(true);
        expect(store.getRunConfigs("claude")).toHaveLength(2);
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");
      });
    });

    it("handles empty config gracefully", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {} });
        expect(store.state.loaded).toBe(true);
        expect(store.getRunConfigs("claude")).toHaveLength(0);
      });
    });

    it("handles hydrate failure gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("load failed"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.loaded).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("getDefaultConfig()", () => {
    it("returns the config marked as default", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        const def = store.getDefaultConfig("claude");
        expect(def?.name).toBe("Default");
        expect(def?.is_default).toBe(true);
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

      await testInScopeAsync(async () => {
        await hydrateWith(noDefault);
        const def = store.getDefaultConfig("claude");
        expect(def?.name).toBe("A");
      });
    });

    it("returns undefined for agent with no configs", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {} });
        const def = store.getDefaultConfig("aider");
        expect(def).toBeUndefined();
      });
    });
  });

  describe("addRunConfig()", () => {
    it("adds a config and saves", async () => {
      await testInScopeAsync(async () => {
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
      });
    });
  });

  describe("updateRunConfig()", () => {
    it("updates a config at index", async () => {
      await testInScopeAsync(async () => {
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
      });
    });

    it("ignores out-of-bounds index", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");

        const updated: AgentRunConfig = {
          name: "X", command: "x", args: [], env: {}, is_default: false,
        };
        await store.updateRunConfig("claude", 99, updated);
        expect(store.getRunConfigs("claude")[0].name).toBe("Default");
        expect(store.getRunConfigs("claude")).toHaveLength(2);
      });
    });
  });

  describe("updateRunConfigEnv()", () => {
    it("persists env from entries", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await store.updateRunConfigEnv("claude", 0, [
          { key: "FOO", value: "1" },
          { key: "BAR", value: "2" },
        ]);
        expect(store.getRunConfigs("claude")[0].env).toEqual({ FOO: "1", BAR: "2" });
      });
    });

    it("throws on duplicate keys rather than silently overwriting", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await expect(
          store.updateRunConfigEnv("claude", 0, [
            { key: "FOO", value: "1" },
            { key: "FOO", value: "2" },
          ]),
        ).rejects.toThrow(/Duplicate env keys.*FOO/);
        expect(store.getRunConfigs("claude")[0].env).toEqual({});
      });
    });

    it("ignores empty/whitespace keys", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await store.updateRunConfigEnv("claude", 0, [
          { key: "FOO", value: "1" },
          { key: "  ", value: "2" },
          { key: "", value: "3" },
        ]);
        expect(store.getRunConfigs("claude")[0].env).toEqual({ FOO: "1" });
      });
    });

    it("ignores out-of-bounds index", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await store.updateRunConfigEnv("claude", 99, [{ key: "FOO", value: "1" }]);
        expect(store.getRunConfigs("claude")[0].env).toEqual({});
      });
    });
  });

  describe("removeRunConfig()", () => {
    it("removes a config and reassigns default", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await store.removeRunConfig("claude", 0);
        const configs = store.getRunConfigs("claude");
        expect(configs).toHaveLength(1);
        expect(configs[0].name).toBe("Print");
        expect(configs[0].is_default).toBe(true);
      });
    });
  });

  describe("setDefaultConfig()", () => {
    it("sets a specific config as default", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith(configWithClaude());
        await store.setDefaultConfig("claude", 1);
        const configs = store.getRunConfigs("claude");
        expect(configs[0].is_default).toBe(false);
        expect(configs[1].is_default).toBe(true);
      });
    });
  });

  describe("headless agent", () => {
    it("defaults to null", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {} });
        expect(store.getHeadlessAgent()).toBeNull();
      });
    });

    it("persists headless_agent from config", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {}, headless_agent: "claude" });
        expect(store.getHeadlessAgent()).toBe("claude");
      });
    });

    it("can be set to 'api' for External API mode", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {} });
        store.setHeadlessAgent("api");
        expect(store.getHeadlessAgent()).toBe("api");
      });
    });

    it("saves when headless agent changes", async () => {
      await testInScopeAsync(async () => {
        await hydrateWith({ agents: {} });
        mockInvoke.mockClear();
        store.setHeadlessAgent("api");
        // setHeadlessAgent triggers a save
        expect(mockInvoke).toHaveBeenCalledWith(
          "save_agents_config",
          expect.objectContaining({
            config: expect.objectContaining({ headless_agent: "api" }),
          }),
        );
      });
    });
  });
});
