import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { countAgentSessions, destroyAgentActivity, initAgentActivity } from "../../features/agentActivity";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { terminalsStore } from "../../stores/terminals";

describe("countAgentSessions", () => {
	it("counts working and idle agent sessions only", () => {
		const counts = countAgentSessions([
			{ agentType: "claude", agentState: "working" },
			{ agentType: "claude", agentState: "starting" },
			{ agentType: "codex", agentState: "idle" },
			{ agentType: "codex", agentState: "completed" },
			{ agentType: "claude", agentState: "awaiting_input" },
			{ agentType: null, agentState: "working" }, // a plain shell is not an agent
			{ agentType: "claude", agentState: null },
		]);
		expect(counts).toEqual({ working: 2, idle: 2 });
	});
});

describe("agent activity ticker", () => {
	beforeAll(() => activityStore.hydrate());
	const ids: string[] = [];
	const addAgent = (agentState: "working" | "idle") => {
		const id = terminalsStore.add({
			sessionId: `s-${ids.length}`,
			fontSize: 14,
			name: "t",
			cwd: "/tmp",
			awaitingInput: null,
			agentType: "claude",
		});
		terminalsStore.update(id, { agentState });
		ids.push(id);
		return id;
	};
	const text = () => statusBarTicker.getAll().find((m) => m.pluginId === "agent-activity")?.text;

	beforeEach(() => {
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		statusBarTicker.clear();
	});
	afterEach(() => {
		destroyAgentActivity();
		for (const id of ids.splice(0)) terminalsStore.remove(id);
		statusBarTicker.clear();
		vi.restoreAllMocks();
	});

	it("shows the counts and follows a session changing state — catches a ticker that is not reactive", () => {
		addAgent("working");
		const second = addAgent("idle");
		initAgentActivity();
		expect(text()).toBe("1 working · 1 idle");

		terminalsStore.update(second, { agentState: "working" });
		expect(text()).toBe("2 working · 0 idle");
	});

	it("disappears when no agent session is left — catches a stale '0 working · 0 idle'", () => {
		const id = addAgent("working");
		initAgentActivity();
		expect(text()).toBe("1 working · 0 idle");
		terminalsStore.remove(id);
		ids.splice(ids.indexOf(id), 1);
		expect(text()).toBeUndefined();
	});
});
