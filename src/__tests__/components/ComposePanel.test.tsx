import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ComposePanel } from "../../components/ComposePanel/ComposePanel";
import { captureDictationTarget } from "../../hooks/dictationTarget";
import type { QueuedCommand } from "../../hooks/usePty";
import { invoke } from "../../invoke";
import { terminalsStore } from "../../stores/terminals";

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

afterEach(async () => {
	cleanup();
	// happy-dom implements requestAnimationFrame with setImmediate. CodeMirror may
	// leave one measurement frame queued while its view is being destroyed; keep
	// that frame inside the test lifecycle so Vitest does not report an async leak.
	await new Promise<void>((resolve) => setImmediate(resolve));
	// destroy() also blurs a focused editor, and CodeMirror answers every focus
	// change with a 10ms timer — a pinned panel keeps focus after a send.
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
});

/** Type text into the panel's CodeMirror instance. Dispatching a change through
 *  the view is the only way in — the editor has no <textarea> to fill. */
function typeIntoEditor(container: HTMLElement, text: string): void {
	const editor = container.querySelector(".cm-editor") as HTMLElement | null;
	if (!editor) throw new Error("CodeMirror not mounted");
	const view = EditorView.findFromDOM(editor);
	if (!view) throw new Error("CodeMirror view not attached");
	view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

function editorText(container: HTMLElement): string {
	const editor = container.querySelector(".cm-editor") as HTMLElement;
	return EditorView.findFromDOM(editor)?.state.doc.toString() ?? "";
}

function renderPanel(overrides: Partial<Parameters<typeof ComposePanel>[0]> = {}) {
	const [isOpen] = createSignal(true);
	const [queued, setQueued] = createSignal(0);
	const props = {
		isOpen,
		initialText: () => "",
		onClose: vi.fn(),
		onSend: vi.fn(),
		onEnqueue: vi.fn(),
		canEnqueue: () => true,
		queuedCount: queued,
		onClearQueue: vi.fn(),
		onLoadQueue: vi.fn(
			async (): Promise<QueuedCommand[]> => [
				{ id: 1, text: "run the tests", kind: "user_command" },
				{ id: 2, text: "then push", kind: "user_command" },
			],
		),
		onRemoveQueued: vi.fn(),
		pinned: () => false,
		onTogglePin: vi.fn(),
		onDismiss: vi.fn(),
		focusRequest: () => 0,
		...overrides,
	};
	const rendered = render(() => <ComposePanel {...props} />);
	return { ...rendered, props, setQueued };
}

describe("ComposePanel", () => {
	// Catches: the mounted Compose editor is undiscoverable from transcript/BODY focus.
	it("delivers Chat dictation into the mounted editor and publishes the changed draft", async () => {
		const id = terminalsStore.add({
			sessionId: "compose-dictation",
			fontSize: 14,
			name: "Chat",
			cwd: null,
			awaitingInput: null,
		});
		terminalsStore.setActive(id);
		terminalsStore.setViewMode(id, "chat");
		const onTextChange = vi.fn();
		const { container } = renderPanel({ pinned: () => true, initialText: () => "before THIS after", onTextChange });
		container.dataset.terminalId = id;
		try {
			await waitFor(() => expect(editorText(container)).toBe("before THIS after"));
			const dom = container.querySelector<HTMLElement>(".cm-editor");
			const view = dom && EditorView.findFromDOM(dom);
			if (!view) throw new Error("Compose editor not mounted");
			(document.activeElement as HTMLElement | null)?.blur();
			expect(view.hasFocus).toBe(false);
			view.dispatch({ selection: { anchor: 7, head: 11 } });
			const target = captureDictationTarget();
			const write = vi.fn();
			await target.deliver("spoken words", write, false);
			expect(editorText(container)).toBe("before spoken words after");
			expect(onTextChange).toHaveBeenLastCalledWith("before spoken words after");
			expect(view.state.selection.main.anchor).toBe(19);
			expect(write).not.toHaveBeenCalled();
		} finally {
			terminalsStore.remove(id);
		}
	});

	it("Enter sends instead of inserting a newline; Shift+Ctrl+Enter still queues", async () => {
		const { container, props } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "run the tests");
		const content = container.querySelector(".cm-content") as HTMLElement;

		fireEvent.keyDown(content, { key: "Enter" });
		expect(props.onSend).toHaveBeenCalledWith("run the tests");
		expect(props.onEnqueue).not.toHaveBeenCalled();
		// Let the send settle: a second submit while one is in flight is dropped.
		await new Promise<void>((resolve) => setImmediate(resolve));

		fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, shiftKey: true });
		expect(props.onEnqueue).toHaveBeenCalledWith("run the tests");
		// The immediate-send path must not fire a second time: queueing exists
		// precisely so the text does NOT reach a working agent now.
		expect(props.onSend).toHaveBeenCalledTimes(1);
	});

	// Catches: Ctrl+Enter still sending, or Shift+Enter falling through to send.
	it.each([{ ctrlKey: true }, { shiftKey: true }])("inserts a newline without submitting for %o", async (modifiers) => {
		const { container, props } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "before after");
		const editor = container.querySelector(".cm-editor") as HTMLElement;
		const view = EditorView.findFromDOM(editor)!;
		view.dispatch({ selection: { anchor: 6 } });
		fireEvent.keyDown(view.contentDOM, { key: "Enter", ...modifiers });
		expect(editorText(container)).toBe("before\n after");
		expect(props.onSend).not.toHaveBeenCalled();
		expect(props.onEnqueue).not.toHaveBeenCalled();
	});

	// Catches: IME candidate confirmation submitting a half-composed draft.
	it("does not send IME Enter, then sends with Enter after composition ends", async () => {
		const { container, props } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "draft");
		const content = container.querySelector(".cm-content") as HTMLElement;
		fireEvent.compositionStart(content);
		// Drive the DOM edit that a browser makes during composition, not an
		// internal CodeMirror flag. Its observer derives the composing state.
		content.querySelector(".cm-line")!.textContent = "候補";
		await waitFor(() => expect(editorText(container)).toBe("候補"));
		fireEvent.keyDown(content, { key: "Enter", isComposing: true, keyCode: 229 });
		expect(props.onSend).not.toHaveBeenCalled();
		expect(props.onEnqueue).not.toHaveBeenCalled();
		expect(editorText(container)).toBe("候補");
		fireEvent.compositionEnd(content);
		fireEvent.keyDown(content, { key: "Enter" });
		expect(props.onSend).toHaveBeenCalledWith("候補");
	});

	// Catches: the newline legend advertising both bindings and crowding compact panels.
	it("shows only Ctrl+Enter for newline in the shortcut legend", () => {
		const { getByText } = renderPanel();
		expect(getByText("send").textContent).toBe("↵ send");
		expect(getByText("newline").textContent).toBe("ctrl ↵ newline");
		expect(getByText("queue").textContent).toBe("⇧ ctrl ↵ queue");
	});

	// Catches: the send control still advertising the old shortcut.
	it("labels the send control with Enter", () => {
		const { getByRole } = renderPanel();
		expect(getByRole("button", { name: "Send (Enter)" }).getAttribute("title")).toBe("Send (Enter)");
	});

	it("refuses to queue for a session with no agent", async () => {
		const { container, props } = renderPanel({ canEnqueue: () => false });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "ls -la");
		const content = container.querySelector(".cm-content") as HTMLElement;

		fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, shiftKey: true });
		expect(props.onEnqueue).not.toHaveBeenCalled();
		expect(container.querySelector('[title^="Queue for the next idle moment"]')).toBeNull();
	});

	it("shows the pending count only when something is queued", async () => {
		const { container, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		expect(container.textContent).not.toContain("queued");

		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
	});

	it("expands the badge into the queued texts — a count alone cannot be reviewed", async () => {
		const { container, props, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		// Collapsed: no IPC round-trip for texts nobody is looking at.
		expect(props.onLoadQueue).not.toHaveBeenCalled();

		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("run the tests")).toBeTruthy());
		expect(getByText("then push")).toBeTruthy();
	});

	it("shows server-parked entries and lets them be removed", async () => {
		// The regression: a TUIC notice parked in the same FIFO blocked `submit`
		// with `queued_commands_pending` while this list showed nothing, so an
		// operator staring at an empty composer had nothing to act on.
		const { container, props, setQueued, getByText, getAllByTitle } = renderPanel({
			onLoadQueue: vi.fn(
				async (): Promise<QueuedCommand[]> => [
					{ id: 7, text: "[TUIC] message available", kind: "notice" },
					{ id: 8, text: "run the tests", kind: "user_command" },
				],
			),
		});
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));

		await waitFor(() => expect(container.textContent).toContain("TUIC notice"));
		expect(container.textContent).toContain("[TUIC] message available");

		fireEvent.click(getAllByTitle("Remove from queue")[0]);
		expect(props.onRemoveQueued).toHaveBeenCalledWith(7);
	});

	it("removes a single queued command by id", async () => {
		const { container, props, setQueued, getByText, getAllByTitle } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("then push")).toBeTruthy());

		fireEvent.click(getAllByTitle("Remove from queue")[1]);
		expect(props.onRemoveQueued).toHaveBeenCalledWith(2);
	});

	it("clears the whole queue from the expanded list", async () => {
		const { container, props, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("Clear all")).toBeTruthy());

		fireEvent.click(getByText("Clear all"));
		expect(props.onClearQueue).toHaveBeenCalledTimes(1);
	});

	it("collapses the list when the queue drains", async () => {
		const { container, setQueued, getByText, queryByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("run the tests")).toBeTruthy());

		setQueued(0);
		await waitFor(() => expect(queryByText("run the tests")).toBeNull());
	});

	it("closes from the close button even when pinned, where Esc only leaves the panel", async () => {
		const { container, props, getByLabelText } = renderPanel({ pinned: () => true });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());

		fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Escape" });
		expect(props.onDismiss).not.toHaveBeenCalled();

		fireEvent.click(getByLabelText("Close compose panel"));
		expect(props.onDismiss).toHaveBeenCalledTimes(1);
	});

	describe("pinned", () => {
		it("empties the editor after a send and stays open — it replaces the agent's input box", async () => {
			const { container, props } = renderPanel({ pinned: () => true });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Enter" });
			expect(props.onSend).toHaveBeenCalledWith("run the tests");
			await waitFor(() => expect(editorText(container)).toBe(""));
			expect(props.onClose).not.toHaveBeenCalled();
		});

		it("empties the editor after a queue as well", async () => {
			const { container, props } = renderPanel({ pinned: () => true });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "then push");

			fireEvent.click(container.querySelector('[title^="Queue for the next idle moment"]') as HTMLElement);
			expect(props.onEnqueue).toHaveBeenCalledWith("then push");
			await waitFor(() => expect(editorText(container)).toBe(""));
		});

		it("keeps what was typed while the send was in flight — only the sent text is cleared", async () => {
			let resolveSend: () => void = () => {};
			const onSend = vi.fn(() => new Promise<void>((resolve) => (resolveSend = resolve)));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Enter" });
			expect(onSend).toHaveBeenCalledWith("run the tests");
			// The user starts the next message while sendCommand is still writing.
			const view = EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement);
			view?.dispatch({ changes: { from: view.state.doc.length, insert: "\nthen push" } });

			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe("then push"));
		});

		it("drops a second submit while the first is in flight, so the text is not sent twice", async () => {
			let resolveSend: () => void = () => {};
			const onSend = vi.fn(() => new Promise<void>((resolve) => (resolveSend = resolve)));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");
			const content = container.querySelector(".cm-content") as HTMLElement;

			fireEvent.keyDown(content, { key: "Enter" });
			fireEvent.keyDown(content, { key: "Enter" });
			fireEvent.click(container.querySelector('[title^="Queue for the next idle moment"]') as HTMLElement);
			expect(onSend).toHaveBeenCalledTimes(1);

			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe(""));
			// Once settled, the panel accepts the next message.
			typeIntoEditor(container, "then push");
			fireEvent.keyDown(content, { key: "Enter" });
			expect(onSend).toHaveBeenCalledTimes(2);
			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe(""));
		});

		it("keeps the text when the send fails, so nothing typed is lost", async () => {
			const onSend = vi.fn(() => Promise.reject(new Error("pty gone")));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.click(container.querySelector('[title^="Send"]') as HTMLElement);
			await waitFor(() => expect(onSend).toHaveBeenCalled());
			await new Promise<void>((resolve) => setImmediate(resolve));
			expect(editorText(container)).toBe("run the tests");
		});

		it("leaves the text alone when unpinned — the parent closes the panel instead", async () => {
			const { container, props } = renderPanel();
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.click(container.querySelector('[title^="Send"]') as HTMLElement);
			expect(props.onSend).toHaveBeenCalledWith("run the tests");
			await new Promise<void>((resolve) => setImmediate(resolve));
			expect(editorText(container)).toBe("run the tests");
		});

		it("toggles through the pin button, which reports its state", async () => {
			const [pinned, setPinned] = createSignal(false);
			const onTogglePin = vi.fn(() => setPinned(!pinned()));
			const { container } = renderPanel({ pinned, onTogglePin });
			const button = () => container.querySelector("[aria-pressed]") as HTMLElement;
			await waitFor(() => expect(button()).not.toBeNull());
			expect(button().getAttribute("aria-pressed")).toBe("false");

			fireEvent.click(button());
			expect(onTogglePin).toHaveBeenCalledTimes(1);
			expect(button().getAttribute("aria-pressed")).toBe("true");
			expect(container.textContent).toContain("esc terminal");
		});

		it("moves the caret into the editor on a focus request", async () => {
			const [request, setRequest] = createSignal(0);
			const { container } = renderPanel({ pinned: () => true, focusRequest: request });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			(document.activeElement as HTMLElement | null)?.blur();
			const content = container.querySelector(".cm-content") as HTMLElement;
			await waitFor(() => expect(document.activeElement).not.toBe(content));

			setRequest(1);
			await waitFor(() => expect(document.activeElement).toBe(content));
		});
	});

	function pasteEvent(items: { type: string; file: File | null }[], text = ""): Event {
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: {
				items: items.map((i) => ({ type: i.type, getAsFile: () => i.file })),
				getData: (type: string) => (type === "text/plain" ? text : ""),
			},
		});
		return event;
	}

	// Catches: Compose ignoring clipboard images (story 1350-a1e6) — the paste fell
	// through to CodeMirror's text paste and the image was lost.
	it("saves a pasted image and inserts its [image: path] reference", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("/data/note-images/n1/pic.png");
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "look at ");
		const content = container.querySelector(".cm-content") as HTMLElement;

		const event = pasteEvent([{ type: "image/png", file: new File(["x"], "pic.png", { type: "image/png" }) }]);
		content.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true);
		await waitFor(() => expect(editorText(container)).toContain("[image: /data/note-images/n1/pic.png]"));
		expect(invoke).toHaveBeenCalledWith(
			"save_note_image",
			expect.objectContaining({ extension: "png", dataBase64: btoa("x") }),
		);
	});

	// Catches: the image handler swallowing every paste, breaking plain text paste.
	it("leaves a text-only paste to the editor", async () => {
		vi.mocked(invoke).mockClear();
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		const content = container.querySelector(".cm-content") as HTMLElement;

		const event = pasteEvent([{ type: "text/plain", file: null }], "plain words");
		content.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true); // CodeMirror's own text paste claims it
		expect(editorText(container)).toBe("plain words");
		expect(invoke).not.toHaveBeenCalled();
	});

	// Catches: a spreadsheet cell copy (text/plain + rendered image) attaching a picture instead of pasting the text.
	it("pastes the text and saves no image when the clipboard carries text/plain with an image", async () => {
		vi.mocked(invoke).mockClear();
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		const content = container.querySelector(".cm-content") as HTMLElement;

		content.dispatchEvent(
			pasteEvent(
				[
					{ type: "image/png", file: new File(["x"], "cell.png", { type: "image/png" }) },
					{ type: "text/plain", file: null },
				],
				"A1\tB1",
			),
		);

		expect(invoke).not.toHaveBeenCalled();
		expect(editorText(container)).toBe("A1\tB1");
	});

	// Catches: a Finder image-file copy (file name as text/plain + image/png) pasting the file name instead of the image.
	it("attaches the image when text/plain is only the image's file name", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("/data/note-images/n1/shot.png");
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		const content = container.querySelector(".cm-content") as HTMLElement;

		const event = pasteEvent(
			[
				{ type: "text/plain", file: null },
				{ type: "image/png", file: new File(["x"], "shot.png", { type: "image/png" }) },
			],
			"shot.png",
		);
		content.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true);
		await waitFor(() => expect(editorText(container)).toBe("[image: /data/note-images/n1/shot.png]"));
	});
});

describe("Compose pending recall", () => {
	it("withdraws before restoring text and keeps later typing — catches duplicate recall or a replaced draft", async () => {
		let acknowledge!: (text: string | null) => void;
		const recall = vi.fn(
			() =>
				new Promise<string | null>((resolve) => {
					acknowledge = resolve;
				}),
		);
		const { container } = renderPanel({ queuedCount: () => 2, onRecallQueued: recall, pinned: () => true });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		const content = container.querySelector(".cm-content") as HTMLElement;
		fireEvent.keyDown(content, { key: "ArrowUp" });
		fireEvent.keyDown(content, { key: "ArrowUp" });
		expect(recall).toHaveBeenCalledTimes(1);
		expect(editorText(container)).toBe("");
		typeIntoEditor(container, "new typing");
		acknowledge("pending prompt");
		await waitFor(() => expect(editorText(container)).toBe("pending prompt\n\nnew typing"));
	});

	it("keeps native ArrowUp in a nonempty editor — catches recall stealing normal caret movement", async () => {
		const recall = vi.fn().mockResolvedValue("pending prompt");
		const { container } = renderPanel({ queuedCount: () => 1, onRecallQueued: recall });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "already typing");
		fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "ArrowUp" });
		expect(recall).not.toHaveBeenCalled();
		expect(editorText(container)).toBe("already typing");
	});

	it("does not restore a lost withdrawal race — catches a prompt being edited after it already reached the CLI", async () => {
		const recall = vi.fn().mockResolvedValue(null);
		const { container } = renderPanel({ queuedCount: () => 1, onRecallQueued: recall });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "ArrowUp" });
		await waitFor(() => expect(recall).toHaveBeenCalledTimes(1));
		expect(editorText(container)).toBe("");
	});
});
