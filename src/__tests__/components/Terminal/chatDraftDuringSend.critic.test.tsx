import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

describe("Chat draft during delayed send", () => {
	const run = async (queue = false, remount = false, replace = false) => {
		let finish: (() => void) | undefined;
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
		const element = view.container.querySelector<HTMLElement>(".cm-editor");
		const editor = element && EditorView.findFromDOM(element);
		if (!editor) throw new Error("No Compose editor");
		editor.dispatch({ changes: { from: 0, insert: "Sent message" } });
		fireEvent.click(view.getByTitle(queue ? "Queue for the next idle moment (Shift+Ctrl+Enter)" : "Send (Enter)"));
		editor.dispatch({
			changes: replace
				? { from: 0, to: editor.state.doc.length, insert: "Next unsent message" }
				: { from: editor.state.doc.length, insert: "\nNext unsent message" },
		});
		terminalsStore.setViewMode(id, "cli");
		if (remount) {
			terminalsStore.setViewMode(id, "chat");
			await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
			await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
		}
		if (!finish) throw new Error("Submission did not start");
		finish();
		await new Promise<void>((resolve) => setImmediate(resolve));
		terminalsStore.setViewMode(id, "chat");
		await waitFor(() => {
			const restored = view.container.querySelector<HTMLElement>(".cm-editor");
			expect(restored && EditorView.findFromDOM(restored)?.state.doc.toString()).toBe("Next unsent message");
		});
	};

	// Catches: completion wiping a shared draft after its editor has unmounted.
	it("keeps_the_next_unsent_message_when_leaving_chat_before_send_settles", () => run());

	// Catches: queued completion, a remounted editor, or a replaced draft using stale cleanup.
	it.each([
		[true, false, false],
		[false, true, false],
		[true, true, false],
		[false, false, true],
		[true, false, true],
		[false, true, true],
		[true, true, true],
	])("preserves_next_draft_queue_%s_remount_%s_replace_%s", run);
});
