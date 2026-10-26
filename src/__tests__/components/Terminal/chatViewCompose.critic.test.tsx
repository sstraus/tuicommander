import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const pty = vi.hoisted(() => ({
	sendCommand: vi.fn().mockResolvedValue(undefined),
	enqueueCommand: vi.fn().mockResolvedValue({ queued: 1 }),
}));

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		...pty,
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
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
	invoke: vi.fn(async (command: string) => {
		if (command === "get_session_foreground_process") return "claude";
		if (command === "get_shell_state") return "busy";
		if (command !== "chat_view_snapshot") return undefined;
		return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	}),
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

function addTerminal() {
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/repo",
		repoPath: "/repo",
		name: "claude",
		fontSize: 13,
		awaitingInput: null,
	});
	terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid" });
	return id;
}

function editor(container: HTMLElement) {
	const element = container.querySelector<HTMLElement>(".cm-editor");
	const view = element && EditorView.findFromDOM(element);
	if (!view) throw new Error("Compose CodeMirror view is unavailable");
	return view;
}

async function readyEditor(container: HTMLElement) {
	await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
	// Opening initializes CodeMirror over two frames; type after setup finishes.
	await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
	return editor(container);
}

function typeDraft(container: HTMLElement, text: string) {
	const view = editor(container);
	view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

describe("Chat Compose input", () => {
	// Catches: entering Chat without an input, or forcing Compose open in CLI on return.
	it("opens_docked_and_focused_in_chat_and_restores_closed_cli_with_the_draft", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		expect(cm.hasFocus).toBe(true);
		expect(view.container.querySelector('[class*="panelPinned"]')).not.toBeNull();
		expect(view.queryByLabelText("Close compose panel")).toBeNull();
		expect(view.queryByTitle("Unpin from the terminal bottom")).toBeNull();
		typeDraft(view.container, "Unsent from Chat");
		fireEvent.click(view.getByRole("tab", { name: "CLI" }));
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		fireEvent.click(view.getByText(/^Compose /));
		expect((await readyEditor(view.container)).state.doc.toString()).toBe("Unsent from Chat");
	});

	// Catches: Chat overwriting a tab's previous pin/open state or remounting away an unsent draft.
	it.each([false, true])("preserves_open_cli_pin_%s_and_drafts_in_both_directions", async (pinned) => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByText(/^Compose /));
		await readyEditor(view.container);
		if (pinned) fireEvent.click(view.getByTitle("Pin to the terminal bottom"));
		typeDraft(view.container, "Keep this unsent draft\nincluding its second line.");
		const cm = editor(view.container);
		cm.contentDOM.blur();
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await waitFor(() => expect(cm.hasFocus).toBe(true));
		expect(editor(view.container)).toBe(cm);
		expect(cm.state.doc.toString()).toContain("second line.");
		typeDraft(view.container, "Edited in Chat");
		fireEvent.click(view.getByRole("tab", { name: "CLI" }));
		expect(editor(view.container).state.doc.toString()).toBe("Edited in Chat");
		expect(view.container.querySelector('[class*="panelPinned"]') !== null).toBe(pinned);
		expect(view.getByLabelText("Close compose panel")).toBeDefined();
	});

	// Catches: closing Chat input after submit, confusing queue with immediate send, or changing CLI state.
	it.each([false, true])("retains_chat_input_and_clears_text_after_queue_%s", async (queue) => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		typeDraft(view.container, "Next task");
		fireEvent.keyDown(cm.contentDOM, { key: "Enter", ctrlKey: true, shiftKey: queue });
		await waitFor(() => expect(cm.state.doc.toString()).toBe(""));
		expect(cm.hasFocus).toBe(true);
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");
		expect(queue ? pty.enqueueCommand : pty.sendCommand).toHaveBeenCalledWith(
			"live-session",
			"Next task",
			...(queue ? [] : ["claude"]),
		);
		expect(queue ? pty.sendCommand : pty.enqueueCommand).not.toHaveBeenCalled();
		if (queue) expect(view.getByText("1 queued")).toBeDefined();
		fireEvent.click(view.getByRole("tab", { name: "CLI" }));
		expect(view.container.querySelector(".cm-editor")).toBeNull();
	});

	// Catches: always-open Chat ignoring a smart prompt because its open flag never changes.
	it("replaces_chat_text_from_the_terminal_ref_without_opening_cli_compose", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		typeDraft(view.container, "Old draft");
		terminalsStore.get(id)?.ref?.openComposeWithText?.("Review this change");
		expect(cm.state.doc.toString()).toBe("Review this change");
		cm.contentDOM.blur();
		terminalsStore.get(id)?.ref?.focus();
		expect(cm.hasFocus).toBe(true);
		fireEvent.click(view.getByRole("tab", { name: "CLI" }));
		expect(view.container.querySelector(".cm-editor")).toBeNull();
	});

	// Catches: shortcut/dismiss paths hiding the only Chat input, or Escape focusing a hidden grid.
	it("keeps_compose_active_on_its_shortcut_and_esc_returns_to_cli_with_the_draft", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		typeDraft(view.container, "Keep on Escape");
		terminalsStore.get(id)?.ref?.toggleCompose?.();
		expect(editor(view.container)).toBe(cm);
		expect(cm.hasFocus).toBe(true);
		fireEvent.keyDown(cm.contentDOM, { key: "Escape" });
		expect(terminalsStore.get(id)?.viewMode).toBe("cli");
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		expect((await readyEditor(view.container)).state.doc.toString()).toBe("Keep on Escape");
	});
});
