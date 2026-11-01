import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Snapshot = { selectedSessionId?: string; servers: { servers: { url: string }[] } };
type Reply = { ok: boolean; result?: Snapshot; error?: string };
const PANEL = "chrome-extension://ext/panel.html";
type NativePayload = {
	action: string;
	sessionId?: string;
	requestId?: string;
	reply?: { ok: boolean; error?: string; result?: unknown };
};
const calls: NativePayload[] = [];
const offeredCommands: object[] = [];
let beforeCommand: (() => void) | undefined;
type Tab = { id: number; windowId: number; active: boolean; url: string };
const suspendWorkers: (() => void)[] = [];
const serversOf: Record<string, string[]> = { A: ["http://localhost:3001/"], B: ["http://localhost:3002/"] };

vi.mock("../../../browser-extension/design-mode/native.js", () => ({
	connectNative: () => {},
	nativeRequest: async (payload: NativePayload) => {
		calls.push(payload);
		if (payload.action === "command_next") {
			beforeCommand?.();
			beforeCommand = undefined;
			return offeredCommands.shift() ?? null;
		}
		if (payload.action === "command_reply") return { accepted: true };
		if (payload.action === "hello") return { endpoint: "test" };
		if (payload.action === "list_agents")
			return ["A", "B"].map((sessionId) => ({ sessionId, agentType: "claude", repoPath: "/repo", label: sessionId }));
		if (payload.action === "mode_status") return [{ repoPath: "/repo", sessionId: "A", status: "armed" }];
		if (payload.action === "list_dev_servers")
			return { servers: (serversOf[payload.sessionId ?? ""] ?? []).map((url) => ({ url })), canStart: false };
		throw new Error(`unexpected ${payload.action}`);
	},
}));

async function boot(
	historyUnavailable = false,
	tabs: Tab[] = [{ id: 7, windowId: 2, active: true, url: "http://localhost:5173/" }],
) {
	let listener: (m: unknown, s: unknown, r: (x: Reply) => void) => boolean | undefined = () => undefined;
	const store: Record<string, unknown> = {};
	const sessionStore: Record<string, unknown> = {};
	const tabsUpdate = vi.fn(async () => {});
	vi.stubGlobal("fetch", async () => ({ ok: true, json: async () => ({ repoPath: "/repo", sessionId: "A" }) }));
	vi.stubGlobal("chrome", {
		runtime: {
			getURL: (p: string) => `chrome-extension://ext/${p}`,
			sendMessage: async () => {},
			onSuspend: { addListener: (listener: () => void) => suspendWorkers.push(listener) },
			onMessage: {
				addListener: (fn: typeof listener) => {
					listener = fn;
				},
			},
		},
		sidePanel: { setPanelBehavior: () => {} },
		permissions: {
			getAll: async () => ({
				origins: ["http://localhost/*", "http://127.0.0.1/*", "http://[::1]/*", "https://localhost/*"],
			}),
			onAdded: { addListener: () => {} },
			onRemoved: { addListener: () => {} },
		},
		scripting: { getRegisteredContentScripts: async () => [] },
		storage: {
			session: {
				get: async () => ({ ...sessionStore }),
				set: async (v: Record<string, unknown>) => {
					if (historyUnavailable && "actionHistory" in v) throw new Error("history storage failed");
					Object.assign(sessionStore, v);
				},
			},
			local: {
				get: async () => ({ ...store }),
				set: async (v: Record<string, unknown>) => {
					Object.assign(store, v);
				},
			},
		},
		tabs: {
			query: async (query: { active?: boolean }) =>
				tabs.filter((tab) => !query.active || tab.active).map((tab) => ({ ...tab })),
			get: async (id: number) => {
				const tab = tabs.find((tab) => tab.id === id);
				if (!tab) throw new Error("No tab with id");
				return { ...tab };
			},
			captureVisibleTab: async () => "data:image/png;base64,iVBORw0KGgo=",
			sendMessage: async () => {},
			update: tabsUpdate,
			onActivated: { addListener: () => {} },
			onUpdated: { addListener: () => {} },
			onRemoved: { addListener: () => {} },
		},
	});
	// @ts-expect-error plain MV3 module without declarations
	await import("../../../browser-extension/design-mode/worker.js");
	const send = (message: object) => new Promise<Reply>((resolve) => listener(message, { url: PANEL }, resolve));
	return { send, tabsUpdate };
}

describe("Design Mode worker dev-server wiring", () => {
	beforeEach(() => {
		vi.resetModules();
		vi.useFakeTimers();
		calls.length = 0;
		offeredCommands.length = 0;
		beforeCommand = undefined;
	});

	afterEach(async () => {
		for (const suspend of suspendWorkers.splice(0)) suspend();
		await vi.advanceTimersByTimeAsync(1000);
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	// Catches: the command receiver depending on a sidebar request or a later heartbeat to start.
	it("starts the native command receiver when the worker boots without a sidebar", async () => {
		await boot();
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.some((call) => call.action === "command_next")).toBe(true);
	});

	// Catches: commands requiring a sidebar-selected tab and grant denial disappearing instead of replying.
	it("acknowledges the launched page and replies to grant-free evidence and denied eval without a sidebar", async () => {
		offeredCommands.push(
			{ requestId: "ready", sessionId: "A", action: "ready", args: { url: "http://localhost:5173/" } },
			{ requestId: "eval", sessionId: "A", action: "eval", args: { script: "1+1" } },
			{ requestId: "screenshot", sessionId: "A", action: "screenshot" },
			{ requestId: "errors", sessionId: "A", action: "errors" },
		);
		await boot();
		await vi.advanceTimersByTimeAsync(0);
		const replies = calls.filter((call) => call.action === "command_reply");
		expect(replies.map((call) => call.requestId)).toEqual(["ready", "eval", "screenshot", "errors"]);
		expect(replies[0].reply).toEqual({ ok: true, result: { tabId: 7, windowId: 2 } });
		expect(replies[1].reply).toEqual({
			ok: false,
			error: "eval_not_granted: grant needed: click Allow eval on this host in the Design Mode sidebar.",
		});
		expect(replies[2].reply).toEqual({
			ok: true,
			result: { image: "data:image/png;base64,iVBORw0KGgo=", url: "http://localhost:5173/" },
		});
		expect(replies[3].reply).toEqual({ ok: true, result: { errors: [] } });
	});

	// Catches: URL equality rejecting a launch tab that redirected before startup acknowledgment.
	it("acknowledges the launch tab after it redirects before ready", async () => {
		const tabs = [{ id: 7, windowId: 2, active: true, url: "http://localhost:5173/" }];
		beforeCommand = () => {
			tabs[0].url = "http://localhost:5173/login";
		};
		offeredCommands.push({
			requestId: "ready",
			sessionId: "A",
			action: "ready",
			args: { url: "http://localhost:5173/" },
		});
		await boot(false, tabs);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.find((call) => call.action === "command_reply")?.reply).toEqual({
			ok: true,
			result: { tabId: 7, windowId: 2 },
		});
	});

	// Catches: a different tab at the original URL stealing readiness after the launch tab redirects.
	it("does not substitute a same-URL sibling for the redirected launch tab", async () => {
		const tabs = [
			{ id: 7, windowId: 2, active: true, url: "http://localhost:5173/" },
			{ id: 8, windowId: 2, active: false, url: "http://localhost:5173/" },
		];
		beforeCommand = () => {
			tabs[0].url = "http://localhost:5173/login";
		};
		offeredCommands.push({
			requestId: "ready",
			sessionId: "A",
			action: "ready",
			args: { url: "http://localhost:5173/" },
		});
		await boot(false, tabs);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.find((call) => call.action === "command_reply")?.reply).toEqual({
			ok: true,
			result: { tabId: 7, windowId: 2 },
		});
	});

	// Catches: a same-URL sibling acknowledging startup when the launch tab no longer exists.
	it("rejects readiness when the launch tab closed despite a same-URL sibling", async () => {
		const tabs = [
			{ id: 7, windowId: 2, active: true, url: "http://localhost:5173/" },
			{ id: 8, windowId: 2, active: false, url: "http://localhost:5173/" },
		];
		beforeCommand = () => {
			tabs.shift();
		};
		offeredCommands.push({
			requestId: "ready",
			sessionId: "A",
			action: "ready",
			args: { url: "http://localhost:5173/" },
		});
		await boot(false, tabs);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.find((call) => call.action === "command_reply")?.reply?.ok).toBe(false);
	});

	// Catches: an optional action-history write swallowing an otherwise completed command reply.
	it("delivers command replies even when action history cannot be saved", async () => {
		offeredCommands.push(
			{ requestId: "ready", sessionId: "A", action: "ready", args: { url: "http://localhost:5173/" } },
			{ requestId: "errors", sessionId: "A", action: "errors" },
		);
		await boot(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.filter((call) => call.action === "command_reply").map((call) => call.requestId)).toEqual([
			"ready",
			"errors",
		]);
	});

	// Catches: an old session's queued command controlling the newly bound agent's page.
	it("replies with not_armed instead of executing a command for a different session", async () => {
		offeredCommands.push({
			requestId: "old",
			sessionId: "B",
			action: "ready",
			args: { url: "http://localhost:5173/" },
		});
		await boot();
		await vi.advanceTimersByTimeAsync(0);
		expect(calls.find((call) => call.action === "command_reply")?.reply).toEqual({
			ok: false,
			error: "not_armed: Design Mode is not armed for this session.",
		});
	});

	// Catches: the dropdown pick changing the recipient while the list stays on the launching
	// session (or waits for the 5 s refresh throttle), so the panel keeps showing the old servers.
	it("shows the servers of the agent picked in the dropdown at once", async () => {
		const { send } = await boot();
		const first = await send({ kind: "snapshot" });
		expect(first.result?.servers.servers.map((s: { url: string }) => s.url)).toEqual(serversOf.A);

		const picked = await send({ kind: "select", tabId: 7, sessionId: "B" });
		expect(picked.result?.selectedSessionId).toBe("B");
		expect(picked.result?.servers.servers.map((s: { url: string }) => s.url)).toEqual(serversOf.B);
		expect(calls.filter((c) => c.action === "list_dev_servers").map((c) => c.sessionId)).toEqual(["A", "B"]);
	});

	// Catches: open_server validating against another session's list, so a server of the
	// launching agent can still be opened after the pick moved to a different agent.
	it("refuses to open a server that is not in the selected agent's list", async () => {
		const { send, tabsUpdate } = await boot();
		await send({ kind: "snapshot" });
		await send({ kind: "select", tabId: 7, sessionId: "B" });
		const stale = await send({ kind: "open_server", url: serversOf.A[0] });
		expect(stale.ok).toBe(false);
		expect(tabsUpdate).not.toHaveBeenCalled();
		const fresh = await send({ kind: "open_server", url: serversOf.B[0] });
		expect(fresh.ok).toBe(true);
		expect(tabsUpdate).toHaveBeenCalledWith(7, { url: serversOf.B[0] });
	});
});
