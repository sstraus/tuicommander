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
const { initAgentUsage, destroyAgentUsage, usagePlacement, formatAgentUsage } = await import(
	"../../features/agentUsage"
);
const key = { owner: "claude-code", provider: "anthropic", credential: "/one" };
const view = (session: string, percent: number): QuotaView => ({
	key: { ...key, credential: `/${session}` },
	session_ids: [session],
	state: "fresh",
	snapshot: {
		windows: [
			{ kind: "weekly", label: "7d", used_percent: percent, resets_at: null, window_seconds: 604800, model: null },
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
	vi.restoreAllMocks();
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
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 21%");
	invoke.mockClear();
	setTerminals("activeId", "b");
	await Promise.resolve();
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 42%");
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
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 21%");
	handlers.get("provider-quota-changed")?.({ payload: view("first", 55) });
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 55%");
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

// Catches the badge blinking for a window it does not show, or not blinking for the one it shows.
it("quota_priority_follows_the_shown_weekly_window", async () => {
	const quota = view("first", 97);
	quota.snapshot!.windows.push({
		kind: "weekly",
		label: "7d",
		used_percent: 99,
		resets_at: null,
		window_seconds: 604800,
		model: "opus",
	});
	invoke.mockResolvedValue([quota]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]).toMatchObject({ text: "7d: 97%", priority: 96 });
	handlers.get("provider-quota-changed")?.({
		payload: {
			...quota,
			snapshot: {
				...quota.snapshot!,
				windows: [{ ...quota.snapshot!.windows[0], used_percent: 20 }, quota.snapshot!.windows[1]],
			},
		},
	});
	expect(statusBarTicker.getAll()[0]).toMatchObject({ text: "7d: 20%", priority: 10 });
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
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 21%");
	setTerminals("activeId", "shell");
	expect(statusBarTicker.getAll()).toEqual([]);
});

// Catches the ego label "ego" never matching the badge, and a producer message the badge rule rejects.
it.each([
	["ego", "openai-codex", "Codex"],
	["ego", "anthropic", "Claude"],
	["claude", "anthropic", "Claude"],
	["codex", "openai-codex", "Codex"],
])("%s_session_bound_to_%s_reaches_the_agent_badge_as_%s", async (agentType, provider, label) => {
	const bound = view("pty", 33);
	bound.key = {
		owner: agentType === "ego" ? "ego" : agentType === "claude" ? "claude-code" : "codex-cli",
		provider,
		credential: "/home",
	};
	invoke.mockResolvedValue([bound]);
	const active = { agentType, sessionId: "pty" };
	setTerminals({ activeId: "a", terminals: { a: active } });
	initAgentUsage();
	await settle();
	const [message] = statusBarTicker.getAll();
	expect(message).toMatchObject({ label, text: "7d: 33%" });
	expect(usagePlacement(message, active)).toBe("badge");
	expect(usagePlacement(message, { agentType, sessionId: "other" })).toBe("nowhere");
});

// Snapshot shapes recorded from GET /provider-quotas on 2026-11-15 (credentials removed).
const NOW = Date.parse("2026-11-15T12:00:00Z");
const weekly = (used: number, resets_at: string | null) => ({
	kind: "weekly",
	label: "7d",
	used_percent: used,
	resets_at,
	window_seconds: 604800,
	model: null,
});
const snap = (over: Partial<NonNullable<QuotaView["snapshot"]>>): QuotaView["snapshot"] => ({
	windows: [],
	plan: null,
	credits: null,
	account: null,
	observed_at_ms: 1,
	source: "active",
	...over,
});

const claudeExtraUsage = {
	provider: "claude" as const,
	observed_at_ms: 1,
	response: { extra_usage: { is_enabled: false, monthly_limit: 10000, used_credits: 133, resets_at: null } },
};

// Catches: the Claude pay-as-you-go overage budget (extra_usage) shown as a monthly limit.
it("claude_extra_usage_is_not_a_monthly_limit", () => {
	const withWeekly = snap({ windows: [weekly(79, "2026-11-19T17:00:00Z")], detail: claudeExtraUsage });
	expect(formatAgentUsage(withWeekly, NOW)?.text).toBe("7d: 79% -4d");
	expect(formatAgentUsage(snap({ detail: claudeExtraUsage }), NOW)).toBeNull();
});

// Catches: reset time dropped from the text (regression of d4529cc706), weekly shown when absent,
// monthly remaining not shown, and segments other than weekly/monthly leaking into the badge.
it.each([
	[
		"codex credits",
		snap({ windows: [weekly(32, "2026-11-18T09:01:43Z")], credits: "8418.4673090000" }),
		"7d: 32% -2d · credits: 8418 left",
	],
	["weekly only (ego anthropic)", snap({ windows: [weekly(78, "2026-11-19T17:00:00Z")] }), "7d: 78% -4d"],
	[
		"no weekly, monthly window",
		snap({
			windows: [
				{ kind: "five_hour", label: "5h", used_percent: 50, resets_at: null, window_seconds: 18000, model: null },
				{
					kind: "monthly",
					label: "30d",
					used_percent: 40,
					resets_at: "2026-11-15T17:30:00Z",
					window_seconds: 2592000,
					model: null,
				},
			],
		}),
		"30d: 60% left -5h",
	],
	["past and missing resets", snap({ windows: [weekly(10, "2026-11-15T11:00:00Z")], credits: null }), "7d: 10%"],
	["minutes", snap({ windows: [weekly(10, "2026-11-15T12:12:00Z")] }), "7d: 10% -12m"],
])("badge_text_%s", (_name, snapshot, text) => {
	expect(formatAgentUsage(snapshot, NOW)?.text).toBe(text);
});

it("badge_tooltip_keeps_the_full_reset_date", () => {
	const formatted = formatAgentUsage(snap({ windows: [weekly(78, "2026-11-19T17:00:00Z")] }), NOW);
	expect(formatted?.tooltip).toMatch(/^7d: 78%, resets .*Nov/);
});

// Catches: ego display depending on a native Codex binding (codex-cli key, Codex CLI dashboard).
it("ego_codex_usage_needs_no_codex_cli_binding", async () => {
	const egoCodex: QuotaView = {
		key: { owner: "ego", provider: "openai-codex", credential: "/ego-home" },
		session_ids: ["ego-pty"],
		state: "fresh",
		snapshot: snap({ windows: [weekly(32, null)], credits: "8418.4673090000" }),
	};
	invoke.mockResolvedValue([egoCodex]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "ego", sessionId: "ego-pty" } } });
	initAgentUsage();
	await settle();
	const [message] = statusBarTicker.getAll();
	expect(message).toMatchObject({ label: "Codex", text: "7d: 32% · credits: 8418 left" });
	expect(message.onClick).toBeUndefined();
	expect(invoke.mock.calls.map(([command]) => command)).toEqual(["get_provider_quotas"]);
});

// Catches: remove/add churn on every effect run (a same-value republish or another account's
// push rewrote the ticker store, repainting the badge).
it("same_value_refreshes_do_not_write_the_ticker_store", async () => {
	invoke.mockResolvedValue([view("first", 21), view("second", 42)]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	const add = vi.spyOn(statusBarTicker, "addMessage");
	const remove = vi.spyOn(statusBarTicker, "removeMessage");
	for (let i = 0; i < 10; i++) {
		handlers.get("provider-quota-changed")?.({ payload: view("first", 21) });
		handlers.get("provider-quota-changed")?.({ payload: view("second", 40 + i) });
	}
	expect(add).not.toHaveBeenCalled();
	expect(remove).not.toHaveBeenCalled();
	handlers.get("provider-quota-changed")?.({ payload: view("first", 22) });
	expect(add).toHaveBeenCalledOnce();
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 22%");
});

// Catches: an empty flash (remove then add) when switching to a session that already has a view.
it("switching_between_bound_sessions_never_removes_the_message", async () => {
	const codex = view("second", 42);
	codex.key = { owner: "codex-cli", provider: "openai-codex", credential: "/second" };
	invoke.mockResolvedValue([view("first", 21), codex]);
	setTerminals({
		activeId: "a",
		terminals: { a: { agentType: "claude", sessionId: "first" }, b: { agentType: "codex", sessionId: "second" } },
	});
	initAgentUsage();
	await settle();
	const remove = vi.spyOn(statusBarTicker, "removeMessage");
	setTerminals("activeId", "b");
	setTerminals("activeId", "a");
	expect(remove).not.toHaveBeenCalled();
	expect(statusBarTicker.getAll()).toHaveLength(1);
});

// Catches: the badge width jumping to a state word while a backoff has momentarily no snapshot.
it("backoff_without_snapshot_keeps_the_last_good_text", async () => {
	invoke.mockResolvedValue([view("first", 21)]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "claude", sessionId: "first" } } });
	initAgentUsage();
	await settle();
	handlers.get("provider-quota-changed")?.({ payload: { ...view("first", 21), state: "backoff", snapshot: null } });
	expect(statusBarTicker.getAll()[0]?.text).toBe("7d: 21%");
	handlers.get("provider-quota-changed")?.({
		payload: { ...view("first", 21), state: "auth_invalid", snapshot: null },
	});
	expect(statusBarTicker.getAll()[0]?.text).toBe("auth invalid");
});

// Catches: a session still bound to the previous agent's account combining Codex text
// with a Claude icon and dashboard action (the registry reconciles independently).
it.each([
	["claude", "ego", "openai-codex"],
	["claude", "claude-code", "openai-codex"],
	["codex", "claude-code", "anthropic"],
	["ego", "codex-cli", "openai-codex"],
])("%s_rejects_mismatched_binding_%s_%s", async (agentType, owner, provider) => {
	const mismatched = view("pty", 33);
	mismatched.key = { owner, provider, credential: "/wrong-account" };
	mismatched.snapshot!.credits = "8418.4673090000";
	invoke.mockResolvedValue([mismatched]);
	setTerminals({ activeId: "a", terminals: { a: { agentType, sessionId: "pty" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()).toEqual([]);
});

// Catches: an agent transition on the same PTY retaining the previous account until
// the asynchronous binding event arrives, including last-good backoff text.
it("agent_transition_retracts_previous_account_until_matching_binding_arrives", async () => {
	const previous = view("pty", 33);
	previous.key = { owner: "ego", provider: "openai-codex", credential: "/ego" };
	previous.snapshot!.credits = "8418.4673090000";
	invoke.mockResolvedValue([previous]);
	setTerminals({ activeId: "a", terminals: { a: { agentType: "ego", sessionId: "pty" } } });
	initAgentUsage();
	await settle();
	expect(statusBarTicker.getAll()[0]?.text).toContain("8418 left");
	setTerminals("terminals", "a", "agentType", "claude");
	expect(statusBarTicker.getAll()).toEqual([]);
	handlers.get("provider-quota-changed")?.({ payload: { ...previous, state: "backoff", snapshot: null } });
	expect(statusBarTicker.getAll()).toEqual([]);
	const current = view("pty", 79);
	handlers.get("provider-quota-changed")?.({ payload: current });
	handlers.get("provider-quota-binding-changed")?.({ payload: { session_id: "pty", key: current.key } });
	expect(statusBarTicker.getAll()[0]).toMatchObject({ label: "Claude", sessionId: "pty", text: "7d: 79%" });
	const { mdTabsStore } = await import("../../stores/mdTabs");
	statusBarTicker.getAll()[0]?.onClick?.();
	expect(mdTabsStore.addClaudeUsage).toHaveBeenCalledWith("pty");
});

// Catches: native dashboard clicks dropping the session that supplied the badge.
it.each([
	["claude", "claude-code", "anthropic", "addClaudeUsage"],
	["codex", "codex-cli", "openai-codex", "addCodexUsage"],
	["grok", "grok-cli", "xai", "addGrokUsage"],
] as const)("%s_dashboard_click_keeps_the_bound_session", async (agentType, owner, provider, action) => {
	const bound = view("private-session", 42);
	bound.key = { owner, provider, credential: "/private" };
	invoke.mockResolvedValue([bound]);
	setTerminals({ activeId: "a", terminals: { a: { agentType, sessionId: "private-session" } } });
	initAgentUsage();
	await settle();
	const { mdTabsStore } = await import("../../stores/mdTabs");
	vi.mocked(mdTabsStore[action]).mockClear();
	statusBarTicker.getAll()[0]?.onClick?.();
	expect(mdTabsStore[action]).toHaveBeenCalledWith("private-session");
});
