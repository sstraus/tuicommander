import { createStore } from "solid-js/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { HttpRpcError } from "../../transport/http";

/**
 * The ticker must not name a vendor it has not seen. A Codex-only install polling
 * `get_claude_usage_api` at startup shows "Claude — no token", which is the
 * wrong-vendor reading the feature exists to prevent.
 */

const invoke = vi.fn();
vi.mock("../../invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const addMessage = vi.spyOn(statusBarTicker, "addMessage");
const removeMessage = vi.spyOn(statusBarTicker, "removeMessage");

vi.mock("../../stores/mdTabs", () => ({
	mdTabsStore: { addClaudeUsage: vi.fn(), addCodexUsage: vi.fn(), addGrokUsage: vi.fn() },
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

interface FakeTerminal {
	agentType: string | null;
	sessionId?: string | null;
}
const [terminals, setTerminals] = createStore<{
	activeId: string | null;
	terminals: Record<string, FakeTerminal>;
}>({ activeId: null, terminals: {} });

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		get state() {
			return terminals;
		},
	},
}));

const { destroyAgentUsage, initAgentUsage } = await import("../../features/agentUsage");

/** Let the Solid effect and the async poll settle. */
const settle = async () => {
	await Promise.resolve();
	await Promise.resolve();
};

describe("agent usage ticker — no vendor default", () => {
	beforeEach(() => {
		destroyAgentUsage();
		invoke.mockReset();
		invoke.mockResolvedValue({});
		statusBarTicker.clear();
		addMessage.mockClear();
		removeMessage.mockClear();
		setTerminals({ activeId: null, terminals: {} });
	});

	// The poll interval outlives the test otherwise.
	afterEach(() => {
		destroyAgentUsage();
		statusBarTicker.clear();
	});

	it("polls nothing and writes no ticker when no terminal is active", async () => {
		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("polls nothing when the active tab is a shell with no agent", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: null } } });

		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("polls nothing when the active tab is an agent with no usage API", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "opencode" } } });

		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("reads the ego terminal only — catches accidental Claude/Codex account fallback", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "ego", sessionId: "ego-one" } } });
		invoke.mockResolvedValue({
			text: "ctx: 14/128000 · openrouter key: USD 195.970819484 left",
			tooltip: "openrouter key: observed 2026-10-27T17:53:47.635+00:00; source: openrouter_key:data.limit_remaining",
		});
		initAgentUsage();
		await settle();
		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_ego_usage", { sessionId: "ego-one" });
		expect(statusBarTicker.getAll()).toEqual([
			expect.objectContaining({
				label: "ego",
				sessionId: "ego-one",
				text: "ctx: 14/128000 · openrouter key: USD 195.970819484 left",
				tooltip: "openrouter key: observed 2026-10-27T17:53:47.635+00:00; source: openrouter_key:data.limit_remaining",
				onClick: undefined,
			}),
		]);
	});

	it("clears missing ego data — catches unknown usage rendered as zero or stale provider data", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "ego", sessionId: "ego-one" } } });
		statusBarTicker.addMessage({
			id: "claude-usage:rate",
			pluginId: "claude-usage",
			label: "ego",
			sessionId: "ego-one",
			text: "old usage",
			priority: 0,
			ttlMs: 0,
		});
		invoke.mockResolvedValue({ text: null });
		initAgentUsage();
		await settle();
		expect(statusBarTicker.getAll()).toEqual([]);
	});

	it("never polls Claude on a Codex-only install", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "codex" } } });

		initAgentUsage();
		await settle();

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("get_codex_usage_api");
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Codex" }));
	});

	it("uses Grok's provider billing surface for a Grok tab", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "grok" } } });

		initAgentUsage();
		await settle();

		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_grok_usage_api");
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Grok" }));
	});

	it("starts polling when an agent tab becomes active after an empty start", async () => {
		initAgentUsage();
		await settle();
		expect(invoke).not.toHaveBeenCalled();

		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "claude-one" } } });
		await settle();

		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_usage_api", { sessionId: "claude-one" });
	});

	it("repolls when focus moves between Claude sessions and opens the matching dashboard", async () => {
		setTerminals({
			activeId: "t1",
			terminals: {
				t1: { agentType: "claude", sessionId: "private-session" },
				t2: { agentType: "claude", sessionId: "default-session" },
			},
		});
		initAgentUsage();
		await settle();
		expect(invoke).toHaveBeenCalledWith("get_claude_usage_api", { sessionId: "private-session" });

		setTerminals("activeId", "t2");
		await settle();
		expect(invoke).toHaveBeenLastCalledWith("get_claude_usage_api", { sessionId: "default-session" });
		expect(addMessage).toHaveBeenLastCalledWith(expect.objectContaining({ label: "Claude" }));
		const { mdTabsStore } = await import("../../stores/mdTabs");
		statusBarTicker.getAll()[0]?.onClick?.();
		expect(mdTabsStore.addClaudeUsage).toHaveBeenCalledWith("default-session");
	});

	it("shows neutral API key without a warning — catches API-key auth rendered as an expired token", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "api-session" } } });
		invoke.mockRejectedValueOnce(new Error("Claude API-key authentication"));
		const { appLogger } = await import("../../stores/appLogger");
		vi.mocked(appLogger.warn).mockClear();
		initAgentUsage();
		await settle();
		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_usage_api", { sessionId: "api-session" });
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Claude", text: "API key", priority: 0 }));
		expect(appLogger.warn).not.toHaveBeenCalled();
	});

	it.each([JSON.stringify({ error: "Claude API-key authentication" }), "Claude API-key authentication"])(
		"shows neutral API key over HTTP — catches browser and remote sessions rendered offline (%s)",
		async (body) => {
			setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "api-session" } } });
			invoke.mockRejectedValueOnce(new HttpRpcError("get_claude_usage_api", 500, body));
			const { appLogger } = await import("../../stores/appLogger");
			vi.mocked(appLogger.warn).mockClear();
			initAgentUsage();
			await settle();
			expect(statusBarTicker.getAll()[0]).toEqual(expect.objectContaining({ text: "API key", priority: 0 }));
			expect(appLogger.warn).not.toHaveBeenCalled();
		},
	);

	it("restores profile usage after an API-key tab — catches a sticky API-key label across focus changes", async () => {
		setTerminals({
			activeId: "t1",
			terminals: {
				t1: { agentType: "claude", sessionId: "api-session" },
				t2: { agentType: "claude", sessionId: "oauth-session" },
			},
		});
		invoke.mockRejectedValueOnce("Claude API-key authentication");
		invoke.mockResolvedValueOnce({
			five_hour: { utilization: 23, resets_at: null },
			seven_day: null,
			seven_day_oauth_apps: null,
			seven_day_opus: null,
			seven_day_sonnet: null,
			seven_day_cowork: null,
			extra_usage: null,
		});
		initAgentUsage();
		await settle();
		expect(statusBarTicker.getAll()[0]?.text).toBe("API key");
		setTerminals("activeId", "t2");
		await settle();
		expect(invoke).toHaveBeenLastCalledWith("get_claude_usage_api", { sessionId: "oauth-session" });
		expect(statusBarTicker.getAll()[0]?.text).toBe("5h: 23%");
	});

	it.each(["401 Invalid bearer token", new HttpRpcError("get_claude_usage_api", 401, "Unauthorized")])(
		"keeps OAuth failures distinct — catches API-key handling hiding expired subscription tokens (%s)",
		async (error) => {
			setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "oauth-session" } } });
			invoke.mockRejectedValueOnce(error);
			initAgentUsage();
			await settle();
			expect(statusBarTicker.getAll()[0]?.text).toBe("token expired");
		},
	);

	it("shows unknown when the focused Claude profile cannot be read", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "missing-profile" } } });
		invoke.mockRejectedValueOnce(new Error("Cannot read Claude session profile"));
		initAgentUsage();
		await settle();
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Claude", text: "unknown" }));
	});

	it("does not open default-account usage for a Claude tab without a backend session", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: null } } });
		initAgentUsage();
		await settle();
		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ text: "unknown", onClick: undefined }));
	});

	it("keeps the detected agent when the active tab moves to a shell", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "codex" } } });
		initAgentUsage();
		await settle();
		invoke.mockClear();

		setTerminals("activeId", null);
		await settle();

		// Sticky: no repoll, and nothing retracts the Codex number already shown.
		expect(invoke).not.toHaveBeenCalled();
		expect(removeMessage).not.toHaveBeenCalled();
	});
});
