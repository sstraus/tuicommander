import { EditorView } from "@codemirror/view";
import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CodeEditorTab } from "../../components/CodeEditorPanel/CodeEditorTab";
import { editorTabsStore } from "../../stores/editorTabs";
import { nextAnimationFrame } from "../helpers/animationFrame";

const disk = new Map<string, string>();
const failing = new Set<string>();

vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string, args?: { file?: string }) => {
		if (command === "read_editor_file") {
			const file = args?.file ?? "";
			if (failing.has(file)) throw new Error("read failed");
			return disk.get(file) ?? "";
		}
		if (command === "stat_path") return null;
		if (command === "mdkb_outline" || command === "get_gutter_changes" || command === "get_file_blame") return [];
		return undefined;
	}),
}));
vi.mock("../../hooks/useFileBrowser", () => ({ useFileBrowser: () => ({ writeFile: vi.fn() }) }));

const opened: string[] = [];
function openTab(file: string): string {
	const id = editorTabsStore.add("/repo", file, undefined, { background: true });
	opened.push(id);
	return id;
}

function mount(id: string) {
	const result = render(() => (
		<CodeEditorTab id={id} repoPath="/repo" filePath={editorTabsStore.get(id)?.filePath ?? ""} />
	));
	const view = async () => {
		await waitFor(() => expect(result.container.querySelector(".cm-editor")).not.toBeNull());
		return EditorView.findFromDOM(result.container.querySelector<HTMLElement>(".cm-editor")!)!;
	};
	return { ...result, view };
}

afterEach(async () => {
	await nextAnimationFrame();
	for (const id of opened.splice(0)) editorTabsStore.remove(id);
	disk.clear();
	failing.clear();
	document.body.innerHTML = "";
});

describe("editor reload draft boundaries", () => {
	// Catches: only the first dirtying keystroke reaches the draft, so a crash
	// reload restores text older than the last edit.
	it("every real edit updates the stored draft and its baseline", async () => {
		disk.set("a.ts", "disk");
		const id = openTab("a.ts");
		const { view, unmount } = mount(id);
		const editor = await view();
		await waitFor(() => expect(editor.state.doc.toString()).toBe("disk"));
		editor.dispatch({ changes: { from: 4, insert: "!" } });
		editor.dispatch({ changes: { from: 5, insert: "?" } });
		expect(editorTabsStore.get(id)?.reloadDraft).toEqual({ text: "disk!?", savedContent: "disk" });
		editor.dispatch({ changes: { from: 4, to: 6, insert: "" } });
		expect(editorTabsStore.get(id)?.reloadDraft).toBeUndefined();
		unmount();
	});

	// Catches: an external write made before the reload is silently overwritten on save.
	it("a recovered draft over changed disk content raises the conflict banner", async () => {
		disk.set("c.ts", "agent rewrote this");
		const id = openTab("c.ts");
		editorTabsStore.setReloadDraft(id, { text: "base + edit", savedContent: "base" });
		const { view, findByText, unmount } = mount(id);
		const editor = await view();
		await waitFor(() => expect(editor.state.doc.toString()).toBe("base + edit"));
		expect(await findByText("Keep mine")).toBeTruthy();
		unmount();
	});

	// Catches: an MCP reopen retargets the tab to another file but the
	// store merge keeps the old file's draft, which loadFile then shows (and a save
	// writes) as the new file's content.
	it("retargeting an MCP editor tab to another file never shows the old file's draft", async () => {
		disk.set("old.ts", "old base");
		disk.set("new.ts", "new content");
		const id = editorTabsStore.addMcpFile("doc", "/repo", "old.ts", undefined, false, {
			background: true,
			externalEditable: false,
		});
		opened.push(id);
		const { view, unmount } = mount(id);
		const editor = await view();
		await waitFor(() => expect(editor.state.doc.toString()).toBe("old base"));
		editor.dispatch({ changes: { from: 0, insert: "edited " } });
		editorTabsStore.addMcpFile("doc", "/repo", "new.ts", undefined, false, {
			background: true,
			externalEditable: false,
		});
		await waitFor(() => expect(editor.state.doc.toString()).toBe("new content"));
		expect(editorTabsStore.get(id)?.reloadDraft).toBeUndefined();
		unmount();
	});
	// Catches: disk already contains the recovered text, yet reload reports dirty/conflict.
	it("clears a draft already saved to disk without a conflict", async () => {
		disk.set("saved.ts", "already saved");
		const id = openTab("saved.ts");
		editorTabsStore.setReloadDraft(id, { text: "already saved", savedContent: "old" });
		const { view, queryByText, unmount } = mount(id);
		const editor = await view();
		await waitFor(() => expect(editor.state.doc.toString()).toBe("already saved"));
		expect(editorTabsStore.get(id)?.isDirty).toBe(false);
		expect(editorTabsStore.get(id)?.reloadDraft).toBeUndefined();
		expect(queryByText("Keep mine")).toBeNull();
		unmount();
	});
	// Catches: an MCP cursor jump in the same file clears the only recoverable unsaved buffer.
	it("keeps the draft and dirty indicator when MCP reopens the same file", async () => {
		disk.set("same.ts", "baseline");
		const opts = { background: true, externalEditable: false };
		const id = editorTabsStore.addMcpFile("same-document", "/repo", "same.ts", undefined, false, opts);
		opened.push(id);
		const { view, unmount } = mount(id);
		const editor = await view();
		await waitFor(() => expect(editor.state.doc.toString()).toBe("baseline"));
		editor.dispatch({ changes: { from: 8, insert: " edited" } });
		editorTabsStore.addMcpFile("same-document", "/repo", "same.ts", 2, false, opts);
		expect(editor.state.doc.toString()).toBe("baseline edited");
		expect(editorTabsStore.get(id)?.isDirty).toBe(true);
		expect(editorTabsStore.get(id)?.reloadDraft).toEqual({ text: "baseline edited", savedContent: "baseline" });
		unmount();
	});
});
