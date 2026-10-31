import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn(), diag: { info: vi.fn() } },
}));

const pty = vi.hoisted(() => ({
	sendCommand: vi.fn().mockResolvedValue(undefined),
	enqueueCommand: vi.fn().mockResolvedValue({ queued: 1 }),
	listQueuedCommands: vi.fn().mockResolvedValue([]),
	removeQueuedCommand: vi.fn().mockResolvedValue(false),
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
// Canvas publishes its ref only after its asynchronous grid subscription is ready.
const canvasLifecycle = vi.hoisted(() => ({ deferRef: false }));

vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: (props: { onRef?: (ref: { focus: () => void; searchClear: () => void }) => void }) => {
		let input!: HTMLInputElement;
		return (
			<div data-testid="canvas" data-terminal-container>
				<input
					aria-label="Canvas terminal input"
					ref={(element) => {
						input = element;
						if (!canvasLifecycle.deferRef) {
							props.onRef?.({ focus: () => input.focus({ preventScroll: true }), searchClear: vi.fn() });
						}
					}}
				/>
			</div>
		);
	},
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (command: string) => {
		if (command === "get_session_foreground_process") return "claude";
		if (command === "get_shell_state") return "idle";
		if (command !== "chat_view_snapshot") return undefined;
		return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	}),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import { paneLayoutStore } from "../../../stores/paneLayout";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	vi.clearAllMocks();
	canvasLifecycle.deferRef = false;
	window.getSelection()?.removeAllRanges();
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
	terminalsStore.setActive(id);
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

describe("Chat transcript typing handoff", () => {
	// Catches: clicking the transcript drops the first key or replaces the whole draft.
	it("redirects_transcript_printable_keys_to_the_existing_compose_selection", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		typeDraft(view.container, "Keep draft");
		cm.contentDOM.blur();
		cm.dispatch({ selection: { anchor: 5, head: 10 } });
		const transcript = view.getByLabelText("Chat transcript");
		fireEvent.click(transcript);
		transcript.focus(); // The DOM harness does not apply the browser's click focus default.
		fireEvent.keyDown(transcript, { key: "A", shiftKey: true });
		expect(cm.state.doc.toString()).toBe("Keep A");
		expect(cm.hasFocus).toBe(true);
	});

	// Catches: key forwarding steals native selection/copy or navigation from the transcript.
	it("preserves_transcript_selection_and_shortcut_keys", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		const transcript = view.getByLabelText("Chat transcript");
		const text = document.createTextNode("Selected transcript text");
		transcript.append(text);
		transcript.focus();
		const range = document.createRange();
		range.selectNodeContents(text);
		window.getSelection()?.removeAllRanges();
		window.getSelection()?.addRange(range);
		fireEvent.keyDown(transcript, { key: "x" });
		fireEvent.keyDown(transcript, { key: "c", metaKey: true });
		expect(invoke).toHaveBeenCalledWith("write_clipboard_text", { text: "Selected transcript text" });
		expect(window.getSelection()?.toString()).toBe("Selected transcript text");
		expect(document.activeElement).toBe(transcript);
		window.getSelection()?.removeAllRanges();
		for (const init of [
			{ key: "ArrowDown" },
			{ key: "Escape" },
			{ key: "x", metaKey: true },
			{ key: "x", ctrlKey: true },
			{ key: "Dead" },
			{ key: "a", isComposing: true },
		]) {
			const event = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
			transcript.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(false);
			expect(document.activeElement).toBe(transcript);
		}
		expect(cm.state.doc.toString()).toBe("");
	});

	// Catches: redirecting text from Find or a transcript button/link instead of its container.
	it("leaves_search_and_interactive_transcript_descendants_alone", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		const transcript = view.getByLabelText("Chat transcript");
		transcript.focus();
		fireEvent.keyDown(transcript, { key: "f", metaKey: true });
		const search = view.getByLabelText("Find in chat");
		search.focus();
		fireEvent.keyDown(search, { key: "q" });
		expect(document.activeElement).toBe(search);
		for (const tag of ["button", "a"]) {
			const control = document.createElement(tag);
			control.tabIndex = 0;
			transcript.append(control);
			control.focus();
			fireEvent.keyDown(control, { key: "q" });
			expect(document.activeElement).toBe(control);
		}
		expect(cm.state.doc.toString()).toBe("");
	});
});

describe("Chat Compose input", () => {
	// Catches: the CLI switch reveals the grid but leaves keyboard focus on its tab.
	it("returns_keyboard_focus_to_canvas_when_switching_from_chat_to_cli", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		expect((await readyEditor(view.container)).hasFocus).toBe(true);
		const cli = view.getByRole("tab", { name: "CLI" });
		cli.focus();
		fireEvent.click(cli);
		await waitFor(() => expect(document.activeElement).toBe(view.getByLabelText("Canvas terminal input")));
	});

	// Catches: unavailable Chat returning to a visible grid without a keyboard target.
	it("focuses_canvas_when_chat_becomes_unavailable", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		terminalsStore.update(id, { agentType: null });
		await waitFor(() => expect(document.activeElement).toBe(view.getByLabelText("Canvas terminal input")));
	});

	// Catches: fallback in a background Chat tab stealing focus from another terminal.
	it("does_not_focus_a_hidden_canvas_when_chat_becomes_unavailable", async () => {
		const id = addTerminal();
		const [visible, setVisible] = createSignal(true);
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" alwaysVisible={visible()} />
				<input aria-label="Other terminal input" />
			</>
		));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		const other = terminalsStore.add({
			sessionId: null,
			awaitingInput: null,
			cwd: "/other",
			name: "other",
			fontSize: 13,
		});
		terminalsStore.setActive(other);
		setVisible(false);
		const input = view.getByLabelText("Other terminal input");
		input.focus();
		terminalsStore.update(id, { agentType: null });
		await waitFor(() => expect(terminalsStore.get(id)?.viewMode).toBe("cli"));
		expect(document.activeElement).toBe(input);
	});

	// Catches: the real Compose Send path never starts Chat feedback before PTY output.
	it("shows working immediately from Compose Send until existing terminal state settles", async () => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		terminalsStore.update(id, { shellState: "idle" });
		expect(view.queryByText("Working…")).toBeNull();
		typeDraft(view.container, "Next task");
		fireEvent.keyDown(cm.contentDOM, { key: "Enter" });
		expect(view.getByText("Working…")).toBeTruthy();
		await waitFor(() => expect(pty.sendCommand).toHaveBeenCalled());
		expect(view.getByText("Working…")).toBeTruthy();
		terminalsStore.update(id, { shellState: "busy" });
		expect(view.getByText("Working…")).toBeTruthy();
		terminalsStore.update(id, { shellState: "idle" });
		expect(view.queryByText("Working…")).toBeNull();
	});

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
		fireEvent.keyDown(cm.contentDOM, { key: "Enter", ctrlKey: queue, shiftKey: queue });
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
		await waitFor(() => expect(document.activeElement).toBe(view.getByLabelText("Canvas terminal input")));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		expect((await readyEditor(view.container)).state.doc.toString()).toBe("Keep on Escape");
	});
});

describe("Chat fallback focus ownership", () => {
	// Catches: asynchronous Chat fallback redirects typing from a settings/dialog field to the PTY.
	it("keeps_another_input_focused_when_active_chat_becomes_unavailable", async () => {
		const id = addTerminal();
		terminalsStore.setActive(id);
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" />
				<input aria-label="Settings input" />
			</>
		));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		const settingsInput = view.getByLabelText("Settings input");
		settingsInput.focus();
		terminalsStore.update(id, { agentType: null });
		await waitFor(() => expect(terminalsStore.get(id)?.viewMode).toBe("cli"));
		expect(document.activeElement).toBe(settingsInput);
	});

	// Catches: a visible but inactive split pane steals typing on its own Chat fallback.
	it("keeps_the_active_pane_input_focused_when_another_visible_pane_falls_back", async () => {
		const id = addTerminal();
		const otherId = terminalsStore.add({
			sessionId: "other-session",
			cwd: "/repo",
			repoPath: "/repo",
			name: "shell",
			fontSize: 13,
			awaitingInput: null,
		});
		const group = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(group, { id, type: "terminal" });
		const otherGroup = paneLayoutStore.split(group, "horizontal");
		if (!otherGroup) throw new Error("split setup failed");
		paneLayoutStore.addTab(otherGroup, { id: otherId, type: "terminal" });
		terminalsStore.setActive(id);
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" />
				<div data-terminal-id={otherId}>
					<input aria-label="Other pane input" />
				</div>
			</>
		));
		try {
			fireEvent.click(view.getByRole("tab", { name: "Chat" }));
			await readyEditor(view.container);
			terminalsStore.setActive(otherId);
			paneLayoutStore.setActiveGroup(otherGroup);
			const otherInput = view.getByLabelText("Other pane input");
			otherInput.focus();
			terminalsStore.update(id, { agentType: null });
			await waitFor(() => expect(terminalsStore.get(id)?.viewMode).toBe("cli"));
			expect(document.activeElement).toBe(otherInput);
		} finally {
			cleanup();
			paneLayoutStore.reset();
			paneLayoutStore.flushSave();
		}
	});
});

describe("independent CLI focus guard conditions", () => {
	// Catches: active identity removed while a background pane remains visible.
	it("does_not_return_focus_to_a_visible_inactive_terminal", async () => {
		const id = addTerminal();
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" alwaysVisible />
				<button type="button">Keep keyboard focus</button>
			</>
		));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		const other = terminalsStore.add({
			sessionId: null,
			cwd: "/other",
			name: "other",
			fontSize: 13,
			awaitingInput: null,
		});
		terminalsStore.setActive(other);
		const button = view.getByRole("button", { name: "Keep keyboard focus" });
		button.focus();
		terminalsStore.setViewMode(id, "cli");
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		expect(view.getByLabelText("Canvas terminal input").isConnected).toBe(true);
		expect(document.activeElement).toBe(button);
	});

	// Catches: visibility removed while the active tab's local canvas is detached.
	it("does_not_return_focus_to_a_hidden_but_active_terminal", async () => {
		const id = addTerminal();
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" />
				<button type="button">Keep keyboard focus</button>
			</>
		));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		// A floating window owns this active tab; the original window is hidden.
		terminalsStore.detach(id, "floating-focus-guard");
		const button = view.getByRole("button", { name: "Keep keyboard focus" });
		button.focus();
		terminalsStore.setViewMode(id, "cli");
		expect(terminalsStore.state.activeId).toBe(id);
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		expect(view.getByLabelText("Canvas terminal input").isConnected).toBe(true);
		expect(document.activeElement).toBe(button);
	});

	// Catches: the return path runs on initial CLI render, without leaving Chat.
	it("does_not_return_focus_on_initial_cli_render", () => {
		const id = addTerminal();
		const button = document.createElement("button");
		button.textContent = "Keep initial focus";
		document.body.appendChild(button);
		button.focus();
		try {
			const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
			// The normal visibility RAF has not run: this checks only the initial
			// view-mode effect, with a mounted canvas and a non-editable focus owner.
			expect(view.getByLabelText("Canvas terminal input").isConnected).toBe(true);
			expect(view.container.querySelector(".cm-editor")).toBeNull();
			expect(document.activeElement).toBe(button);
		} finally {
			button.remove();
		}
	});
});

describe("Compose focus lifecycle", () => {
	// Catches: missing or constant Chat focus requests leave the next keystroke
	// outside an already-open editor until its deferred opening frames run.
	it("immediately_focuses_the_existing_editor_on_each_chat_entry", async () => {
		const id = addTerminal();
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" alwaysVisible />
				<button type="button">Other control</button>
			</>
		));
		fireEvent.click(view.getByText(/^Compose /));
		const cm = await readyEditor(view.container);
		view.getByRole("button", { name: "Other control" }).focus();
		expect(cm.hasFocus).toBe(false);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		expect(document.activeElement).toBe(cm.contentDOM);
		expect(editor(view.container)).toBe(cm);
		await readyEditor(view.container);
		fireEvent.click(view.getByRole("tab", { name: "CLI" }));
		view.getByRole("button", { name: "Other control" }).focus();
		expect(cm.hasFocus).toBe(false);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		expect(document.activeElement).toBe(cm.contentDOM);
		expect(editor(view.container)).toBe(cm);
	});

	// Catches: Escape closes a pinned CLI composer, leaves an unpinned one open,
	// or omits the canvas focus handoff in either branch.
	it.each([false, true])("cli_escape_preserves_pin_%s_and_returns_keyboard_focus", async (pinned) => {
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByText(/^Compose /));
		const cm = await readyEditor(view.container);
		if (pinned) fireEvent.click(view.getByTitle("Pin to the terminal bottom"));
		fireEvent.keyDown(cm.contentDOM, { key: "Escape" });
		expect(view.container.querySelector(".cm-editor") !== null).toBe(pinned);
		expect(document.activeElement).toBe(view.getByLabelText("Canvas terminal input"));
	});

	// Catches: an early Chat return dereferences a canvas whose grid subscription
	// has not published its ref yet (CanvasTerminal awaits transport.subscribe).
	it("returns_from_chat_before_the_canvas_ref_is_ready", async () => {
		canvasLifecycle.deferRef = true;
		const id = addTerminal();
		const view = render(() => (
			<>
				<Terminal id={id} cwd="/repo" alwaysVisible />
				<button type="button">Other control</button>
			</>
		));
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		await readyEditor(view.container);
		const button = view.getByRole("button", { name: "Other control" });
		button.focus();
		terminalsStore.setViewMode(id, "cli");
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		expect(document.activeElement).toBe(button);
	});

	// Catches: early CLI Escape dereferences the still-unavailable canvas ref.
	it("closes_cli_compose_before_the_canvas_ref_is_ready", async () => {
		canvasLifecycle.deferRef = true;
		const id = addTerminal();
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByText(/^Compose /));
		const cm = await readyEditor(view.container);
		// CodeMirror catches key-handler exceptions and reports them to console.
		// A closed panel alone would therefore miss an undefined-ref crash.
		const errors = vi.spyOn(console, "error").mockImplementation(() => {});
		try {
			fireEvent.keyDown(cm.contentDOM, { key: "Escape" });
			expect(view.container.querySelector(".cm-editor")).toBeNull();
			expect(view.getByText(/^Compose /)).toBeTruthy();
			expect(errors).not.toHaveBeenCalled();
		} finally {
			errors.mockRestore();
		}
	});
});

describe("host-owned terminal queue recall", () => {
	it("withdraws the newest user command, not a server notice — catches editing the wrong owner or delivering recalled text twice", async () => {
		const id = addTerminal();
		terminalsStore.update(id, { queuedCommands: 3 });
		pty.listQueuedCommands.mockResolvedValueOnce([
			{ id: 1, text: "older prompt", kind: "user_command" },
			{ id: 2, text: "latest full prompt", kind: "user_command" },
			{ id: 3, text: "BG wake", kind: "notice" },
		]);
		pty.removeQueuedCommand.mockResolvedValueOnce(true);
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		fireEvent.keyDown(cm.contentDOM, { key: "ArrowUp" });
		await waitFor(() => expect(cm.state.doc.toString()).toBe("latest full prompt"));
		expect(pty.removeQueuedCommand).toHaveBeenCalledWith("live-session", 2);
		expect(pty.sendCommand).not.toHaveBeenCalled();
		expect(terminalsStore.get(id)?.queuedCommands).toBe(2);
	});

	it("keeps the composer empty when queue delivery won — catches duplicating a command already dispatched to the CLI", async () => {
		const id = addTerminal();
		terminalsStore.update(id, { queuedCommands: 1 });
		pty.listQueuedCommands.mockResolvedValueOnce([{ id: 1, text: "already sent", kind: "user_command" }]);
		pty.removeQueuedCommand.mockResolvedValueOnce(false);
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByRole("tab", { name: "Chat" }));
		const cm = await readyEditor(view.container);
		fireEvent.keyDown(cm.contentDOM, { key: "ArrowUp" });
		await waitFor(() => expect(pty.removeQueuedCommand).toHaveBeenCalledWith("live-session", 1));
		expect(cm.state.doc.toString()).toBe("");
		expect(pty.sendCommand).not.toHaveBeenCalled();
	});
});
