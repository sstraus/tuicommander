import { describe, expect, it, vi } from "vitest";
import { type AgentType, HOOK_SUPPORT } from "../agents";
import { createAgentConfigsStore } from "../stores/agentConfigs";

describe("agent hook instrumentation toggle", () => {
	it("HOOK_SUPPORT gates only Claude and Gemini (A1)", () => {
		expect(HOOK_SUPPORT.claude).toBe(true);
		expect(HOOK_SUPPORT.gemini).toBe(true);
		const off: AgentType[] = ["codex", "grok", "opencode", "aider", "cursor", "amp", "goose", "droid", "git", "api"];
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
		expect(store.getHookInstrumentation("claude")).toBe(false);
		// The Tauri command owns persistence — sync must never write to disk.
		expect(save).not.toHaveBeenCalled();
	});
});
