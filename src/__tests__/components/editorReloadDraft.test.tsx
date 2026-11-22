import { EditorView } from "@codemirror/view";
import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CodeEditorTab } from "../../components/CodeEditorPanel/CodeEditorTab";
import { editorTabsStore } from "../../stores/editorTabs";
import { uiStore } from "../../stores/ui";
import { nextAnimationFrame } from "../helpers/animationFrame";

const io = vi.hoisted(() => ({ text: "disk baseline", failRead: false }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string) => {
		if (command === "read_editor_file") {
			if (io.failRead) throw new Error("File unavailable");
			return io.text;
		}
		if (command === "stat_path") return { exists: true, modified_at: 1, size: io.text.length };
		if (["mdkb_outline", "get_gutter_changes", "get_file_blame"].includes(command)) return [];
		return undefined;
	}),
}));
vi.mock("../../hooks/useFileBrowser", () => ({
	useFileBrowser: () => ({
		writeFile: vi.fn(async (_root, _file, text) => {
			io.text = text;
		}),
	}),
}));

async function mount(id: string) {
	const mounted = render(() => <CodeEditorTab id={id} repoPath="/repo" filePath="draft.txt" />);
	await waitFor(() => expect(mounted.container.querySelector(".cm-editor")).not.toBeNull());
	const view = EditorView.findFromDOM(mounted.container.querySelector<HTMLElement>(".cm-editor")!)!;
	return { ...mounted, view };
}

beforeEach(() => {
	sessionStorage.clear();
	editorTabsStore.clearAll();
	io.text = "disk baseline";
	io.failRead = false;
});
afterEach(async () => {
	await nextAnimationFrame();
	uiStore._testCancelPendingSave();
	document.body.innerHTML = "";
});

describe("editor reload drafts", () => {
	// Catches: a recovered tab loads disk bytes over its unsaved text (including deletion of all text).
	it.each(["unsaved user text", ""])(
		"keeps the unsaved buffer %j across component recreation and clears it on save",
		async (draft) => {
			const id = editorTabsStore.add("/repo", "draft.txt");
			const old = await mount(id);
			await waitFor(() => expect(old.view.state.doc.toString()).toBe("disk baseline"));
			old.view.dispatch({ changes: { from: 0, to: old.view.state.doc.length, insert: draft } });
			await waitFor(() => expect(editorTabsStore.get(id)?.isDirty).toBe(true));
			old.unmount();
			const next = await mount(id);
			await waitFor(() => expect(next.view.state.doc.toString()).toBe(draft));
			expect(editorTabsStore.get(id)?.isDirty).toBe(true);
			expect(io.text).toBe("disk baseline");
			await editorTabsStore.getHandle<{ save: () => Promise<void> }>(id)!.save();
			expect(io.text).toBe(draft);
			expect(editorTabsStore.get(id)?.reloadDraft).toBeUndefined();
			next.unmount();
		},
	);

	// Catches: a failed disk read replaces the only retained unsaved buffer with an empty string.
	it("retains the recovered buffer when the file cannot be read", async () => {
		const id = editorTabsStore.add("/repo", "draft.txt");
		const old = await mount(id);
		await waitFor(() => expect(old.view.state.doc.toString()).toBe("disk baseline"));
		old.view.dispatch({ changes: { from: 0, to: old.view.state.doc.length, insert: "not on disk" } });
		await waitFor(() => expect(editorTabsStore.get(id)?.isDirty).toBe(true));
		old.unmount();
		io.failRead = true;
		const next = render(() => <CodeEditorTab id={id} repoPath="/repo" filePath="draft.txt" />);
		await waitFor(() => expect(next.container.textContent).toContain("File unavailable"));
		expect(editorTabsStore.get(id)?.reloadDraft?.text).toBe("not on disk");
		next.unmount();
		expect(editorTabsStore.get(id)?.reloadDraft?.text).toBe("not on disk");
	});
});
