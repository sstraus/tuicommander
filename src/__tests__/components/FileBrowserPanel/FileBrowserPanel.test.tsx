import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ContentMatch, ContentSearchBatch, DirEntry } from "../../../types/fs";

const { mockInvoke, mockListen, listeners } = vi.hoisted(() => {
	const listeners = new Map<string, Array<(payload: unknown) => void>>();
	return {
		listeners,
		mockInvoke: vi.fn(),
		mockListen: vi.fn((event: string, cb: (e: { payload: unknown }) => void) => {
			const wrapped = (payload: unknown) => cb({ payload });
			const arr = listeners.get(event) ?? [];
			arr.push(wrapped);
			listeners.set(event, arr);
			return Promise.resolve(() => {
				const current = listeners.get(event) ?? [];
				const idx = current.indexOf(wrapped);
				if (idx !== -1) current.splice(idx, 1);
			});
		}),
	};
});

vi.mock("../../../invoke", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../../invoke")>();
	return {
		...actual,
		invoke: mockInvoke,
		listen: (event: string, callback: (event: { payload: unknown }) => void) =>
			"__TAURI_SHIM__" in globalThis ? actual.listen(event, callback) : mockListen(event, callback),
	};
});

const emitEvent = (event: string, payload: unknown) => {
	for (const handler of [...(listeners.get(event) ?? [])]) handler(payload);
};

const { FileBrowserPanel } = await import("../../../components/FileBrowserPanel/FileBrowserPanel");
const { uiStore } = await import("../../../stores/ui");
const { mdTabsStore } = await import("../../../stores/mdTabs");
const { editorTabsStore } = await import("../../../stores/editorTabs");
const { repositoriesStore } = await import("../../../stores/repositories");

const dir = (name: string, path = name): DirEntry => ({
	name,
	path,
	is_dir: true,
	size: 0,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

const file = (name: string, path = name): DirEntry => ({
	name,
	path,
	is_dir: false,
	size: 10,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

const match = (path: string, line: number): ContentMatch => ({
	path,
	line_number: line,
	line_text: `hit on line ${line}`,
	match_start: 0,
	match_end: 3,
});

const batch = (matches: ContentMatch[], isFinal: boolean, searchId: string): ContentSearchBatch => ({
	search_id: searchId,
	matches,
	is_final: isFinal,
	files_searched: 1,
	files_skipped: 0,
	truncated: false,
	repos_pending: 0,
	repos_indexing: 0,
	repos_searched: 0,
});

const unicodeMatch: ContentMatch = {
	path: "unicode.ts",
	line_number: 4,
	// `hit` starts at byte 12, Unicode scalar 6, and JavaScript/UTF-16 offset 7.
	line_text: "— é 😀 hit tail",
	match_start: 7,
	match_end: 10,
};

/** `list_directory` responses keyed by `${repoPath}|${subdir}`. */
let listings = new Map<string, DirEntry[]>();
/** Every `list_directory` call, in order. */
let listCalls: Array<{ repoPath: string; subdir: string }> = [];

// uiStore persists its prefs on a debounced timer, so a write can still land
// after a test finished — with no implementation it would return undefined and
// the store's `.catch` would throw.
const defaultInvoke = (cmd: string, args?: Record<string, unknown>) => {
	if (cmd === "list_directory") {
		const repoPath = String(args?.repoPath);
		const subdir = String(args?.subdir);
		listCalls.push({ repoPath, subdir });
		return Promise.resolve(listings.get(`${repoPath}|${subdir}`) ?? []);
	}
	return Promise.resolve(undefined);
};

mockInvoke.mockImplementation(defaultInvoke);
// Match app startup: preference writes must have a loaded comparison base.
await uiStore.hydrate();

const setBrowserMode = (enabled: boolean) => {
	const globals = globalThis as Record<string, unknown>;
	if (enabled) globals.__TAURI_SHIM__ = {};
	else delete globals.__TAURI_SHIM__;
};

beforeEach(() => {
	listings = new Map();
	listCalls = [];
	listeners.clear();
	mockInvoke.mockReset().mockImplementation(defaultInvoke);
	setBrowserMode(false);
	uiStore.setFileBrowserViewMode("flat");
});

// The view-mode toggle persists prefs on a 500 ms debounce; let that timer fire
// before teardown so it does not leak into the next test file.
afterEach(async () => {
	vi.unstubAllGlobals();
	setBrowserMode(false);
	uiStore.setFileBrowserViewMode("flat");
	await new Promise((resolve) => setTimeout(resolve, 600));
});

const countListCalls = (repoPath: string, subdir: string) =>
	listCalls.filter((c) => c.repoPath === repoPath && c.subdir === subdir).length;

const clickTreeViewButton = (container: HTMLElement) => {
	const btn = container.querySelector('button[title="Tree view"]') as HTMLButtonElement;
	fireEvent.click(btn);
};

const clickListViewButton = (container: HTMLElement) => {
	const btn = container.querySelector('button[title="List view"]') as HTMLButtonElement;
	fireEvent.click(btn);
};

/** Click the row whose name label is `name`. */
const clickRow = (container: HTMLElement, name: string) => {
	const label = Array.from(container.querySelectorAll(".entryName")).find((el) => el.textContent === name);
	if (!label) throw new Error(`row not found: ${name}`);
	fireEvent.click(label.parentElement as HTMLElement);
};

describe("FileBrowserPanel external transfer refresh", () => {
	it.each([
		{ name: "external folder drop into an expanded tree folder", confirm: false, copy: false, remote: false },
		{ name: "confirmed recursive drop", confirm: true, copy: false, remote: false },
		{ name: "move source in another expanded folder of the same tree", confirm: false, copy: false, remote: false },
		{ name: "copy leaves the source visible and reloads the destination", confirm: false, copy: true, remote: false },
		{ name: "remote copy reloads the owning destination", confirm: true, copy: true, remote: true },
	])("$name — catches stale transfer children", async ({ name, confirm, copy, remote }) => {
		const { dispatchTauriDrop, confirmFolderDrop, setFolderDropConfirmHandler } = await import(
			"../../../hooks/useFileDrop"
		);
		const { dragDropStore } = await import("../../../stores/dragDrop");
		const owner = vi.spyOn(repositoriesStore, "getConnectionId").mockReturnValue(remote ? "mint" : undefined);
		const { toastsStore } = await import("../../../stores/toasts");
		const toast = vi.spyOn(toastsStore, "add").mockReturnValue(0);
		const modifier = vi.spyOn(dragDropStore, "copyModifierHeld").mockReturnValue(copy);
		listings.set("/repo|.", [dir("dest"), dir("source")]);
		listings.set("/repo|dest", [file("existing.txt", "dest/existing.txt")]);
		listings.set("/repo|source", [dir("incoming", "source/incoming")]);
		const originalHitTest = Object.getOwnPropertyDescriptor(document, "elementFromPoint");
		const pending = vi.fn();
		setFolderDropConfirmHandler(pending);
		mockInvoke.mockImplementation((cmd, args) => {
			if (cmd === "fs_transfer_paths" || cmd === "fs_transfer_remote_paths") {
				if (confirm && !args?.allowRecursive) {
					return Promise.resolve({ moved: 0, skipped: 0, errors: [], needs_confirm: true });
				}
				listings.set("/repo|dest", [file("existing.txt", "dest/existing.txt"), dir("incoming", "dest/incoming")]);
				if (!copy) listings.set("/repo|source", []);
				return Promise.resolve({ moved: 1, skipped: 0, errors: [], needs_confirm: false });
			}
			return defaultInvoke(cmd, args);
		});
		try {
			const { container } = render(() => (
				<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
			));
			clickTreeViewButton(container);
			await waitFor(() => expect(container.textContent).toContain("dest"));
			clickRow(container, "dest");
			clickRow(container, "source");
			await waitFor(() => expect(container.querySelector('[data-abs-path="/repo/source/incoming"]')).not.toBeNull());
			await waitFor(() => expect(container.textContent).toContain("existing.txt"));
			const dest = container.querySelector('[data-abs-path="/repo/dest"]');
			Object.defineProperty(document, "elementFromPoint", { configurable: true, writable: true, value: () => dest });
			await dispatchTauriDrop(["/repo/source/incoming"], 10, 10);
			if (confirm) {
				expect(pending).toHaveBeenCalledOnce();
				expect(countListCalls("/repo", "dest")).toBe(1);
				await confirmFolderDrop(pending.mock.calls[0][0]);
			}
			if (name.startsWith("move source")) {
				await waitFor(() => expect(container.querySelector('[data-abs-path="/repo/source/incoming"]')).toBeNull());
			}
			await waitFor(() => expect(container.querySelector('[data-abs-path="/repo/dest/incoming"]')).not.toBeNull());
			expect(container.querySelector('[data-abs-path="/repo/source/incoming"]') !== null).toBe(copy);
		} finally {
			if (originalHitTest) Object.defineProperty(document, "elementFromPoint", originalHitTest);
			else Reflect.deleteProperty(document, "elementFromPoint");
			toast.mockRestore();
			owner.mockRestore();
			modifier.mockRestore();
			setFolderDropConfirmHandler(null);
		}
	});
});

describe("FileBrowserPanel tree cache", () => {
	// Catches: remote folder descendants losing the panel's actual repository owner.
	it("exposes the repository connection on the native file-drop surface", () => {
		const owner = vi.spyOn(repositoriesStore, "getConnectionId").mockReturnValue("mint");
		try {
			const { container } = render(() => (
				<FileBrowserPanel visible={true} repoPath="/remote/repo" onClose={() => {}} onFileOpen={() => {}} />
			));
			expect(container.querySelector("#file-browser-panel")?.getAttribute("data-drop-connection-id")).toBe("mint");
			expect(owner).toHaveBeenCalledWith("/remote/repo");
		} finally {
			owner.mockRestore();
		}
	});

	it("clears cached children and expanded dirs when the repo changes", async () => {
		listings.set("/repoA|.", [dir("src")]);
		listings.set("/repoA|src", [file("from-repo-a.ts", "src/from-repo-a.ts")]);
		listings.set("/repoB|.", [dir("src")]);
		listings.set("/repoB|src", [file("from-repo-b.ts", "src/from-repo-b.ts")]);

		const [repoPath, setRepoPath] = createSignal("/repoA");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath={repoPath()} onClose={() => {}} onFileOpen={() => {}} />
		));

		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());

		clickRow(container, "src");
		await waitFor(() => expect(queryByText("from-repo-a.ts")).not.toBeNull());

		setRepoPath("/repoB");
		await waitFor(() => expect(countListCalls("/repoB", ".")).toBeGreaterThan(0));

		// repoB's tree must not render repoA's children, and nothing may stay expanded.
		expect(queryByText("from-repo-a.ts")).toBeNull();
		expect(countListCalls("/repoB", "src")).toBe(0);
	});

	it("invalidates the cached children of the directory a dir-changed event names", async () => {
		listings.set("/repo|.", [dir("src")]);
		listings.set("/repo|src", [file("a.ts", "src/a.ts")]);

		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());

		// Expand, then collapse: children are now cached under the relative key "src".
		clickRow(container, "src");
		await waitFor(() => expect(countListCalls("/repo", "src")).toBe(1));
		clickRow(container, "src");

		// Browse into src in flat view so the dir watcher covers /repo/src.
		clickListViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		clickRow(container, "src");
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("start_dir_watcher", { path: "/repo/src" }));

		emitEvent("dir-changed", { dir_path: "/repo/src" });

		// Back to the tree: the invalidated subtree must be fetched again on expand.
		clickTreeViewButton(container);
		await waitFor(() => expect(container.querySelector(".breadcrumb")).toBeNull());
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		const before = countListCalls("/repo", "src");
		clickRow(container, "src");
		await waitFor(() => expect(countListCalls("/repo", "src")).toBe(before + 1));
	});
});

describe("FileBrowserPanel streamed content search", () => {
	/** The correlation id the panel sent with its `search_content` invoke. */
	const sentSearchId = (): string | null => {
		const call = [...mockInvoke.mock.calls].reverse().find(([cmd]) => cmd === "search_content");
		return (call?.[1] as { searchId?: string } | undefined)?.searchId ?? null;
	};

	const startContentSearch = async (container: HTMLElement): Promise<string> => {
		const modeToggle = container.querySelector('button[title="Switch to content search"]') as HTMLButtonElement;
		fireEvent.click(modeToggle);
		const input = container.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "hit" } });
		await waitFor(() => expect(sentSearchId()).not.toBeNull(), { timeout: 2000 });
		return sentSearchId() as string;
	};

	it("renders UTF-16 offsets from desktop streamed batches", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);
		emitEvent("content-search-batch", batch([match("ascii.ts", 1), unicodeMatch], true, searchId));

		await waitFor(() => expect(container.querySelectorAll(".contentMatchHighlight").length).toBe(2));
		expect(Array.from(container.querySelectorAll(".contentMatchHighlight"), (node) => node.textContent)).toEqual([
			"hit",
			"hit",
		]);
		expect(Array.from(container.querySelectorAll(".contentMatchText"), (node) => node.textContent)).toEqual([
			"hit on line 1",
			unicodeMatch.line_text,
		]);
	});

	it("renders UTF-16 offsets returned by the browser HTTP transport", async () => {
		setBrowserMode(true);
		mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
			if (cmd === "search_content") {
				return Promise.resolve({
					matches: [unicodeMatch],
					files_searched: 1,
					files_skipped: 0,
					truncated: false,
					repos_pending: 0,
					repos_indexing: 0,
					repos_searched: 0,
				});
			}
			return defaultInvoke(cmd, args);
		});

		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await startContentSearch(container);

		await waitFor(() => expect(container.querySelector(".contentMatchHighlight")?.textContent).toBe("hit"));
		expect(container.querySelector(".contentMatchText")?.textContent).toBe(unicodeMatch.line_text);
	});

	it("keeps existing group and match rows across streamed batches", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);

		emitEvent("content-search-batch", batch([match("a.ts", 1), match("a.ts", 2)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentGroup").length).toBe(1));

		const firstGroup = container.querySelector(".contentGroup") as HTMLElement;
		const firstMatchRows = Array.from(container.querySelectorAll(".contentMatch"));
		expect(firstMatchRows.length).toBe(2);

		// A later batch adds a match to the same file plus a new file.
		emitEvent("content-search-batch", batch([match("a.ts", 3), match("b.ts", 9)], true, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentGroup").length).toBe(2));

		// The already-rendered group and its rows must be the very same DOM nodes.
		expect(container.querySelector(".contentGroup")).toBe(firstGroup);
		const rowsAfter = Array.from(container.querySelectorAll(".contentMatch"));
		expect(rowsAfter.slice(0, 2)).toEqual(firstMatchRows);
		expect(rowsAfter.length).toBe(4);
	});

	it("counts matches and files across batches in the status bar", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);

		emitEvent("content-search-batch", batch([match("a.ts", 1), match("a.ts", 2)], false, searchId));
		emitEvent("content-search-batch", batch([match("a.ts", 3), match("b.ts", 9)], true, searchId));

		await waitFor(() => expect(container.querySelector(".searchStatus")?.textContent).toBe("4 matches in 2 files"));
		expect(container.querySelector(".contentGroupCount")?.textContent).toBe("3 matches");
	});

	/// `content-search-batch` is one global event with three panels listening. The
	/// backend cancels the previous search when a new one starts, but the listeners
	/// are not mutually exclusive — so the command palette's batches used to append
	/// to this panel's list and flip its spinner off on someone else's `is_final`.
	it("ignores batches belonging to another panel's search", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);
		emitEvent("content-search-batch", batch([match("a.ts", 1)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(1));

		// The command palette's search answers while this one is still streaming.
		emitEvent("content-search-batch", batch([match("other.ts", 7), match("other.ts", 8)], true, "cs-someone-else"));

		expect(container.querySelectorAll(".contentMatch").length).toBe(1);
		// Its `is_final` must not stop this panel's spinner either.
		expect(container.querySelector(".searchStatus")?.textContent).toBe("Searching…");

		// …and this panel's own final batch still lands, and does stop it.
		emitEvent("content-search-batch", batch([match("a.ts", 2)], true, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(2));
		expect(container.querySelector(".searchStatus")?.textContent).toBe("2 matches in 1 file");
	});

	/// Starting any other content search cancels this one — the backend has a
	/// single cancellation slot for the whole process. The cancelled search still
	/// emits an empty final batch under its own id (`dispatch_content_batches` in
	/// `fs.rs`), and that terminator is the only thing that can stop this
	/// spinner: the search that cancelled it carries a different `search_id` and
	/// is dropped above. Without it the panel spins for the life of the window.
	it("stops searching on the empty final batch a cancelled search sends", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);
		emitEvent("content-search-batch", batch([match("a.ts", 1)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(1));

		// Cancelled mid-stream: no further matches, only the terminator.
		emitEvent("content-search-batch", batch([], true, searchId));

		await waitFor(() => expect(container.querySelector(".searchStatus")?.textContent).toBe("1 match in 1 file"));
		// What it did find before being cut short stays on screen.
		expect(container.querySelectorAll(".contentMatch").length).toBe(1);
	});
});

describe("FileBrowserPanel active-file highlight", () => {
	afterEach(() => {
		mdTabsStore.clearAll();
		editorTabsStore.clearAll();
	});

	/** The row classes of the entry whose name label is `name`. */
	const rowClasses = (container: HTMLElement, name: string) => {
		const label = Array.from(container.querySelectorAll(".entryName")).find((el) => el.textContent === name);
		if (!label) throw new Error(`row not found: ${name}`);
		return (label.parentElement as HTMLElement).className;
	};

	it("highlights the file behind an HTML preview tab", async () => {
		listings.set("/repo|.", [file("page.html"), file("other.html")]);
		mdTabsStore.addHtmlPreview("/repo", "page.html");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("page.html")).not.toBeNull());

		await waitFor(() => expect(rowClasses(container, "page.html")).toContain("entryActive"));
		expect(rowClasses(container, "other.html")).not.toContain("entryActive");
	});

	it("highlights the file behind a markdown tab", async () => {
		listings.set("/repo|.", [file("README.md"), file("CHANGELOG.md")]);
		mdTabsStore.add("/repo", "README.md");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("README.md")).not.toBeNull());

		await waitFor(() => expect(rowClasses(container, "README.md")).toContain("entryActive"));
		expect(rowClasses(container, "CHANGELOG.md")).not.toContain("entryActive");
	});

	it("highlights nothing for a markdown tab with no file behind it", async () => {
		listings.set("/repo|.", [file("README.md")]);
		mdTabsStore.addVirtual("Plan", "plan:file?path=/plan.md", "/repo");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("README.md")).not.toBeNull());

		expect(container.querySelector(".entryActive")).toBeNull();
	});

	it("prefers the editor tab over the markdown panel", async () => {
		listings.set("/repo|.", [file("main.ts"), file("page.html")]);
		mdTabsStore.addHtmlPreview("/repo", "page.html");
		editorTabsStore.add("/repo", "main.ts");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("main.ts")).not.toBeNull());

		await waitFor(() => expect(rowClasses(container, "main.ts")).toContain("entryActive"));
		expect(rowClasses(container, "page.html")).not.toContain("entryActive");
	});

	it("highlights nothing when the preview belongs to another root", async () => {
		listings.set("/repo|.", [file("page.html")]);
		mdTabsStore.addHtmlPreview("/other-repo", "page.html");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("page.html")).not.toBeNull());

		expect(container.querySelector(".entryActive")).toBeNull();
	});
});

describe("FileBrowserPanel delete errors", () => {
	it("shows the backend error after confirmed Delete — catches log-only deletion failures", async () => {
		const { ToastContainer } = await import("../../../components/ToastContainer/ToastContainer");
		const { toastsStore } = await import("../../../stores/toasts");
		const { activityStore } = await import("../../../stores/activityStore");
		await activityStore.hydrate();
		const { appLogger } = await import("../../../stores/appLogger");
		const logged = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		listings.set("/repo|.", [file("locked.txt")]);
		mockInvoke.mockImplementation((cmd, args) =>
			cmd === "delete_path" ? Promise.reject("Permission denied: locked.txt") : defaultInvoke(cmd, args),
		);
		const { container, getByText, queryByText } = render(() => (
			<>
				<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
				<ToastContainer />
			</>
		));
		try {
			await waitFor(() => expect(queryByText("locked.txt")).not.toBeNull());
			fireEvent.contextMenu(container.querySelector(".entryName")!.parentElement!);
			fireEvent.click(getByText("Delete"));
			const confirm = Array.from(document.querySelectorAll("button")).find((b) => b.textContent?.trim() === "Delete");
			fireEvent.click(confirm!);
			await waitFor(() => expect(queryByText("Permission denied: locked.txt")).not.toBeNull());
			expect(queryByText("Delete failed")).not.toBeNull();
			expect(queryByText("locked.txt")).not.toBeNull();
			expect(mockInvoke).toHaveBeenCalledWith("delete_path", { repoPath: "/repo", path: "locked.txt" });
			expect(logged).toHaveBeenCalledWith("app", "Failed to delete", "Permission denied: locked.txt");
		} finally {
			for (const toast of toastsStore.toasts) toastsStore.remove(toast.id);
			logged.mockRestore();
		}
	});
});

describe("FileBrowserPanel create file", () => {
	/** Right-click a row and pick a context-menu item by label. */
	const openContextMenuOn = (container: HTMLElement, name: string) => {
		const label = Array.from(container.querySelectorAll(".entryName")).find((el) => el.textContent === name);
		if (!label) throw new Error(`row not found: ${name}`);
		fireEvent.contextMenu(label.parentElement as HTMLElement);
	};

	const clickMenuItem = (label: string) => {
		const item = Array.from(document.querySelectorAll("*")).find(
			(el) => el.children.length === 0 && el.textContent?.trim() === label,
		);
		if (!item) throw new Error(`menu item not found: ${label}`);
		fireEvent.click(item);
	};

	it("creates a dotfile from the tree view too", async () => {
		listings.set("/repo|.", [dir("src"), file("README.md")]);
		listings.set("/repo|src", [file("main.ts", "src/main.ts")]);
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		clickRow(container, "src");
		await waitFor(() => expect(queryByText("main.ts")).not.toBeNull());

		openContextMenuOn(container, "main.ts");
		clickMenuItem("New File\u2026");
		const input = document.querySelector("input[placeholder='File name']") as HTMLInputElement;
		expect(input).not.toBeNull();
		fireEvent.input(input, { target: { value: ".env" } });
		const confirm = Array.from(document.querySelectorAll("button")).find((b) => b.textContent?.trim() === "Create");
		fireEvent.click(confirm as HTMLButtonElement);

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", { repoPath: "/repo", file: "src/.env", content: "" }),
		);
		// And the new row must actually appear — creating it invisibly is the bug.
		listings.set("/repo|src", [file("main.ts", "src/main.ts"), file(".env", "src/.env")]);
		await waitFor(() => expect(queryByText(".env")).not.toBeNull(), { timeout: 2000 });
	});

	it("opens the root menu on the empty area below the tree entries and creates at the root", async () => {
		listings.set("/repo|.", [dir("src"), file("README.md")]);
		listings.set("/repo|src", [file("main.ts", "src/main.ts")]);
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		clickRow(container, "src");
		await waitFor(() => expect(queryByText("main.ts")).not.toBeNull());

		const empty = container.querySelector(".content") as HTMLElement;
		const notPrevented = fireEvent.contextMenu(empty);
		expect(notPrevented).toBe(false);
		clickMenuItem("New Folder\u2026");
		const input = document.querySelector("input[placeholder='Folder name']") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "docs" } });
		const confirm = Array.from(document.querySelectorAll("button")).find((b) => b.textContent?.trim() === "Create");
		fireEvent.click(confirm as HTMLButtonElement);

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("create_directory", { repoPath: "/repo", dir: "docs" }),
		);
	});

	it("empty-area menu in flat view inside a subdirectory creates in that listed directory", async () => {
		listings.set("/repo|.", [dir("src"), file("README.md")]);
		listings.set("/repo|src", [file("main.ts", "src/main.ts")]);
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		clickRow(container, "src");
		await waitFor(() => expect(queryByText("main.ts")).not.toBeNull());

		fireEvent.contextMenu(container.querySelector(".content") as HTMLElement);
		clickMenuItem("New File\u2026");
		const input = document.querySelector("input[placeholder='File name']") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "notes.txt" } });
		const confirm = Array.from(document.querySelectorAll("button")).find((b) => b.textContent?.trim() === "Create");
		fireEvent.click(confirm as HTMLButtonElement);

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", { repoPath: "/repo", file: "src/notes.txt", content: "" }),
		);
	});

	it("creates a dotfile typed into the New File dialog", async () => {
		listings.set("/repo|.", [file("README.md")]);
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("README.md")).not.toBeNull());

		openContextMenuOn(container, "README.md");
		clickMenuItem("New File…");

		const input = document.querySelector("input[placeholder='File name']") as HTMLInputElement;
		expect(input).not.toBeNull();
		fireEvent.input(input, { target: { value: ".env" } });

		const confirm = Array.from(document.querySelectorAll("button")).find((b) => b.textContent?.trim() === "Create");
		expect(confirm).toBeDefined();
		expect(confirm?.disabled).toBe(false);
		fireEvent.click(confirm as HTMLButtonElement);

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", { repoPath: "/repo", file: ".env", content: "" }),
		);
	});
});

describe("FileBrowserPanel external root (#1207-efd4)", () => {
	afterEach(() => uiStore.setFileBrowserExternalRoot(null));

	it("returns to the active repository when the repository changes", async () => {
		listings.set("/repoA|.", [file("a.txt")]);
		listings.set("/outside|.", [file("outside.txt")]);
		listings.set("/repoB|.", [file("b.txt")]);

		const [repoPath, setRepoPath] = createSignal("/repoA");
		const { queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath={repoPath()} onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("a.txt")).not.toBeNull());

		uiStore.setFileBrowserExternalRoot("/outside");
		await waitFor(() => expect(queryByText("outside.txt")).not.toBeNull());

		setRepoPath("/repoB");
		await waitFor(() => expect(queryByText("b.txt")).not.toBeNull());
		expect(uiStore.state.fileBrowserExternalRoot).toBeNull();
		expect(queryByText("outside.txt")).toBeNull();
	});

	it("returns to the active worktree when only the worktree changes", async () => {
		listings.set("/repo/wt1|.", [file("one.txt")]);
		listings.set("/outside|.", [file("outside.txt")]);
		listings.set("/repo/wt2|.", [file("two.txt")]);

		const [fsRoot, setFsRoot] = createSignal("/repo/wt1");
		const { queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" fsRoot={fsRoot()} onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("one.txt")).not.toBeNull());
		uiStore.setFileBrowserExternalRoot("/outside");
		await waitFor(() => expect(queryByText("outside.txt")).not.toBeNull());

		setFsRoot("/repo/wt2");
		await waitFor(() => expect(queryByText("two.txt")).not.toBeNull());
		expect(uiStore.state.fileBrowserExternalRoot).toBeNull();
	});

	it("shows the external path with a control that returns to the repository", async () => {
		listings.set("/repo|.", [file("in-repo.txt")]);
		listings.set("/outside|.", [file("outside.txt")]);
		const { queryByText, getByTitle } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("in-repo.txt")).not.toBeNull());
		expect(document.querySelector('[data-testid="external-root-banner"]')).toBeNull();

		uiStore.setFileBrowserExternalRoot("/outside");
		await waitFor(() => expect(queryByText("outside.txt")).not.toBeNull());
		expect(document.querySelector('[data-testid="external-root-banner"]')?.textContent).toContain("/outside");

		fireEvent.click(getByTitle("Back to the active repository"));
		await waitFor(() => expect(queryByText("in-repo.txt")).not.toBeNull());
		expect(uiStore.state.fileBrowserExternalRoot).toBeNull();
	});

	it("keeps the external folder while the repository stays the same", async () => {
		listings.set("/repo|.", [file("in-repo.txt")]);
		listings.set("/outside|.", [file("outside.txt")]);
		const { queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("in-repo.txt")).not.toBeNull());
		uiStore.setFileBrowserExternalRoot("/outside");
		await waitFor(() => expect(queryByText("outside.txt")).not.toBeNull());
		expect(uiStore.state.fileBrowserExternalRoot).toBe("/outside");
	});
});

describe("FileBrowserPanel pointer drag (#1329-a31a)", () => {
	const pointer = (type: string, pointerType: string, x: number, y: number) =>
		new PointerEvent(type, {
			bubbles: true,
			cancelable: true,
			button: 0,
			pointerId: 1,
			pointerType,
			clientX: x,
			clientY: y,
		});

	const renameCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "rename_path");

	/** Press on `file`, optionally hold, move past the drag threshold and release over the `dir` row. */
	const dragFileOntoDir = async (pointerType: string, holdMs = 0) => {
		listings.set("/repo|.", [dir("docs"), file("a.txt")]);
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));
		await waitFor(() => expect(queryByText("a.txt")).not.toBeNull());
		const rows = Array.from(container.querySelectorAll(".entry")) as HTMLElement[];
		const fileRow = rows.find((r) => r.textContent?.includes("a.txt")) as HTMLElement;
		const dirRow = rows.find((r) => r.dataset.dropTarget === "folder") as HTMLElement;
		const original = document.elementFromPoint;
		document.elementFromPoint = () => dirRow;
		try {
			fileRow.dispatchEvent(pointer("pointerdown", pointerType, 50, 50));
			if (holdMs) await new Promise((resolve) => setTimeout(resolve, holdMs));
			document.dispatchEvent(pointer("pointermove", pointerType, 50, 20));
			document.dispatchEvent(pointer("pointerup", pointerType, 50, 10));
			await new Promise((resolve) => setTimeout(resolve, 20));
		} finally {
			document.elementFromPoint = original;
		}
	};

	// Catches: a swipe that outruns pointercancel dropping the file onto a folder
	// (an irreversible move started by a scroll gesture).
	it.each(["touch", "pen"])("never moves a file when a %s swipe ends over a folder", async (pointerType) => {
		vi.stubGlobal("matchMedia", () => ({ matches: false }));
		await dragFileOntoDir(pointerType);
		expect(renameCalls()).toHaveLength(0);
	});

	// Catches: removing touch file move instead of arming it with a long press.
	it("moves a file when a touch long press is dragged onto a folder", async () => {
		await dragFileOntoDir("touch", 450);
		expect(renameCalls()).toHaveLength(1);
		expect(renameCalls()[0][1]).toMatchObject({ from: "a.txt", to: "docs/a.txt" });
	});

	// Guards the mouse path: drag and drop must keep moving files.
	it("still moves a file when a mouse drag ends over a folder", async () => {
		await dragFileOntoDir("mouse");
		expect(renameCalls()).toHaveLength(1);
		expect(renameCalls()[0][1]).toMatchObject({ from: "a.txt", to: "docs/a.txt" });
	});
});

describe("FileBrowserPanel symlink labels", () => {
	it.each(["flat", "tree"] as const)("%s: catches links shown as ordinary file bytes", async (mode) => {
		uiStore.setFileBrowserViewMode(mode);
		listings.set("/repo|.", [
			{
				...file("linked-dir"),
				size: 50,
				symlink_target: "/shared/documents",
				symlink_target_kind: "directory",
			},
		]);
		const open = vi.fn();
		const view = render(() => <FileBrowserPanel visible repoPath="/repo" onClose={() => {}} onFileOpen={open} />);
		await waitFor(() => expect(view.getByText("directory link → /shared/documents")).toBeTruthy());
		expect(view.queryByText("50 B")).toBeNull();
		expect(
			view.getByTitle("linked-dir\ndirectory link → /shared/documents", { normalizer: (value) => value }),
		).toBeTruthy();
		fireEvent.click(
			view.getByTitle("linked-dir\ndirectory link → /shared/documents", { normalizer: (value) => value }),
		);
		expect(open).toHaveBeenCalledWith("/repo", "linked-dir");
		// Catches refresh dedup hiding a retargeted link with unchanged timestamp.
		listings.set("/repo|.", [{ ...listings.get("/repo|.")![0], symlink_target: "/shared/updated" }]);
		repositoriesStore.bumpRevision("/repo");
		await waitFor(() => expect(view.getByText("directory link → /shared/updated")).toBeTruthy());
		view.unmount();
	});
});
