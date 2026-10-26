import { EditorView } from "@codemirror/view";
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ComposePanel } from "../../components/ComposePanel/ComposePanel";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

afterEach(async () => {
	cleanup();
	await new Promise<void>((r) => setImmediate(r));
	await new Promise<void>((r) => setTimeout(r, 20));
});

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockResolvedValue("/saved/a.png" as never);
});

async function mount() {
	const [isOpen] = createSignal(true);
	const [queued] = createSignal(0);
	const r = render(() => (
		<ComposePanel
			isOpen={isOpen}
			initialText={() => ""}
			onClose={vi.fn()}
			onSend={vi.fn()}
			onEnqueue={vi.fn()}
			canEnqueue={() => true}
			queuedCount={queued}
			onClearQueue={vi.fn()}
			onLoadQueue={vi.fn(async () => [])}
			onRemoveQueued={vi.fn()}
			pinned={() => false}
			onTogglePin={vi.fn()}
			onDismiss={vi.fn()}
			focusRequest={() => 0}
		/>
	));
	const editor = r.container.querySelector(".cm-editor") as HTMLElement;
	const view = EditorView.findFromDOM(editor) as EditorView;
	await new Promise((r) => setTimeout(r, 50));
	return { view };
}

function paste(view: EditorView, types: string[]): Event {
	const items = types.map((type) => ({
		kind: type.startsWith("image/") ? "file" : "string",
		type,
		getAsFile: () => (type.startsWith("image/") ? new File([new Uint8Array([1])], "x", { type }) : null),
	}));
	const e = new Event("paste", { bubbles: true, cancelable: true });
	Object.defineProperty(e, "clipboardData", {
		value: { items, getData: (t: string) => (t === "text/plain" ? "hello" : "") },
	});
	view.contentDOM.dispatchEvent(e);
	return e;
}

describe("ComposePanel paste (critic 1350)", () => {
	it("inserts the image tag at the caret — catches inserting at doc end or position 0", async () => {
		const { view } = await mount();
		view.dispatch({ changes: { from: 0, insert: "abcdef" }, selection: { anchor: 2 } });
		await new Promise((r) => setTimeout(r, 20));
		paste(view, ["image/png"]);
		await waitFor(() => expect(view.state.doc.toString()).toBe("ab[image: /saved/a.png]cdef"));
	});

	it("reuses one note id across pastes — catches a fresh asset directory per paste", async () => {
		const { view } = await mount();
		paste(view, ["image/png"]);
		await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
		paste(view, ["image/png"]);
		await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
		const ids = vi.mocked(invoke).mock.calls.map((c) => (c[1] as { noteId: string }).noteId);
		expect(ids[0]).toBe(ids[1]);
	});

	it("leaves the document unchanged when saving fails — catches inserting `[image: null]`", async () => {
		const failure = new Error("boom");
		const loggedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		vi.mocked(invoke).mockRejectedValue(failure);
		const { view } = await mount();
		view.dispatch({ changes: { from: 0, insert: "keep" } });
		const e = paste(view, ["image/png"]);
		expect(e.defaultPrevented).toBe(true);
		await new Promise((r) => setTimeout(r, 30));
		expect(view.state.doc.toString()).toBe("keep");
		expect(loggedError).toHaveBeenCalledWith("store", "Failed to save pasted image", failure);
		loggedError.mockRestore();
	});

	it("does not claim a text-only paste — catches CodeMirror's default paste being swallowed", async () => {
		const { view } = await mount();
		paste(view, ["text/plain", "text/html"]);
		expect(invoke).not.toHaveBeenCalled();
		expect(view.state.doc.toString()).toBe("hello");
	});

	it("inserts at the caret as it is when the save settles, not where it was at paste — catches stale position after typing", async () => {
		let release!: (p: string) => void;
		vi.mocked(invoke).mockReturnValue(new Promise<string>((r) => (release = r)) as never);
		const { view } = await mount();
		view.dispatch({ changes: { from: 0, insert: "ab" }, selection: { anchor: 1 } });
		paste(view, ["image/png"]);
		view.dispatch({ changes: { from: 0, insert: "XY" }, selection: { anchor: 3 } });
		await new Promise((r) => setTimeout(r, 10));
		release("/p.png");
		await waitFor(() => expect(view.state.doc.toString()).toBe("XYa[image: /p.png]b"));
	});

	it("survives the panel unmounting while the save is pending — catches an unhandled rejection from dispatch on a destroyed view", async () => {
		let release!: (p: string) => void;
		vi.mocked(invoke).mockReturnValue(new Promise<string>((r) => (release = r)) as never);
		const { view } = await mount();
		paste(view, ["image/png"]);
		cleanup();
		const unhandled = vi.fn();
		process.on("unhandledRejection", unhandled);
		release("/p.png");
		await new Promise((r) => setTimeout(r, 30));
		process.off("unhandledRejection", unhandled);
		expect(unhandled).not.toHaveBeenCalled();
	});
});
