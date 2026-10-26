import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { createSession, resize, sendCommand, mockRpc, ptyOptions, mockVerifyResume } = vi.hoisted(() => ({
	createSession: vi.fn().mockResolvedValue("fresh-session"),
	resize: vi.fn().mockResolvedValue(undefined),
	sendCommand: vi.fn().mockResolvedValue(undefined),
	mockRpc: vi.fn(),
	ptyOptions: new Map<
		string,
		{
			onStateChange?: (state: Record<string, unknown>) => void;
			onParsed?: (frame: { type: string; event: unknown }) => void;
		}
	>(),
	mockVerifyResume: vi.fn(),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		createSession,
		resize,
		sendCommand,
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: (...args: unknown[]) => mockRpc(...args),
	subscribePty: (
		sessionId: string,
		_onData: unknown,
		_onExit: unknown,
		options: typeof ptyOptions extends Map<string, infer V> ? V : never,
	) => {
		ptyOptions.set(sessionId, options);
		return Promise.resolve(() => {});
	},
}));
// The mounted tab reconnects to a live PTY and asks which process runs in the foreground.
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../utils/agentSession", () => ({
	verifyAndBuildResumeCommand: (...args: unknown[]) => mockVerifyResume(...args),
}));
vi.mock("../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({
		getEnvFlags: () => ({}),
		isAutoRetryEnabled: () => true,
		getIntentTabTitle: () => true,
	}),
}));
vi.mock("../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));

import { Terminal } from "../../components/Terminal/Terminal";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";

let mounted: { unmount: () => void } | undefined;
function openIdleAgentTab() {
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/Gits/alpha",
		repoPath: "/Gits/alpha",
		name: "agent",
		fontSize: 13,
		awaitingInput: null,
	});
	terminalsStore.update(id, {
		agentType: "claude",
		agentSessionId: "agent-uuid",
		tuicSession: "tab-uuid",
		shellState: "idle",
		agentState: "idle",
	});
	const oldRaf = globalThis.requestAnimationFrame;
	const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
	const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 600 });
	globalThis.requestAnimationFrame = (cb) => {
		cb(0);
		return 1;
	};
	try {
		mounted = render(() => <Terminal id={id} cwd="/Gits/alpha" alwaysVisible />);
	} finally {
		globalThis.requestAnimationFrame = oldRaf;
		if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
		if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
	}
	return id;
}

const emit = (event: Record<string, unknown>) => ptyOptions.get("live-session")?.onParsed?.({ type: "parsed", event });
const failed = () =>
	emit({
		type: "api-error",
		pattern_name: "claude-transport-error",
		matched_text: "Connection lost mid-response.",
		error_kind: "server",
	});
const snapshot = (shell: string, error?: string) =>
	ptyOptions.get("live-session")?.onStateChange?.({
		agent_type: "claude",
		agent_state: shell === "busy" ? "working" : "completed",
		shell_state: shell,
		last_error: error,
	});

describe("Terminal API failure recovery 2335", () => {
	let id: string;
	beforeEach(async () => {
		// The dynamic event import can load the external core.js, bypassing the
		// static core module mock. Supply its actual WebView boundary as well.
		vi.stubGlobal("__TAURI_INTERNALS__", {
			transformCallback: () => 0,
			invoke: async (cmd: string) => {
				if (cmd === "plugin:event|listen") return 1;
				if (cmd === "plugin:event|unlisten") return;
				throw new Error(`Unexpected native event command: ${cmd}`);
			},
		});
		vi.stubGlobal("__TAURI_EVENT_PLUGIN_INTERNALS__", { unregisterListener: () => {} });
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		vi.spyOn(appLogger, "error").mockImplementation(() => {});
		vi.spyOn(appLogger.diag, "info").mockImplementation(() => {});
		createSession.mockClear().mockResolvedValue("fresh-session");
		resize.mockClear().mockResolvedValue(undefined);
		sendCommand.mockClear().mockResolvedValue(undefined);
		mockRpc.mockReset().mockResolvedValue(undefined);
		ptyOptions.clear();
		id = openIdleAgentTab();
		// The eager subscription exists before resize/listener setup completes.
		// Wait for the reconnect receipt before advancing the retry clock.
		await waitFor(() => {
			expect(vi.mocked(appLogger.error).mock.calls).toEqual([]);
			expect(appLogger.info).toHaveBeenCalledWith("terminal", `initSession(${id}) — reconnected to live-session`);
		});
		expect(createSession).not.toHaveBeenCalled();
		expect(terminalsStore.get(id)?.sessionId).toBe("live-session");
		vi.useFakeTimers();
	});
	afterEach(() => {
		mounted?.unmount();
		terminalsStore.remove(id);
		vi.useRealTimers();
		// These scenarios deliberately report provider failures and retry exhaustion.
		for (const [source, message] of vi.mocked(appLogger.error).mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toMatch(/^claude: (server error \(claude-transport-error\)|auth failure \(codex-native-auth\))$/);
		}
		for (const [source, message] of vi.mocked(appLogger.warn).mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toBe("[AutoRetry] claude: exhausted 3 retries, manual intervention needed");
		}
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// Catches: continue input erases the failure even when the harness never resumes.
	it("keeps Error when injected continue leaves the harness idle", async () => {
		failed();
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(5_000);
		expect(sendCommand).toHaveBeenCalledExactlyOnceWith("live-session", "continue", "claude");
		emit({ type: "user-input", content: "continue", line: -1 });
		emit({ type: "shell-state", state: "busy" });
		emit({ type: "shell-state", state: "idle" });
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
		snapshot("idle", "Connection lost mid-response.");
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});

	// Catches: a late pre-error idle snapshot cancels the first parked retry.
	it("does not treat a no-error idle snapshot as a recovered turn", async () => {
		failed();
		snapshot("idle");
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(5_000);
		expect(sendCommand).toHaveBeenCalledExactlyOnceWith("live-session", "continue", "claude");
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});

	// Catches: each failed attempt's spinner/intent replenishes the retry budget forever.
	it("exhausts three retries despite working rows and intent between failures", async () => {
		for (const [index, delay] of [5_000, 15_000, 30_000].entries()) {
			failed();
			snapshot("idle", "Connection lost mid-response.");
			await vi.advanceTimersByTimeAsync(delay - 1);
			expect(sendCommand).toHaveBeenCalledTimes(index);
			await vi.advanceTimersByTimeAsync(1);
			expect(sendCommand).toHaveBeenCalledTimes(index + 1);
			emit({ type: "user-input", content: "continue", line: -1 });
			snapshot("busy");
			emit({ type: "status-line", task_name: "Working" });
			emit({ type: "intent", text: "Retrying the request", title: "Retry" });
		}
		failed();
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(60_000);
		expect(sendCommand).toHaveBeenCalledTimes(3);
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});

	// Catches: a recovered successful turn remains red or inherits an exhausted budget.
	it("clears Error after recovery and gives a later failure its own retry budget", async () => {
		failed();
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(5_000);
		snapshot("busy");
		emit({ type: "status-line", task_name: "Working" });
		snapshot("idle");
		expect(terminalsStore.get(id)?.awaitingInput).toBeNull();
		failed();
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(5_000);
		expect(sendCommand).toHaveBeenCalledTimes(2);
	});

	// Catches: authentication failures send pointless continue retries or turn green on idle.
	it("keeps auth failures red without automatic retry", async () => {
		emit({
			type: "api-error",
			pattern_name: "codex-native-auth",
			matched_text: "unexpected status 401",
			error_kind: "auth",
		});
		snapshot("idle", "unexpected status 401");
		await vi.advanceTimersByTimeAsync(60_000);
		expect(sendCommand).not.toHaveBeenCalled();
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});
});
