import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const retrySettings = vi.hoisted(() => ({ enabled: true }));

const { createSession, resize, sendCommand, mockRpc, ptyOptions, mockVerifyResume } = vi.hoisted(() => ({
	createSession: vi.fn().mockResolvedValue("fresh-session"),
	resize: vi.fn().mockResolvedValue(undefined),
	sendCommand: vi.fn().mockResolvedValue(undefined),
	mockRpc: vi.fn(),
	ptyOptions: new Map<
		string,
		Array<{
			onStateChange?: (state: Record<string, unknown>) => void;
			onParsed?: (frame: { type: string; event: unknown }) => void;
		}>
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
		options: typeof ptyOptions extends Map<string, Array<infer V>> ? V : never,
	) => {
		const listeners = ptyOptions.get(sessionId) ?? [];
		listeners.push(options);
		ptyOptions.set(sessionId, listeners);
		return Promise.resolve(() => {
			const index = listeners.indexOf(options);
			if (index >= 0) listeners.splice(index, 1);
		});
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
		isAutoRetryEnabled: () => retrySettings.enabled,
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
import { resumeTerminal, suspendTerminal } from "../../utils/suspendTerminal";

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

// Like the real transport, broadcast to every active subscription. A Map that
// keeps only the newest options hides duplicate listener installation.
const emit = (event: Record<string, unknown>) => {
	for (const options of ptyOptions.get("live-session") ?? []) options.onParsed?.({ type: "parsed", event });
};
const failed = () =>
	emit({
		type: "api-error",
		pattern_name: "claude-transport-error",
		matched_text: "Connection lost mid-response.",
		error_kind: "server",
	});
const snapshot = (shell: string, error?: string) => {
	for (const options of ptyOptions.get("live-session") ?? [])
		options.onStateChange?.({
			agent_type: "claude",
			agent_state: shell === "busy" ? "working" : "completed",
			shell_state: shell,
			last_error: error,
		});
};

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
		retrySettings.enabled = true;
		localStorage.clear();
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
		// These scenarios deliberately report provider failures.
		for (const [source, message] of vi.mocked(appLogger.error).mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toMatch(/^claude: (server error \(claude-transport-error\)|auth failure \(codex-native-auth\))$/);
		}
		expect(appLogger.warn).not.toHaveBeenCalled();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// Catches: reconnect resize succeeds but a failed eager handshake leaves parsed events unattached.
	it("recovers parsed events after an eager subscription handshake fails", async () => {
		vi.useRealTimers();
		mounted?.unmount();
		const transport = await import("../../transport");
		vi.spyOn(transport, "subscribePty").mockRejectedValueOnce(new Error("WebSocket closed before opening"));
		const replacement = openIdleAgentTab();
		try {
			await waitFor(() =>
				expect(appLogger.info).toHaveBeenCalledWith(
					"terminal",
					`initSession(${replacement}) — reconnected to live-session`,
				),
			);
			expect(appLogger.error).toHaveBeenCalledWith("terminal", "Failed to attach session listeners", expect.any(Error));
			vi.mocked(appLogger.error).mockClear();
			failed();
			expect(appLogger.error).toHaveBeenCalledExactlyOnceWith(
				"terminal",
				"claude: server error (claude-transport-error)",
			);
		} finally {
			mounted?.unmount();
			terminalsStore.remove(replacement);
		}
	});

	// Catches: one parsed transport error is logged twice by the terminal consumer.
	it("logs one entry for one transport error", () => {
		failed();
		expect(appLogger.error).toHaveBeenCalledExactlyOnceWith(
			"terminal",
			"claude: server error (claude-transport-error)",
		);
	});

	// Catches: removing the reconnect attachment also drops the replacement PTY on resume.
	it("resumes with one replacement listener and retires the old session", async () => {
		vi.useRealTimers();
		expect((await suspendTerminal(id)).ok).toBe(true);
		expect(ptyOptions.get("live-session")).toHaveLength(0);
		mockRpc.mockResolvedValue({ session_id: "resumed-session" });
		expect((await resumeTerminal(id)).ok).toBe(true);
		await waitFor(() =>
			expect(appLogger.info).toHaveBeenCalledWith("terminal", `initSession(${id}) — reconnected to resumed-session`),
		);
		for (const options of ptyOptions.get("resumed-session") ?? [])
			options.onParsed?.({
				type: "parsed",
				event: {
					type: "api-error",
					pattern_name: "claude-transport-error",
					matched_text: "Connection lost mid-response.",
					error_kind: "server",
				},
			});
		expect(appLogger.error).toHaveBeenCalledExactlyOnceWith(
			"terminal",
			"claude: server error (claude-transport-error)",
		);
		expect(createSession).not.toHaveBeenCalled();
	});

	// Catches: an overwritten unsubscribe handle leaves the first listener alive on tab close.
	it("retires every transport listener when the terminal closes", () => {
		mounted?.unmount();
		expect(ptyOptions.get("live-session")).toHaveLength(0);
		failed();
		expect(appLogger.error).not.toHaveBeenCalled();
	});

	// Catches: continue input erases the failure even when the harness never resumes.
	it("keeps Error when a submitted continue leaves the harness idle", async () => {
		failed();
		snapshot("idle", "Connection lost mid-response.");
		emit({ type: "user-input", content: "continue", line: -1 });
		emit({ type: "shell-state", state: "busy" });
		emit({ type: "shell-state", state: "idle" });
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
		snapshot("idle", "Connection lost mid-response.");
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});

	// Catches: the frontend retries as well as the backend (api_retry.rs), submitting continue twice.
	it("never submits continue itself: the backend owns the retry", async () => {
		failed();
		snapshot("idle", "Connection lost mid-response.");
		await vi.advanceTimersByTimeAsync(60_000);
		expect(sendCommand).not.toHaveBeenCalled();
		expect(terminalsStore.get(id)?.awaitingInput).toBe("error");
	});

	// Catches: a recovered successful turn remains red.
	it("clears Error after recovery", async () => {
		failed();
		snapshot("idle", "Connection lost mid-response.");
		snapshot("busy");
		emit({ type: "status-line", task_name: "Working" });
		snapshot("idle");
		expect(terminalsStore.get(id)?.awaitingInput).toBeNull();
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
	// Catches: a disabled expert-only recovery option remains undiscoverable after a recoverable failure.
	it("offers a dismissible retry tip only for a disabled server-error recovery", () => {
		retrySettings.enabled = false;
		emit({ type: "api-error", error_kind: "auth", pattern_name: "codex-native-auth", matched_text: "Unauthorized" });
		expect(document.querySelector('aside[aria-label="Retry temporary API failures"]')).toBeNull();
		failed();
		expect(document.querySelector('aside[aria-label="Retry temporary API failures"]')).not.toBeNull();
		(document.querySelector('button[aria-label="Dismiss Retry temporary API failures"]') as HTMLButtonElement).click();
		expect(document.querySelector('aside[aria-label="Retry temporary API failures"]')).toBeNull();
		expect(localStorage.getItem("tuic.dismissed-tip.autoRetry")).toBe("true");
		expect(sendCommand).not.toHaveBeenCalled();
	});
});
