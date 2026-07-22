import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => {
	return { mockInvoke: vi.fn().mockResolvedValue(undefined) };
});

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
	convertFileSrc: (path: string) => path,
}));

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

import { NotesPanel } from "../../components/NotesPanel/NotesPanel";
import { notesStore } from "../../stores/notes";

const noop = () => {};

function renderPanel() {
	return render(() => <NotesPanel visible={true} repoPath={null} onClose={noop} onSendToTerminal={noop} />);
}

describe("NotesPanel — IME composition handling", () => {
	beforeEach(() => {
		for (const note of [...notesStore.state.notes]) {
			notesStore.removeNote(note.id);
		}
		mockInvoke.mockClear();
	});

	it("does not submit when Enter is pressed during IME composition (isComposing=true)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "ねこ" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: true, keyCode: 229 });

		expect(notesStore.state.notes.length).toBe(0);
		expect(textarea.value).toBe("ねこ");
	});

	it("does not submit on the IME-confirming Enter even when isComposing has already flipped to false (WebKit quirk)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "猫" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: false, keyCode: 229 });

		expect(notesStore.state.notes.length).toBe(0);
		expect(textarea.value).toBe("猫");
	});

	it("submits normally on a real Enter press (not IME)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "hello world" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: false, keyCode: 13 });

		expect(notesStore.state.notes.length).toBe(1);
		expect(notesStore.state.notes[0].text).toBe("hello world");
	});

	it("still inserts a newline on Shift+Enter without submitting", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "line one" } });
		fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true, isComposing: false, keyCode: 13 });

		expect(notesStore.state.notes.length).toBe(0);
	});
});
