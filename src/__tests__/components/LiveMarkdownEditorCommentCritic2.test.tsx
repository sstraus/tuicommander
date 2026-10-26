// @vitest-environment jsdom

import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { LiveMarkdownEditor } from "../../components/MarkdownTab/LiveMarkdownEditor";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { cleanupToasts } from "../helpers/toasts";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

// Real toasts mirror activity; load its in-memory persistence baseline first.
beforeAll(async () => {
	await activityStore.hydrate();
});

afterEach(async () => {
	cleanup();
	cleanupToasts();
	vi.restoreAllMocks();
	// Drain CodeMirror's dependency-owned 10ms focus notification after cleanup.
	await new Promise((resolve) => setTimeout(resolve, 20));
});

const PLACEHOLDER = "Comment on the selection";

async function composer() {
	const { container } = render(() => (
		<LiveMarkdownEditor
			content="hello world"
			onSave={vi.fn().mockResolvedValue(true)}
			readDisk={async () => "hello world"}
		/>
	));
	const view = () => EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement) as EditorView;
	const block = await waitFor(() => {
		const el = container.querySelector<HTMLElement>("[data-comment-source-start]");
		expect(el).not.toBeNull();
		return el as HTMLElement;
	});
	fireEvent.click(block);
	await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
	view().focus();
	view().dispatch({ selection: { anchor: 0, head: 5 } });
	const openComposer = async () => {
		fireEvent.click(await screen.findByText("Comment"));
		return (await screen.findByPlaceholderText(PLACEHOLDER)) as HTMLInputElement;
	};
	const closed = () => waitFor(() => expect(screen.queryByPlaceholderText(PLACEHOLDER)).toBeNull());
	return { view, openComposer, closed };
}

describe("comment composer IME guard and draft reset (critic 1293 round 2)", () => {
	it("WebKit Enter (keyCode 229, isComposing already false) does not submit and keeps the draft", async () => {
		// catches: guard checks only isComposing, so WebKit's post-compositionend Enter posts the half-typed comment
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "にほ" } });
		const notPrevented = fireEvent.keyDown(input, { key: "Enter", isComposing: false, keyCode: 229 });
		expect(t.view().state.sliceDoc()).toBe("hello world");
		expect(screen.getByPlaceholderText(PLACEHOLDER)).toHaveProperty("value", "にほ");
		expect(notPrevented).toBe(true);
	});

	it("WebKit Escape (keyCode 229) does not close the composer or drop the draft", async () => {
		// catches: Escape that dismisses an IME candidate list cancels the whole comment
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "にほ" } });
		fireEvent.keyDown(input, { key: "Escape", isComposing: false, keyCode: 229 });
		expect(screen.getByPlaceholderText(PLACEHOLDER)).toHaveProperty("value", "にほ");
	});

	it("Escape with isComposing true does not close the composer", async () => {
		// catches: Escape guard covers only Enter
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "x" } });
		fireEvent.keyDown(input, { key: "Escape", isComposing: true });
		expect(screen.getByPlaceholderText(PLACEHOLDER)).toHaveProperty("value", "x");
		expect(t.view().state.sliceDoc()).toBe("hello world");
	});

	it("a real Enter after an ignored IME Enter still commits the intact draft", async () => {
		// catches: the ignored IME key clears or corrupts the draft, or leaves the handler latched
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "note" } });
		fireEvent.keyDown(input, { key: "Enter", isComposing: true, keyCode: 229 });
		fireEvent.keyDown(input, { key: "Enter", keyCode: 13 });
		expect(t.view().state.sliceDoc()).toMatch(/note/);
	});

	it("Escape discards the draft: reopening shows an empty input", async () => {
		// catches: cancel hides the input but keeps the draft signal, so the next comment starts with stale text
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "stale" } });
		fireEvent.keyDown(input, { key: "Escape", keyCode: 27 });
		await t.closed();
		t.view().dispatch({ selection: { anchor: 0, head: 5 } });
		const again = await t.openComposer();
		expect(again.value).toBe("");
		expect(t.view().state.sliceDoc()).toBe("hello world");
	});

	it("Cancel button discards the draft: reopening shows an empty input", async () => {
		// catches: only the Escape path was switched to cancelComment, the button still keeps the draft
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "stale" } });
		fireEvent.click(screen.getByText("Cancel"));
		await t.closed();
		t.view().dispatch({ selection: { anchor: 0, head: 5 } });
		const again = await t.openComposer();
		expect(again.value).toBe("");
	});

	it("Escape returns focus to the editor and its default action is prevented", async () => {
		// catches: Escape leaves focus on <body>, or its default reaches another handler
		const t = await composer();
		const input = await t.openComposer();
		const notPrevented = fireEvent.keyDown(input, { key: "Escape", keyCode: 27 });
		await t.closed();
		expect(notPrevented).toBe(false);
		expect(document.activeElement).toBe(t.view().contentDOM);
	});

	it("Enter with a blank draft is swallowed: no comment, composer stays open", async () => {
		// catches: whitespace-only Enter writes an empty comment or closes the composer
		const t = await composer();
		const input = await t.openComposer();
		fireEvent.input(input, { target: { value: "   " } });
		const notPrevented = fireEvent.keyDown(input, { key: "Enter", keyCode: 13 });
		expect(notPrevented).toBe(false);
		expect(t.view().state.sliceDoc()).toBe("hello world");
		expect(screen.getByPlaceholderText(PLACEHOLDER)).toBeTruthy();
	});

	it("an overlapping comment keeps the composer and the draft so the text is not lost", async () => {
		// catches: the error path runs cancelComment/clears the draft, discarding what the user typed
		const error = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		const t = await composer();
		const first = await t.openComposer();
		fireEvent.input(first, { target: { value: "one" } });
		fireEvent.keyDown(first, { key: "Enter", keyCode: 13 });
		await t.closed();
		const before = t.view().state.sliceDoc();
		t.view().dispatch({ selection: { anchor: 0, head: 5 } });
		const second = await t.openComposer();
		fireEvent.input(second, { target: { value: "two" } });
		fireEvent.keyDown(second, { key: "Enter", keyCode: 13 });
		expect(t.view().state.sliceDoc()).toBe(before);
		expect(screen.getByPlaceholderText(PLACEHOLDER)).toHaveProperty("value", "two");
		expect(error).toHaveBeenCalledExactlyOnceWith("app", "live addTweakComment failed", expect.any(Error));
	});
});
