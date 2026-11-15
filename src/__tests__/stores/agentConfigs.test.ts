import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentRunConfig, AgentsConfig } from "../../agents";

const { mockInvoke } = vi.hoisted(() => {
	const mockInvoke = vi.fn().mockResolvedValue(undefined);
	return { mockInvoke };
});

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

// Import the store once (it's a singleton)
import { createAgentConfigsStore, agentConfigsStore as store } from "../../stores/agentConfigs";
import { appLogger } from "../../stores/appLogger";
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
	afterEach(() => vi.restoreAllMocks());
	beforeEach(() => {
		mockInvoke.mockReset().mockResolvedValue(undefined);
	});

	describe("hydrate()", () => {
		// Catches: saved removed-agent settings failing load or remaining the headless selection.
		it("loads saved aider settings without offering the retired headless agent", async () => {
			const persisted = JSON.parse(
				JSON.stringify({
					...configWithClaude(),
					headless_agent: "aider:Old profile",
					agents: {
						...configWithClaude().agents,
						aider: { run_configs: [{ name: "Old profile", command: "aider", args: [], env: {} }] },
					},
				}),
			);
			const machine = createAgentConfigsStore({ load: async () => persisted, save: vi.fn() });
			await expect(machine.hydrate()).resolves.toBeUndefined();
			expect(machine.state.loaded).toBe(true);
			expect(machine.getHeadlessAgent()).toBeNull();
			expect(machine.getDefaultConfig("claude")?.command).toBe("claude");
		});
		it("reports a failed load and permits a later successful retry", async () => {
			const log = vi.spyOn(appLogger, "error").mockImplementation(() => undefined);
			const load = vi
				.fn()
				.mockRejectedValueOnce(new Error("RPC load_agents_config failed: 404"))
				.mockResolvedValueOnce(configWithClaude());
			const machine = createAgentConfigsStore({ load, save: vi.fn() });

			await expect(machine.hydrate()).rejects.toThrow("404");
			expect(machine.state.loaded).toBe(false);
			expect(machine.state.loadError).toContain("404");
			expect(log).toHaveBeenCalledWith("config", "Failed to hydrate agent configs", expect.any(Error));
			await machine.hydrate();
			expect(machine.state.loaded).toBe(true);
			expect(machine.state.loadError).toBeNull();
			expect(machine.getDefaultConfig("claude")?.command).toBe("claude");
		});

		it("keeps the last good config when a refresh fails", async () => {
			const log = vi.spyOn(appLogger, "error").mockImplementation(() => undefined);
			const load = vi
				.fn()
				.mockResolvedValueOnce(configWithClaude())
				.mockRejectedValueOnce(new Error("connection lost"));
			const machine = createAgentConfigsStore({ load, save: vi.fn() });

			await machine.hydrate();
			await expect(machine.hydrate()).rejects.toThrow("connection lost");
			expect(machine.state.loaded).toBe(false);
			expect(machine.state.loadError).toContain("connection lost");
			expect(log).toHaveBeenCalledWith("config", "Failed to hydrate agent configs", expect.any(Error));
			expect(machine.getRunConfigs("claude")).toHaveLength(2);
		});

		it("does not overwrite remote config after a failed load", async () => {
			const log = vi.spyOn(appLogger, "error").mockImplementation(() => undefined);
			const save = vi.fn();
			const machine = createAgentConfigsStore({
				load: async () => {
					throw new Error("RPC load_agents_config failed: 404");
				},
				save,
			});
			await expect(machine.hydrate()).rejects.toThrow("404");
			await machine.addRunConfig("claude", {
				name: "new",
				command: "claude",
				args: [],
				env: {},
				is_default: true,
			});
			expect(save).not.toHaveBeenCalled();
			expect(log).toHaveBeenCalledWith("config", "Failed to hydrate agent configs", expect.any(Error));
		});

		it("accepts an empty successful response as loaded without an error", async () => {
			const machine = createAgentConfigsStore({ load: async () => ({ agents: {} }), save: vi.fn() });
			await machine.hydrate();
			expect(machine.state.loaded).toBe(true);
			expect(machine.state.loadError).toBeNull();
			expect(machine.getRunConfigs("claude")).toEqual([]);
		});

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

		it("reports a local hydrate failure without marking it loaded", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("load failed"));
			const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await expect(store.hydrate()).rejects.toThrow("load failed");
				expect(store.state.loaded).toBe(false);
				expect(store.state.loadError).toContain("load failed");
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
				const def = store.getDefaultConfig("opencode");
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

	it("does not drop the migration marker when Settings removes Codex bypass", async () => {
		let persisted: AgentsConfig = {
			agents: {
				codex: {
					codex_bypass_migrated: true,
					run_configs: [
						{
							name: "Default",
							command: "codex",
							args: ["--dangerously-bypass-approvals-and-sandbox"],
							env: {},
							is_default: true,
						},
					],
				},
			},
		};
		const machine = createAgentConfigsStore({
			load: async () => persisted,
			save: async (_base, config) => {
				persisted = config;
			},
		});
		await machine.hydrate();
		const config = machine.getDefaultConfig("codex")!;
		await machine.updateRunConfig("codex", 0, { ...config, args: [] });
		expect(persisted.agents.codex.codex_bypass_migrated).toBe(true);
		expect(persisted.agents.codex.run_configs[0].args).toEqual([]);
		await machine.hydrate();
		expect(machine.getDefaultConfig("codex")?.args).toEqual([]);
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
					name: "X",
					command: "x",
					args: [],
					env: {},
					is_default: false,
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

	describe("workflow profile", () => {
		// Catches: the owner's workflow profile choice being dropped on load or on save.
		it("loads the saved choice and saves a new one", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {}, workflow_profile: "Claude default" });
				expect(store.getWorkflowProfile()).toBe("Claude default");
				mockInvoke.mockClear();
				store.setWorkflowProfile("Grok");
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({ config: expect.objectContaining({ workflow_profile: "Grok" }) }),
				);
			});
		});
	});

	describe("per-agent progress_tracking", () => {
		it("has no opinion until one is set, which is how the agent follows the global flag", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getProgressTracking("claude")).toBeUndefined();
			});
		});

		it("persists the override and can hand the decision back to the global flag", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();

				await store.setProgressTracking("claude", false);
				expect(store.getProgressTracking("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ progress_tracking: false }),
							}),
						}),
					}),
				);

				await store.setProgressTracking("claude", undefined);
				expect(store.getProgressTracking("claude")).toBeUndefined();
			});
		});
	});
});

// Catches: machine-owned hibernation fields being lost when unrelated agent settings save.
it("preserves and updates machine hibernation settings", async () => {
	const save = vi.fn().mockResolvedValue(undefined);
	const machine = createAgentConfigsStore({
		load: async () => ({ ...configWithClaude(), hibernate_idle_minutes: 90, hibernate_scope: "always" }),
		save,
	});
	await machine.hydrate();
	expect(machine.state.hibernate_idle_minutes).toBe(90);
	expect(machine.state.hibernate_scope).toBe("always");
	await machine.setHibernate(0, "background_only");
	expect(save).toHaveBeenLastCalledWith(
		expect.objectContaining({ hibernate_idle_minutes: 90 }),
		expect.objectContaining({
			hibernate_idle_minutes: 0,
			hibernate_scope: "background_only",
			agents: configWithClaude().agents,
		}),
	);
});

// Catches: editing module toggles drops saved additions/migration metadata or routes agent flags globally.
it("persists module switches and both addition layers without copying generated text", async () => {
	const save = vi.fn().mockResolvedValue(undefined);
	const machine = createAgentConfigsStore({
		load: async () => ({ agents: {}, launch_additions_migrated: true, launch_instructions: "GLOBAL" }),
		save,
	});
	await machine.hydrate();
	await machine.setLaunchModule("waiting", false);
	await machine.setLaunchModule("children", false, "ego");
	await machine.setAgentLaunchInstructions("ego", "AGENT");
	await machine.setLaunchInstructionsDisabled(true);
	const wire = save.mock.lastCall![1] as AgentsConfig;
	expect(wire.launch_instructions).toBe("GLOBAL");
	expect(wire.agents.ego.launch_instructions).toBe("AGENT");
	expect(wire.launch_modules).toEqual({ waiting: false });
	expect(wire.agents.ego.launch_modules).toEqual({ children: false });
	expect(wire.launch_instructions_disabled).toBe(true);
	expect(wire.launch_additions_migrated).toBe(true);
});
