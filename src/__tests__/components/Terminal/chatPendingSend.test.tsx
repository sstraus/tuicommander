import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const pending = vi.hoisted(() => ({ send: vi.fn(), queue: vi.fn() }));

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		sendCommand: pending.send,
		enqueueCommand: pending.queue,
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn().mockResolvedValue(undefined),
	subscribePty: () => Promise.resolve(() => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (cmd: string) =>
		cmd === "list_queued_agent_commands"
			? []
			: cmd === "get_session_foreground_process"
				? "claude"
				: cmd === "get_shell_state"
					? "busy"
					: cmd === "chat_view_snapshot"
						? { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 }
						: undefined,
	),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { appLogger } from "../../../stores/appLogger";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	vi.clearAllMocks();
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

afterEach(async () => {
	cleanup();
	// Real CodeMirror leaves a measurement frame and a 10ms blur timer on destroy.
	await new Promise<void>((resolve) => setImmediate(resolve));
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
});

// Catches: busy Chat Send entering the native queue before TUIC can claim it.
it("busy_chat_send_retains_one_prompt_without_native_submission", async () => {
	pending.queue.mockResolvedValue({ accepted: true, queued: 1, typed: false });
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/repo",
		repoPath: "/repo",
		name: "claude",
		fontSize: 13,
		awaitingInput: null,
	});
	terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid", shellState: "busy" });
	const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
	terminalsStore.setViewMode(id, "chat");
	await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
	await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
	const editor = EditorView.findFromDOM(view.container.querySelector<HTMLElement>(".cm-editor")!)!;
	editor.dispatch({ changes: { from: 0, insert: "Review now" } });
	fireEvent.click(view.getByTitle("Send (Enter)"));
	await waitFor(() => expect(pending.queue).toHaveBeenCalledOnce());
	expect(pending.queue.mock.calls[0].slice(0, 2)).toEqual(["live-session", "Review now"]);
	expect(pending.send).not.toHaveBeenCalled();
});
