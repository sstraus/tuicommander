import { createStore } from "solid-js/store";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { providerQuotaStore, type QuotaView } from "../../stores/providerQuota";
import { statusBarTicker } from "../../stores/statusBarTicker";

const handlers = new Map<string, (event: { payload: unknown }) => void>();
const invoke = vi.fn();
vi.mock("../../invoke", () => ({
	invoke: (...args: unknown[]) => invoke(...args),
	listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
		handlers.set(name, handler);
		return () => handlers.delete(name);
	},
}));
vi.mock("../../stores/mdTabs", () => ({
	mdTabsStore: { addClaudeUsage: vi.fn(), addCodexUsage: vi.fn(), addGrokUsage: vi.fn() },
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));
const [terminals, setTerminals] = createStore<{
	activeId: string | null;
	terminals: Record<string, { agentType: string | null; sessionId?: string | null }>;
}>({ activeId: null, terminals: {} });
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		get state() {
			return terminals;
		},
	},
}));
const { initAgentUsage, destroyAgentUsage } = await import("../../features/agentUsage");
const key = { owner: "claude-code", provider: "anthropic", credential: "/one" };
const view = (session: string, percent: number): QuotaView => ({
	key: { ...key, credential: `/${session}` },
	session_ids: [session],
	state: "fresh",
	snapshot: {
		windows: [
			{ kind: "five_hour", label: "5h", used_percent: percent, resets_at: null, window_seconds: 18000, model: null },
		],
		plan: "pro",
		credits: null,
		account: null,
		observed_at_ms: 1,
		source: "passive",
	},
});
const settle = async () => {
	await providerQuotaStore.start();
	await Promise.resolve();
};
beforeEach(() => {
	destroyAgentUsage();
	providerQuotaStore.stop();
	handlers.clear();
	invoke.mockReset();
	invoke.mockResolvedValue([]);
	setTerminals({ activeId: null, terminals: {} });
	statusBarTicker.clear();
});
afterEach(() => {
	destroyAgentUsage();
	providerQuotaStore.stop();
	statusBarTicker.clear();
	vi.useRealTimers();
});

// Catches the original provider read on every switch, and the wrong cached account winning.
it("tab_switch_between_bound_sessions_does_not_fetch", async () => {
	invoke.mockResolvedValue([view("first", 21), view("second", 42)]);
	setTerminals({
		activeId: "a",
		terminals: { a: { agentType: "claude", sessionId: "first" }, b: { agentType: "claude", sessionId: "second" } },
	});
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 21%");
	invoke.mockClear();
	setTerminals("activeId", "b");
	await Promise.resolve();
	expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 42%");
	expect(invoke).not.toHaveBeenCalled();
	const { mdTabsStore } = await import("../../stores/mdTabs");
	statusBarTicker.getAll()[0]?.onClick?.();
	expect(mdTabsStore.addClaudeUsage).toHaveBeenCalledWith("second");
});

// Catches retaining one account's headroom on an unbound agent session.
it.each(["ego", "claude"])("unbound_%s_does_not_borrow_a_cli_account", async (agent) => {
	invoke.mockResolvedValue([view("claude", 21)]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: agent, sessionId: "ego" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()).toEqual([]);
	expect(invoke.mock.calls).toEqual([["get_provider_quotas"]]);
});

// Catches restoring the deleted periodic provider polling path.
it("clock_ticks_do_not_read_the_network", async () => {
	vi.useFakeTimers();
	invoke.mockResolvedValue([view("first", 21)]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	invoke.mockClear();
	await vi.advanceTimersByTimeAsync(600000);
	expect(invoke).not.toHaveBeenCalled();
});

// Catches a push for an inactive account replacing the active account's ticker.
it("inactive_quota_push_does_not_replace_the_active_account", async () => {
	invoke.mockResolvedValue([view("first", 21), view("second", 42)]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	handlers.get("provider-quota-changed")?.({ payload: view("second", 99) });
	expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 21%");
	handlers.get("provider-quota-changed")?.({ payload: view("first", 55) });
	expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 55%");
});

// Catches showing an authentication failure as zero usage.
it("auth_failure_is_not_zero_usage", async () => {
	invoke.mockResolvedValue([{ ...view("first", 21), state: "auth_invalid", snapshot: null }]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]?.text).toBe("auth invalid");
});

// Catches a quota-clear effect subscribing to ticker writes and cycling with the activity ticker.
it("unbound_session_does_not_subscribe_to_other_ticker_messages", async () => {
	setTerminals({ activeId: "a", terminals: { a: { agentType: "ego", sessionId: "unbound" } } });
	const remove = vi.spyOn(statusBarTicker, "removeMessage");
	initAgentUsage();
	await settle();
	const before = remove.mock.calls.length;
	statusBarTicker.addMessage({ id: "activity", pluginId: "agent-activity", text: "idle", priority: 10, ttlMs: 0 });
	await Promise.resolve();
	expect(remove.mock.calls.length).toBe(before);
	remove.mockRestore();
});

// Catches shared rendering dropping warning/critical priority when the backend changes its window shape.
it("quota_priority_uses_the_most_constrained_window", async () => {
	const quota = view("first", 21);
	quota.snapshot!.windows.push({
		kind: "weekly",
		label: "7d",
		used_percent: 97,
		resets_at: null,
		window_seconds: 604800,
		model: "model",
	});
	invoke.mockResolvedValue([quota]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]?.priority).toBe(96);
});

// Catches a shell tab retaining the previous agent account's quota.
it("switching_to_a_shell_retracts_the_previous_quota", async () => {
	invoke.mockResolvedValue([view("first", 21)]);
	setTerminals({
		activeId: "a",
		terminals: { a: { agentType: "claude", sessionId: "first" }, shell: { agentType: null } },
	});
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 21%");
	setTerminals("activeId", "shell");
	expect(statusBarTicker.getAll()).toEqual([]);
});
