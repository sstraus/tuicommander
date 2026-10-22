import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CodeEditorTab } from "../../components/CodeEditorPanel/CodeEditorTab";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { mockInvoke } from "../mocks/tauri";

// Skip real CodeMirror language-data loading — it's exercised by
// languageDetection's own tests, and mocking it here keeps these tests fast
// and focused on CodeEditorTab's own logic rather than syntax highlighting.
vi.mock("../../components/CodeEditorPanel/languageDetection", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/CodeEditorPanel/languageDetection")>()),
	detectLanguage: vi.fn().mockResolvedValue(null),
}));

/** Dispatch a content change directly into the mounted CodeMirror view — the
 *  editor has no <textarea> to fill, same technique as ComposePanel.test.tsx. */
function typeIntoEditor(container: HTMLElement, text: string): void {
	const editor = container.querySelector(".cm-editor") as HTMLElement | null;
	if (!editor) throw new Error("CodeMirror not mounted");
	const view = EditorView.findFromDOM(editor);
	if (!view) throw new Error("CodeMirror view not attached");
	view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

const FILE_CONTENT = "line one\nline two\n";

function mockInvokeDefaults() {
	mockInvoke.mockImplementation((cmd: string) => {
		switch (cmd) {
			case "read_editor_file":
			case "read_editor_file_external":
				return Promise.resolve(FILE_CONTENT);
			case "mdkb_outline":
				return Promise.resolve([]);
			case "stat_path":
				return Promise.resolve({ exists: false, modified_at: 0, size: 0 });
			case "get_gutter_changes":
				return Promise.resolve([]);
			case "get_file_blame":
				return Promise.resolve([]);
			case "write_file":
			case "write_external_file":
				return Promise.resolve(undefined);
			case "write_clipboard_text":
				return Promise.resolve(undefined);
			default:
				return Promise.resolve(undefined);
		}
	});
}

afterEach(async () => {
	cleanup();
	// happy-dom runs requestAnimationFrame via setImmediate. CodeMirror/ResizeObserver
	// may leave a queued measurement frame while the view tears down — give it a turn
	// inside the test lifecycle so vitest doesn't report it as an async leak.
	await new Promise<void>((resolve) => setImmediate(resolve));
});

beforeEach(() => {
	mockInvoke.mockReset();
	mockInvokeDefaults();
	vi.spyOn(appLogger, "error").mockImplementation(() => {});
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	settingsStore.setInlineBlameEnabled(false);
	for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
	for (const id of diffTabsStore.getIds()) diffTabsStore.remove(id);
});

function renderTab(overrides: Partial<Parameters<typeof CodeEditorTab>[0]> = {}) {
	const props = {
		id: "tab1",
		repoPath: "/repo",
		filePath: "src/index.ts",
		...overrides,
	};
	return { ...render(() => <CodeEditorTab {...props} />), props };
}

/** The .cm-content node exists as soon as CodeMirror mounts — before the async
 *  file read resolves and its content is pushed in. "Loading..." disappearing
 *  is the real signal that the initial read settled. */
async function waitForLoaded(result: { queryByText: (text: string) => HTMLElement | null }) {
	await waitFor(() => expect(result.queryByText("Loading...")).toBeNull());
}

describe("CodeEditorTab", () => {
	it("shows a loading state, then the file content once read", async () => {
		const rendered = renderTab();
		const { container, getByText } = rendered;
		expect(getByText("Loading...")).toBeTruthy();

		await waitForLoaded(rendered);
		expect(mockInvoke).toHaveBeenCalledWith("read_editor_file", { repoPath: "/repo", file: "src/index.ts" });
		expect(container.querySelector(".cm-content")?.textContent).toContain("line one");
	});

	it("reads external (absolute-path) files through the external command, and hides the diff button", async () => {
		const rendered = renderTab({ filePath: "/etc/hosts", repoPath: "" });
		const { queryByTitle } = rendered;

		await waitForLoaded(rendered);
		expect(mockInvoke).toHaveBeenCalledWith("read_editor_file_external", { path: "/etc/hosts" });
		expect(queryByTitle("View diff")).toBeNull();
	});

	it("marks the tab dirty on edit and clears it on save", async () => {
		const rendered = renderTab();
		const { container, getByTitle } = rendered;
		await waitForLoaded(rendered);
		expect(editorTabsStore.getHandle<{ isDirty: () => boolean }>("tab1")?.isDirty()).not.toBe(true);

		typeIntoEditor(container, "changed content");
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(false));

		fireEvent.click(getByTitle(/Save/));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", expect.objectContaining({ file: "src/index.ts" })),
		);
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(true));
	});

	it("bumps the repo revision on a successful save so other panels refresh", async () => {
		const rendered = renderTab();
		const { container, getByTitle } = rendered;
		await waitForLoaded(rendered);
		const before = repositoriesStore.getRevision("/repo");

		typeIntoEditor(container, "changed content");
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(getByTitle(/Save/));

		await waitFor(() => expect(repositoriesStore.getRevision("/repo")).toBeGreaterThan(before));
	});

	it("saves an external file through write_external_file, not the repo file-browser command", async () => {
		const rendered = renderTab({ filePath: "/etc/hosts", repoPath: "", externalEditable: true });
		const { container, getByTitle } = rendered;
		await waitForLoaded(rendered);

		typeIntoEditor(container, "changed content");
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(getByTitle(/Save/));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_external_file", {
				path: "/etc/hosts",
				content: "changed content",
			}),
		);
	});

	it("logs and surfaces an error when save fails, without crashing", async () => {
		const rendered = renderTab();
		const { container, getByTitle } = rendered;
		await waitForLoaded(rendered);
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "write_file" ? Promise.reject(new Error("disk full")) : Promise.resolve(undefined),
		);

		typeIntoEditor(container, "changed content");
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(getByTitle(/Save/));

		await waitFor(() => expect(appLogger.error).toHaveBeenCalledWith("app", "Failed to save file", expect.any(Error)));
		// The save error reuses the same `error` signal as a failed load, which
		// replaces the editor body with an error banner — documented existing
		// behavior, not something this test suite is asserting should change.
		await waitFor(() => expect(container.textContent).toContain("disk full"));
	});

	it("toggles read-only mode and hides the save button while locked", async () => {
		const rendered = renderTab();
		const { container, getByTitle, queryByTitle } = rendered;
		await waitForLoaded(rendered);
		typeIntoEditor(container, "changed content");
		await waitFor(() => expect((getByTitle(/Save/) as HTMLButtonElement).disabled).toBe(false));

		fireEvent.click(getByTitle("Lock (read-only)"));

		expect(queryByTitle(/Save/)).toBeNull();
		expect(getByTitle("Unlock editing")).toBeTruthy();
	});

	it("starts external files read-only when externalEditable is false", async () => {
		const rendered = renderTab({ filePath: "/etc/hosts", repoPath: "", externalEditable: false });
		const { getByTitle } = rendered;
		await waitForLoaded(rendered);

		expect(getByTitle("Unlock editing")).toBeTruthy();
	});

	it("shows a 'not displayable' notice for a binary/non-UTF-8 file", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "read_editor_file"
				? Promise.reject(new Error("stream did not contain valid UTF-8"))
				: Promise.resolve([]),
		);
		const { getByText } = renderTab();

		await waitFor(() => expect(getByText("This file can't be displayed")).toBeTruthy());
	});

	it("shows a 'too large' notice when the backend refuses an oversized file", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "read_editor_file" ? Promise.reject(new Error("file too large to open")) : Promise.resolve([]),
		);
		const { getByText } = renderTab();

		await waitFor(() => expect(getByText("File too large to open")).toBeTruthy());
	});

	it("copies the shortened absolute path via the 'Copy Path' context-menu action", async () => {
		const rendered = renderTab();
		const { container, getByText } = rendered;
		await waitForLoaded(rendered);
		const header = container.querySelector('[title="src/index.ts"]')?.parentElement as HTMLElement;

		fireEvent.contextMenu(header);
		fireEvent.click(await waitFor(() => getByText("Copy Path")));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", {
				text: "/repo/src/index.ts",
			}),
		);
	});

	it("shows a disk-conflict banner when the file changed on disk while locally edited, and 'Reload' discards local changes", async () => {
		const rendered = renderTab();
		const { container, getByText } = rendered;
		await waitForLoaded(rendered);

		typeIntoEditor(container, "local edit");
		await waitFor(() => expect(container.querySelector(".cm-content")?.textContent).toContain("local edit"));

		// Simulate the file changing on disk: stat differs, and a fresh read returns
		// content that differs from what was last saved.
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "stat_path") return Promise.resolve({ exists: true, modified_at: 1, size: 99 });
			if (cmd === "read_editor_file") return Promise.resolve("changed on disk\n");
			return Promise.resolve([]);
		});
		repositoriesStore.bumpRevision("/repo");

		await waitFor(() => expect(getByText("File changed on disk.")).toBeTruthy());

		fireEvent.click(getByText("Reload"));
		await waitFor(() => expect(container.querySelector(".cm-content")?.textContent).toContain("changed on disk"));
	});
});
