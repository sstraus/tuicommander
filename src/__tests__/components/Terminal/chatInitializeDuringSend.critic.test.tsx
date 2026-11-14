import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

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
		cmd === "get_session_foreground_process"
			? "claude"
			: cmd === "get_shell_state"
				? "idle"
				: cmd === "chat_view_snapshot"
					? { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 }
					: undefined,
	),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { appLogger } from "../../../stores/appLogger";
import { terminalsStore } from "../../../stores/terminals";

beforeAll(async () => {
	// Terminal loads ComposePanel lazily. A cold import under full-suite load took longer than
	// the 1s waitFor for `.cm-editor`, so load it here and keep the waits about the view only.
	await import("../../../components/ComposePanel/ComposePanel");
});

beforeEach(() => {
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

describe("Chat send completion during editor initialization", () => {
	it.each([false, true])(
		"does_not_restore_submitted_text_when_queue_%s_settles_before_reopened_editor_initializes",
		async (queue) => {
			let finish!: () => void;
			(queue ? pending.queue : pending.send).mockImplementation(
				() =>
					new Promise<undefined | { queued: number }>((resolve) => {
						finish = () => resolve(queue ? { queued: 1 } : undefined);
					}),
			);
			const id = terminalsStore.add({
				sessionId: "live-session",
				cwd: "/repo",
				repoPath: "/repo",
				name: "claude",
				fontSize: 13,
				awaitingInput: null,
			});
			terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid" });
			const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
			terminalsStore.setViewMode(id, "chat");
			await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
			await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
			const editor = EditorView.findFromDOM(view.container.querySelector<HTMLElement>(".cm-editor")!)!;
			editor.dispatch({ changes: { from: 0, insert: "Sent message" } });
			fireEvent.click(view.getByTitle(queue ? "Queue for the next idle moment (Shift+Ctrl+Enter)" : "Send (Enter)"));
			editor.dispatch({ changes: { from: editor.state.doc.length, insert: "\nNext unsent message" } });
			terminalsStore.setViewMode(id, "cli");

			// A backend acknowledgement can arrive between reopening and its two initialization frames.
			const frames = new Map<number, FrameRequestCallback>();
			let frameId = 0;
			const raf = vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
				frames.set(++frameId, callback);
				return frameId;
			});
			const cancel = vi.spyOn(window, "cancelAnimationFrame").mockImplementation((id) => {
				frames.delete(id);
			});
			try {
				terminalsStore.setViewMode(id, "chat");
				await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
				finish();
				await new Promise<void>((resolve) => setImmediate(resolve));
				for (let frame = 0; frame < 2; frame++) {
					const callbacks = [...frames.values()];
					frames.clear();
					for (const callback of callbacks) callback(performance.now());
				}
				const reopened = EditorView.findFromDOM(view.container.querySelector<HTMLElement>(".cm-editor")!)!;
				expect(reopened.state.doc.toString()).toBe("Next unsent message");
			} finally {
				cleanup();
				raf.mockRestore();
				cancel.mockRestore();
			}
		},
	);
});
