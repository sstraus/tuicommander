import { describe, expect, it, vi } from "vitest";
import { AGENT_TYPES, type AgentType, HOOK_SUPPORT } from "../agents";
import { createAgentConfigsStore } from "../stores/agentConfigs";

describe("agent hook instrumentation toggle", () => {
	it("HOOK_SUPPORT gates the hook-instrumented agents (A1)", () => {
		// Adapters landed incrementally: Claude/Gemini (#048), then Grok (#051),
		// Codex (#050), OpenCode (#052) flipped on. Keep this in sync with agents.ts.
		const on: AgentType[] = ["claude", "gemini", "grok", "codex", "opencode", "pi"];
		for (const a of on) {
			expect(HOOK_SUPPORT[a]).toBe(true);
		}
		const off: AgentType[] = ["cursor", "amp", "goose", "droid", "ego", "git", "api"];
		for (const a of off) {
			expect(HOOK_SUPPORT[a]).toBe(false);
		}
	});

	it("getHookInstrumentation reflects the stored flag", () => {
		const store = createAgentConfigsStore({
			load: async () => ({ agents: { claude: { run_configs: [], hook_instrumentation: true } } }),
			save: vi.fn(),
		});
		// Seed via sync (load is async and not awaited here) then read back.
		store.syncHookInstrumentation("claude", true);
		expect(store.getHookInstrumentation("claude")).toBe(true);
		expect(store.getHookInstrumentation("gemini")).toBeUndefined();
	});

	it("syncHookInstrumentation mirrors the flag in memory without saving to disk", () => {
		const save = vi.fn();
		const store = createAgentConfigsStore({
			load: async () => ({ agents: {} }),
			save,
		});
		store.syncHookInstrumentation("claude", true);
		expect(store.getHookInstrumentation("claude")).toBe(true);
		store.syncHookInstrumentation("claude", false);
		// Off is the default, and the backend stores it as absent; the mirror matches.
		expect(store.getHookInstrumentation("claude")).toBeUndefined();
		// The Tauri command owns persistence — sync must never write to disk.
		expect(save).not.toHaveBeenCalled();
	});

	// Catches: a later save persists default hook flags as explicit overrides.
	it("a later save writes no explicit default for either flag", async () => {
		// The backend stores the default as absent. A whole-file save from the
		// store must not write it back, or Settings reads it as a user override.
		const save = vi.fn();
		const store = createAgentConfigsStore({
			load: async () => ({
				agents: { claude: { run_configs: [], hook_instrumentation: true, native_status_signals: false } },
			}),
			save,
		});
		await store.hydrate();
		store.syncHookInstrumentation("claude", false);
		store.syncNativeStatusSignals("claude", true);
		expect(store.getNativeStatusSignals("claude")).toBe(true);

		await store.setAutoRetry("claude", true);
		const saved = save.mock.calls[0][1].agents.claude;
		expect(saved).not.toHaveProperty("hook_instrumentation");
		expect(saved).not.toHaveProperty("native_status_signals");
		expect(saved.auto_retry_on_error).toBe(true);
	});

	it("keeps a non-default flag as an explicit override", () => {
		const store = createAgentConfigsStore({ load: async () => ({ agents: {} }), save: vi.fn() });
		store.syncHookInstrumentation("gemini", true);
		store.syncNativeStatusSignals("claude", false);
		expect(store.state.agents.gemini.hook_instrumentation).toBe(true);
		expect(store.state.agents.claude.native_status_signals).toBe(false);
	});

	// Catches: a saved scrollback opt-out is dropped or the default is persisted as an override.
	it("defaults native scrollback prevention on for every agent and persists opt-out", async () => {
		const save = vi.fn();
		const store = createAgentConfigsStore({ load: async () => ({ agents: {} }), save });
		await store.hydrate();
		for (const agent of AGENT_TYPES) {
			expect(store.getPreventAltScreen(agent)).toBe(true);
		}
		await store.setPreventAltScreen("codex", false);
		expect(store.getPreventAltScreen("codex")).toBe(false);
		expect(save.mock.lastCall?.[1].agents.codex.prevent_alt_screen).toBe(false);
		await store.setPreventAltScreen("codex", true);
		expect(save.mock.lastCall?.[1].agents.codex).not.toHaveProperty("prevent_alt_screen");
	});
});
