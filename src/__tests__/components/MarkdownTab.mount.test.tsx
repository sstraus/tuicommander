// @vitest-environment jsdom
// MarkdownTab mounts ContentRenderer, which relies on DOMPurify's full NodeIterator
// support — matches ContentRenderer.test.tsx's environment override.

import { fireEvent, render as renderUntracked, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// jsdom (needed here for DOMPurify's full NodeIterator support) has no
// ResizeObserver at all — DomSearchOverview (mounted whenever the search bar
// opens) instantiates one unconditionally.
class StubResizeObserver {
	observe() {}
	unobserve() {}
	disconnect() {}
}
vi.stubGlobal("ResizeObserver", StubResizeObserver);

// This suite has no global afterEach(cleanup) (matches the rest of this codebase's
// test conventions), so a component left mounted keeps its reactive roots — and
// any in-flight async effects — alive into later tests. Track and unmount everything.
let pendingUnmounts: Array<() => void> = [];
function render(...args: Parameters<typeof renderUntracked>) {
	const result = renderUntracked(...args);
	pendingUnmounts.push(result.unmount);
	return result;
}

const { mockReadFile, mockInvoke, mockResolve, mockCopyPathToClipboard, mockOpenFileAction } = vi.hoisted(() => ({
	mockReadFile: vi.fn(),
	mockInvoke: vi.fn(),
	mockResolve: vi.fn(),
	mockCopyPathToClipboard: vi.fn(),
	mockOpenFileAction: vi.fn(),
}));

vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ readFile: mockReadFile }),
}));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("../../plugins/markdownProviderRegistry", () => ({
	markdownProviderRegistry: { resolve: mockResolve },
}));
vi.mock("../../utils/clipboard", () => ({ copyPathToClipboard: mockCopyPathToClipboard }));
vi.mock("../../utils/filePreview", () => ({ openFileAction: mockOpenFileAction }));
// CommentOverlay attaches document-level selectionchange listeners and does its own
// anchor-resolution — out of scope for MarkdownTab's own wiring tests. Stub it with
// buttons that exercise the onSave/onDelete callbacks MarkdownTab wires up.
vi.mock("../../components/MarkdownTab/CommentOverlay", () => ({
	CommentOverlay: (props: { onSave: (c: unknown, occ: number) => void; onDelete: (id: string) => void }) => (
		<div data-testid="comment-overlay">
			<button
				type="button"
				onClick={() =>
					// `highlighted` must actually appear in the rendered content — insertTweakComment
					// anchors by locating this text in the source (mount tests below render "# Hello").
					props.onSave({ id: "c1", highlighted: "Hello", comment: "hi", createdAt: "2026-01-01T00:00:00.000Z" }, 0)
				}
			>
				save-comment
			</button>
			<button type="button" onClick={() => props.onDelete("c1")}>
				delete-comment
			</button>
		</div>
	),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { toastsStore } from "../../stores/toasts";

function fileTab(overrides: Partial<Parameters<typeof MarkdownTab>[0]["tab"]> = {}) {
	return {
		id: "tab1",
		type: "file" as const,
		repoPath: "/repo",
		filePath: "docs/README.md",
		fileName: "README.md",
		fsRoot: undefined,
		fontSize: undefined,
		...overrides,
	} as import("../../stores/mdTabs").FileTab;
}

function virtualTab(overrides: Partial<Parameters<typeof MarkdownTab>[0]["tab"]> = {}) {
	return {
		id: "tab-virtual",
		type: "virtual" as const,
		title: "Plan",
		contentUri: "plan:file?path=/foo.md",
		fontSize: undefined,
		...overrides,
	} as import("../../stores/mdTabs").VirtualTab;
}

describe("MarkdownTab", () => {
	beforeEach(() => {
		mockReadFile.mockReset().mockResolvedValue("# Hello");
		mockInvoke.mockReset().mockResolvedValue(undefined);
		mockResolve.mockReset().mockResolvedValue("resolved content");
		mockCopyPathToClipboard.mockReset();
		mockOpenFileAction.mockReset();
		vi.spyOn(appLogger, "error").mockImplementation(() => {});
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		vi.spyOn(diffTabsStore, "add").mockImplementation(() => "diff-id");
		vi.spyOn(editorTabsStore, "add").mockImplementation(() => "editor-id");
		vi.spyOn(toastsStore, "add").mockImplementation(() => 1);
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		for (const unmount of pendingUnmounts) unmount();
		pendingUnmounts = [];
		mdTabsStore.clearAll();
		paneLayoutStore._testCancelPendingSave();
		vi.restoreAllMocks();
	});

	describe("file tabs — loading and content", () => {
		it("shows the loading message before content resolves", async () => {
			let resolveRead!: (v: string) => void;
			mockReadFile.mockReturnValue(new Promise((r) => (resolveRead = r)));
			const { container } = render(() => <MarkdownTab tab={fileTab()} />);

			expect(container.querySelector("#markdown-content")?.textContent).toContain("Loading");
			resolveRead("# Done");
			await waitFor(() => expect(container.querySelector("#markdown-content")?.innerHTML).toContain("Done"));
		});

		it("renders file content as parsed markdown", async () => {
			mockReadFile.mockResolvedValue("# Hello World");
			const { container } = render(() => <MarkdownTab tab={fileTab()} />);

			await waitFor(() => {
				expect(mockReadFile).toHaveBeenCalledWith("/repo", "docs/README.md");
				expect(container.querySelector("#markdown-content")?.innerHTML).toContain("<h1");
				expect(container.querySelector("#markdown-content")?.innerHTML).toContain("Hello World");
			});
		});

		it("shows 'No content' message for an empty file, without logging", async () => {
			mockReadFile.mockResolvedValue("");
			const { container } = render(() => <MarkdownTab tab={fileTab()} />);

			await waitFor(() => expect(container.querySelector("#markdown-content")?.textContent).toContain("No content"));
			expect(appLogger.error).not.toHaveBeenCalled();
		});

		it("shows an error message and logs when the read fails for a real error", async () => {
			mockReadFile.mockRejectedValue(new Error("permission denied"));
			const { container } = render(() => <MarkdownTab tab={fileTab()} />);

			await waitFor(() =>
				expect(container.querySelector("#markdown-content")?.textContent).toContain("permission denied"),
			);
			expect(appLogger.error).toHaveBeenCalledWith(
				"app",
				"readFileContent failed",
				expect.objectContaining({ error: "permission denied" }),
			);
		});

		it("shows an error message but does NOT log for a missing (deleted) file", async () => {
			mockReadFile.mockRejectedValue(new Error("No such file or directory (os error 2)"));
			const { container } = render(() => <MarkdownTab tab={fileTab()} />);

			await waitFor(() => expect(container.querySelector("#markdown-content")?.textContent).toContain("Error:"));
			expect(appLogger.error).not.toHaveBeenCalled();
		});

		it("reads absolute file paths via read_external_file, bypassing repo.readFile", async () => {
			mockInvoke.mockResolvedValue("# Absolute");
			render(() => <MarkdownTab tab={fileTab({ filePath: "/etc/motd", repoPath: "" })} />);

			await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("read_external_file", { path: "/etc/motd" }));
			expect(mockReadFile).not.toHaveBeenCalled();
		});

		it("sets empty content when the file tab has no filePath", () => {
			const { container } = render(() => <MarkdownTab tab={fileTab({ filePath: "" })} />);
			expect(container.querySelector("#markdown-content")?.textContent).toContain("No content");
			expect(mockReadFile).not.toHaveBeenCalled();
		});
	});

	describe("virtual tabs", () => {
		it("resolves content via markdownProviderRegistry", async () => {
			mockResolve.mockResolvedValue("# Virtual content");
			const { container } = render(() => <MarkdownTab tab={virtualTab()} />);

			await waitFor(() => {
				expect(mockResolve).toHaveBeenCalledWith("plan:file?path=/foo.md");
				expect(container.querySelector("#markdown-content")?.innerHTML).toContain("Virtual content");
			});
		});

		it("shows 'Content unavailable' when the provider resolves null", async () => {
			mockResolve.mockResolvedValue(null);
			const { container } = render(() => <MarkdownTab tab={virtualTab()} />);

			await waitFor(() =>
				expect(container.querySelector("#markdown-content")?.textContent).toContain("Content unavailable"),
			);
		});

		it("shows an error message when the provider throws", async () => {
			mockResolve.mockRejectedValue(new Error("provider boom"));
			const { container } = render(() => <MarkdownTab tab={virtualTab()} />);

			await waitFor(() => expect(container.querySelector("#markdown-content")?.textContent).toContain("provider boom"));
		});
	});

	describe("header actions", () => {
		it("renders the display path in the header", async () => {
			const { container } = render(() => <MarkdownTab tab={fileTab({ filePath: "docs/guide.md" })} />);
			await waitFor(() => expect(container.textContent).toContain("docs/guide.md"));
		});

		it("adds an editor tab when Edit is clicked", async () => {
			const { getByTitle } = render(() => <MarkdownTab tab={fileTab({ fsRoot: "/worktree" })} />);
			fireEvent.click(getByTitle("Edit file"));
			expect(editorTabsStore.add).toHaveBeenCalledWith("/worktree", "docs/README.md");
		});

		it("shows the Diff button only when the tab has a repoPath, and wires it to diffTabsStore", async () => {
			const { getByTitle } = render(() => <MarkdownTab tab={fileTab({ repoPath: "/repo" })} />);
			fireEvent.click(getByTitle("View diff"));
			expect(diffTabsStore.add).toHaveBeenCalledWith("/repo", "docs/README.md", "M");
		});

		it("does not render Edit/Diff buttons for a virtual tab", () => {
			const { queryByTitle } = render(() => <MarkdownTab tab={virtualTab()} />);
			expect(queryByTitle("Edit file")).toBeNull();
			expect(queryByTitle("View diff")).toBeNull();
		});
	});

	describe("Copy Path context menu", () => {
		it("opens the context menu on right-click and copies the shortened full path", async () => {
			const { container, getByText } = render(() => (
				<MarkdownTab tab={fileTab({ repoPath: "/repo", filePath: "docs/README.md" })} />
			));
			const header = container.querySelector(".header, [class*='header']")!;
			fireEvent.contextMenu(header);

			const copyItem = await waitFor(() => getByText("Copy Path"));
			fireEvent.click(copyItem);

			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/repo/docs/README.md");
		});

		it("does not open the context menu when there is no resolvable path", () => {
			const { container, queryByText } = render(() => <MarkdownTab tab={virtualTab()} />);
			const header = container.querySelector(".header, [class*='header']")!;
			fireEvent.contextMenu(header);
			expect(queryByText("Copy Path")).toBeNull();
		});
	});

	describe("markdown link clicks", () => {
		it("resolves a relative link against the file's directory and opens it", async () => {
			mockReadFile.mockResolvedValue("[link](other.md)");
			// Main resolves links in Rust (`resolve_markdown_link`); the frontend only
			// forwards the file and href and opens what comes back.
			mockInvoke.mockImplementation(async (command: string) =>
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/repo/docs/other.md",
							open_path: "docs/other.md",
							is_directory: false,
							same_document: false,
						}
					: undefined,
			);
			const { container } = render(() => (
				<MarkdownTab tab={fileTab({ filePath: "docs/README.md", repoPath: "/repo" })} />
			));

			const link = await waitFor(() => {
				const a = container.querySelector("#markdown-content a");
				expect(a).not.toBeNull();
				return a as HTMLAnchorElement;
			});
			fireEvent.click(link);

			await waitFor(() =>
				expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", {
					root: "/repo",
					currentFile: "docs/README.md",
					href: "other.md",
				}),
			);
			await waitFor(() =>
				expect(mockOpenFileAction).toHaveBeenCalledWith("docs/other.md", "/repo", undefined, undefined),
			);
		});
	});

	describe("checkbox toggling", () => {
		it("toggles a checkbox and writes the updated source back via write_file", async () => {
			mockReadFile.mockResolvedValue("- [ ] todo item");
			const { container } = render(() => (
				<MarkdownTab tab={fileTab({ repoPath: "/repo", filePath: "docs/README.md" })} />
			));

			const checkbox = await waitFor(() => {
				const cb = container.querySelector("#markdown-content input[type=checkbox]");
				expect(cb).not.toBeNull();
				return cb as HTMLInputElement;
			});
			fireEvent.click(checkbox);

			await waitFor(() =>
				expect(mockInvoke).toHaveBeenCalledWith(
					"write_file",
					expect.objectContaining({ repoPath: "/repo", file: "docs/README.md" }),
				),
			);
		});
	});

	describe("comment overlay wiring (active file tab only)", () => {
		it("mounts CommentOverlay for the active file tab and wires onSave to a write", async () => {
			const tab = fileTab({ repoPath: "/repo", filePath: "docs/README.md" });
			mdTabsStore.setActive(tab.id);
			const { container, findByTestId, getByText } = render(() => <MarkdownTab tab={tab} />);

			// CommentOverlay mounts as soon as the content container element exists —
			// independent of whether the async file read has resolved yet. The stub's
			// "save" anchors on text from the *loaded* content, so wait for that too.
			await findByTestId("comment-overlay");
			await waitFor(() => expect(container.querySelector("#markdown-content")?.textContent).toContain("Hello"));
			fireEvent.click(getByText("save-comment"));

			await waitFor(() =>
				expect(mockInvoke).toHaveBeenCalledWith(
					"write_file",
					expect.objectContaining({ repoPath: "/repo", file: "docs/README.md" }),
				),
			);
		});

		it("does not mount CommentOverlay when the tab is not active", async () => {
			const tab = fileTab({ repoPath: "/repo" });
			mdTabsStore.setActive("some-other-tab");
			const { queryByTestId } = render(() => <MarkdownTab tab={tab} />);
			await waitFor(() => expect(container_has_content()).toBe(true));
			expect(queryByTestId("comment-overlay")).toBeNull();

			function container_has_content() {
				return true;
			}
		});

		it("does not mount CommentOverlay for a virtual tab even if marked active", () => {
			const tab = virtualTab();
			mdTabsStore.setActive(tab.id);
			const { queryByTestId } = render(() => <MarkdownTab tab={tab} />);
			expect(queryByTestId("comment-overlay")).toBeNull();
		});
	});

	describe("search handle registration", () => {
		it("registers and clears a handle on mdTabsStore keyed by tab id", () => {
			const tab = fileTab();
			const { unmount } = render(() => <MarkdownTab tab={tab} />);

			expect(mdTabsStore.getHandle(tab.id)).toBeDefined();
			unmount();
			expect(mdTabsStore.getHandle(tab.id)).toBeUndefined();
		});

		it("openSearch() from the handle reveals the search bar", async () => {
			const tab = fileTab();
			const { container } = render(() => <MarkdownTab tab={tab} />);

			const handle = mdTabsStore.getHandle<{ openSearch: () => void }>(tab.id);
			expect(container.querySelector("input[placeholder], [class*='searchBar']")).toBeDefined();
			handle?.openSearch();
			await waitFor(() => {
				const input = container.querySelector("input");
				expect(input).not.toBeNull();
			});
		});
	});

	describe("repo revision reactivity", () => {
		it("re-reads file content when the repo revision bumps while the tab is active", async () => {
			const tab = fileTab({ repoPath: "/repo-revision-test", filePath: "docs/README.md" });
			mdTabsStore.setActive(tab.id);
			mockReadFile.mockResolvedValue("# v1");
			const { container } = render(() => <MarkdownTab tab={tab} />);
			await waitFor(() => expect(container.querySelector("#markdown-content")?.innerHTML).toContain("v1"));

			const callsBeforeBump = mockReadFile.mock.calls.length;
			mockReadFile.mockResolvedValue("# v2");
			repositoriesStore.bumpRevision(tab.repoPath);

			await waitFor(() => expect(mockReadFile.mock.calls.length).toBeGreaterThan(callsBeforeBump));
			await waitFor(() => expect(container.querySelector("#markdown-content")?.innerHTML).toContain("v2"));
		});
	});
});
