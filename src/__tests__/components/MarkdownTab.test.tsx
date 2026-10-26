// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockRpc, mockEmitTo } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockRpc: vi.fn(),
	mockEmitTo: vi.fn().mockResolvedValue(undefined),
}));

let fileContent = "";
let cleanupInfo: ReturnType<typeof vi.spyOn>;

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({
	emitTo: mockEmitTo,
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../hooks/initPanelWindow", () => ({ initPanelWindow: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { renderPanelMode } from "../../panelRouter";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { appLogger } from "../../stores/appLogger";
import { editorTabsStore } from "../../stores/editorTabs";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { setToastBellMirrorResolver, toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { CONVENTION_HEADER, parseTweakComments } from "../../utils/tweakComments";

describe("MarkdownTab agent review actions", () => {
	beforeEach(() => {
		cleanupInfo = vi.spyOn(appLogger, "info").mockImplementation(() => {});
		setToastBellMirrorResolver(() => false);
		mockInvoke.mockReset();
		mockRpc.mockReset();
		mockEmitTo.mockClear();
		mockInvoke.mockImplementation((command: string) => {
			if (command === "read_file" || command === "read_external_file") {
				return Promise.resolve(fileContent);
			}
			return Promise.resolve(undefined);
		});
		mockRpc.mockResolvedValue({ typed: false, queued: 1 });
		fileContent = "<!--tweak:block:c_review @2026-09-23T08:00:00.000Z\nClarify this heading-->\n# Heading";
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		window.history.replaceState({}, "", "/");
		setToastBellMirrorResolver(() => true);
		cleanup();
		for (const [source, message] of cleanupInfo.mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toMatch(/^TermStore\.remove\(term-\d+\)$/);
		}
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		vi.restoreAllMocks();
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		mdTabsStore.clearAll();
	});

	it("renders a detached Markdown document from its file path and refreshes changed disk content", async () => {
		fileContent = "# First version";
		window.history.replaceState(
			{},
			"",
			"/?mode=panel&panel=markdown-tab-md-1&tabId=md-1&filePath=%2FUsers%2Fboss%2Freport.md&fileName=report.md",
		);
		const { container } = render(() => renderPanelMode());
		await waitFor(() => expect(container.textContent).toContain("First version"));
		fileContent = "# Revised version";
		fireEvent.focus(window);
		await waitFor(() => expect(container.textContent).toContain("Revised version"));
	});

	it("rejects a detached document URL whose tab id does not match its window", () => {
		window.history.replaceState(
			{},
			"",
			"/?mode=panel&panel=markdown-tab-md-1&tabId=md-2&filePath=%2FUsers%2Fboss%2Freport.md&fileName=report.md",
		);
		const { container } = render(() => renderPanelMode());
		expect(container.textContent).toContain("Invalid Markdown document");
		expect(mockInvoke).not.toHaveBeenCalledWith("read_external_file", expect.anything());
	});

	it("opens a linked document from the detached window in the main window", async () => {
		fileContent = "[next](./next.md)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/repo/next.md",
							open_path: "next.md",
							is_directory: false,
							same_document: false,
						}
					: fileContent,
			),
		);
		window.history.replaceState(
			{},
			"",
			"/?mode=panel&panel=markdown-tab-md-1&tabId=md-1&repoPath=%2Frepo&fsRoot=%2Frepo&filePath=review.md&fileName=review.md",
		);
		const { container } = render(() => renderPanelMode());
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		fireEvent.click(link);
		await waitFor(() =>
			expect(mockEmitTo).toHaveBeenCalledWith("main", "panel-action", expect.objectContaining({ action: "open-link" })),
		);
	});

	// jsdom has no layout; CodeMirror's measure pass asks Range for client rects.
	Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
	Range.prototype.getBoundingClientRect ??= () => new DOMRect();

	it("Live mode saves the typed buffer byte for byte through write_file and leaves the viewer alone", async () => {
		// catches: Live save going through a serializer, or replacing the read-only viewer
		fileContent =
			"# Title\r\n\r\ntext **bold** <!--tweak:begin:c1-->w<!--tweak:end:c1 @2026-01-01T00:00:00.000Z\nn-->\r\n";
		const tabId = mdTabsStore.add("/repo", "docs/live.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		await waitFor(() => expect(container.textContent).toContain("Title"));
		expect(container.querySelector(".cm-editor")).toBeNull();
		fireEvent.click(screen.getByText("Live"));
		// Live renders the preview; a click on the heading swaps it for its source editor.
		const heading = await waitFor(() => {
			const h = container.querySelector<HTMLElement>("h1[data-comment-source-start]");
			if (!h) throw new Error("live view not rendered");
			return h;
		});
		fireEvent.click(heading);
		await waitFor(() => {
			if (!container.querySelector(".cm-content")) throw new Error("block editor not mounted");
		});
		const view = (await import("@codemirror/view")).EditorView.findFromDOM(
			container.querySelector(".cm-editor") as HTMLElement,
		);
		expect(view).not.toBeNull();
		view?.dispatch({ changes: { from: 0, insert: "X" } });
		fireEvent.click(await screen.findByText("Save"));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", {
				repoPath: "/repo",
				file: "docs/live.md",
				content: `X${fileContent}`,
			}),
		);
	});

	function addAgent(name: string, sessionId: string, repoPath: string) {
		const id = terminalsStore.add({ name, sessionId, fontSize: 14, cwd: repoPath, awaitingInput: null });
		terminalsStore.update(id, { agentType: "claude", repoPath });
		return id;
	}

	// Catches: the first candidate receives a review instead of the MCP tab opener.
	it("preselects the MCP opener rather than the first review candidate and preserves a user choice", async () => {
		addAgent("First", "pty-a", "/repo");
		const opener = addAgent("Opener", "pty-b", "/repo");
		terminalsStore.update(opener, { tuicSession: "tuic-b" });
		const id = mdTabsStore.addMcpFile("review-b", "/repo", "review.md", false, false, "tuic-b");
		render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		const select = await screen.findByRole("combobox", { name: "Review agent" });
		expect(select).toHaveValue("pty-b");
		fireEvent.change(select, { target: { value: "pty-a" } });
		terminalsStore.update(opener, { name: "Renamed opener" });
		expect(select).toHaveValue("pty-a");
		fireEvent.click(screen.getByRole("button", { name: "Send changes to agent" }));
		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("enqueue_agent_command", expect.objectContaining({ sessionId: "pty-a" })),
		);
	});

	// Catches: an automatic fallback is treated as a user choice before session inventory arrives.
	it("adopts a late opener but falls back when the opener is not an eligible repository agent", async () => {
		addAgent("Fallback", "pty-a", "/repo");
		const id = mdTabsStore.addMcpFile("review-b", "/repo", "review.md", false, false, "tuic-b");
		render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		const select = await screen.findByRole("combobox", { name: "Review agent" });
		expect(select).toHaveValue("pty-a");
		const opener = addAgent("Opener", "pty-b", "/other");
		terminalsStore.update(opener, { tuicSession: "tuic-b" });
		expect(select).toHaveValue("pty-a");
		terminalsStore.update(opener, { repoPath: "/repo" });
		expect(select).toHaveValue("pty-b");
		terminalsStore.update(opener, { agentType: null });
		expect(select).toHaveValue("pty-a");
	});

	it("queues the marked-up file for the selected agent in the same repository", async () => {
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		addAgent("Other repo", "session-other", "/other");
		addAgent("Reviewer one", "session-one", "/repo");
		addAgent("Reviewer two", "session-two", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const tab = mdTabsStore.get(tabId) as FileTab;

		const { container } = render(() => <MarkdownTab tab={tab} />);
		const selector = await screen.findByRole("combobox", { name: "Review agent" });
		expect(screen.getByText("Agent")).not.toBeNull();
		expect(Array.from((selector as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
			"Reviewer one",
			"Reviewer two",
		]);

		fireEvent.change(selector, { target: { value: "session-two" } });
		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(send);
		expect((send as HTMLButtonElement).disabled).toBe(true);

		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("enqueue_agent_command", {
				sessionId: "session-two",
				text: expect.stringContaining("/repo/docs/review.md"),
			}),
		);
		await waitFor(() => expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
		expect(addToast).toHaveBeenCalledWith("Sent to agent", expect.stringContaining("queued"), "info");
		expect(container.querySelector(".header")?.contains(send)).toBe(false);
	});

	// Catches: completing an earlier send consumes a checkbox edit saved while enqueue was pending.
	it("keeps a checkbox edit made during an in-flight send available for another send", async () => {
		fileContent += "\n\n- [ ] Answer";
		addAgent("Reviewer", "session-one", "/repo");
		let finishSend!: (outcome: { typed: boolean; queued: number }) => void;
		mockRpc.mockImplementation((command: string) =>
			command === "enqueue_agent_command"
				? new Promise((resolve) => {
						finishSend = resolve;
					})
				: Promise.resolve(undefined),
		);
		mockInvoke.mockImplementation((command: string, args?: { content?: string }) => {
			if (command === "write_file" && args?.content !== undefined) fileContent = args.content;
			return Promise.resolve(command === "read_file" ? fileContent : undefined);
		});
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		fireEvent.click(await screen.findByRole("button", { name: "Send changes to agent" }));
		await waitFor(() => expect(finishSend).toBeTypeOf("function"));
		fireEvent.click(container.querySelector<HTMLInputElement>('input[type="checkbox"]')!);
		await waitFor(() => expect(fileContent).toContain("- [x] Answer"));
		finishSend({ typed: true, queued: 0 });
		await waitFor(() => expect(toastsStore.toasts.some((toast) => toast.title === "Sent to agent")).toBe(true));
		expect(screen.queryByRole("button", { name: "Send changes to agent" })).not.toBeNull();
	});

	// Catches: retained tweak markers leave Agent/Send rendered after delivery.
	it("MarkdownTab send hides controls", async () => {
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		fireEvent.click(await screen.findByRole("button", { name: "Send changes to agent" }));
		await waitFor(() => expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
		expect(screen.queryByRole("combobox", { name: "Review agent" })).toBeNull();
		repositoriesStore.bumpRevision("/repo");
		await screen.findByRole("heading", { name: "Heading" });
		expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull();
	});

	// Catches: remembering sent markers permanently hides controls for a new tweak or edit.
	it.each(["tweak", "checkbox", "text"])("MarkdownTab new change after send: %s", async (change) => {
		fileContent += "\n\n- [ ] Answer\n";
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		fireEvent.click(await screen.findByRole("button", { name: "Send changes to agent" }));
		await waitFor(() => expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
		if (change === "tweak") {
			fileContent += "\n<!--tweak:block:c_new @2026-10-14T08:00:00.000Z\nNew feedback-->\nNew paragraph\n";
			repositoriesStore.bumpRevision("/repo");
		} else if (change === "checkbox") {
			fireEvent.click(container.querySelector<HTMLInputElement>('input[type="checkbox"]')!);
		} else {
			fireEvent.click(screen.getByText("Live"));
			const heading = await waitFor(() => {
				const element = container.querySelector<HTMLElement>("h1[data-comment-source-start]");
				if (!element) throw new Error("Live heading not rendered");
				return element;
			});
			fireEvent.click(heading);
			await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
			const view = (await import("@codemirror/view")).EditorView.findFromDOM(
				container.querySelector(".cm-editor") as HTMLElement,
			);
			view?.dispatch({ changes: { from: 0, insert: "New " } });
			fireEvent.click(await screen.findByText("Save"));
		}
		await screen.findByRole("button", { name: "Send changes to agent" });
		expect(screen.getByRole("combobox", { name: "Review agent" })).not.toBeNull();
	});

	// Catches: a rejected enqueue consumes pending changes and hides retry controls.
	it("MarkdownTab failed send keeps controls", async () => {
		const expectedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		mockRpc.mockRejectedValueOnce(new Error("Agent unavailable"));
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		fireEvent.click(await screen.findByRole("button", { name: "Send changes to agent" }));
		await waitFor(() => expect(addToast).toHaveBeenCalledWith("Couldn't send changes", "Agent unavailable", "error"));
		expect(screen.getByRole("button", { name: "Send changes to agent" }).hasAttribute("disabled")).toBe(false);
		expect(screen.getByRole("combobox", { name: "Review agent" })).not.toBeNull();
		expect(expectedError).toHaveBeenCalledExactlyOnceWith(
			"network",
			"Failed to queue Markdown review: Agent unavailable",
		);
	});

	it("marks Send ready only while it can send", async () => {
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		expect((send as HTMLButtonElement).disabled).toBe(true);
		expect(send.hasAttribute("data-ready")).toBe(false);
	});

	it("marks Send ready when an agent is selected", async () => {
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		expect(send.getAttribute("data-ready")).toBe("true");
	});

	it("queues an absolute Markdown file without prefixing its repository path", async () => {
		addAgent("Reviewer", "session-one", "/repo");
		const path = "/Users/boss/Gits/.tmp/review.md";
		const tabId = mdTabsStore.addMcpFile("external-review", "/repo", path, false, false);
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);

		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(send);

		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("enqueue_agent_command", {
				sessionId: "session-one",
				text: `Open ${path}, re-read the whole file: apply the 1 embedded tweak review comment, treat every other change since your last write (checkbox toggles, edited text) as the user's answer, remove each resolved tweak marker, and leave unrelated files unchanged.`,
			}),
		);
	});

	it("tells the agent to treat edits outside tweak comments as the user's answer, not just the tweaks", async () => {
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);

		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(send);

		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith(
				"enqueue_agent_command",
				expect.objectContaining({
					text: expect.stringMatching(/re-read the whole file.*checkbox.*user's answer/s),
				}),
			),
		);
	});

	/** Open the block-comment popover on a Markdown file's first paragraph and type a comment. */
	async function startBlockComment(name: string) {
		const tabId = name.startsWith("/")
			? mdTabsStore.addMcpFile("boss-open-questions", "/repo", name, false, false)
			: mdTabsStore.add("/repo", `docs/${name}`);
		const tab = mdTabsStore.get(tabId) as FileTab;
		const { container } = render(() => <MarkdownTab tab={tab} />);
		const paragraph = await waitFor(() => {
			const el = container.querySelector<HTMLElement>("p[data-comment-source-start]");
			if (!el) throw new Error("block metadata not applied yet");
			return el;
		});
		paragraph.getBoundingClientRect = () =>
			({ left: 100, right: 500, top: 80, bottom: 120, width: 400, height: 40, x: 100, y: 80, toJSON() {} }) as DOMRect;
		fireEvent.mouseMove(paragraph.closest("#markdown-content")!.parentElement!, { clientX: 76, clientY: 96 });
		fireEvent.mouseDown(await screen.findByRole("button", { name: "Comment on this block" }));
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Clarify" } });
	}

	it("saves a block comment before the block the popover was opened on", async () => {
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		await startBlockComment("fresh.md");

		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", {
				repoPath: "/repo",
				file: "docs/fresh.md",
				content: expect.stringMatching(/# Heading\n\n<!--tweak:block:\S+ @\S+\nClarify-->\nOriginal paragraph\.\n$/),
			}),
		);
	});

	it("saves an MCP Markdown comment to an absolute file outside the active repository", async () => {
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		const path = "/Users/stefano.straus/Gits/.tmp/boss/open-questions.md";
		await startBlockComment(path);
		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_external_file", {
				path,
				content: expect.stringContaining("Clarify"),
			}),
		);
		expect(mockInvoke).not.toHaveBeenCalledWith("write_file", expect.anything());
	});

	it("shows a failed external comment save and retains its draft for retry", async () => {
		const expectedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		const path = "/Users/stefano.straus/Gits/.tmp/boss/open-questions.md";
		let writeAttempts = 0;
		mockInvoke.mockImplementation((command: string) => {
			if (command === "read_external_file") return Promise.resolve(fileContent);
			if (command === "write_external_file") {
				writeAttempts++;
				if (writeAttempts === 1) return Promise.reject(new Error("permission denied"));
			}
			return Promise.resolve(undefined);
		});
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		await startBlockComment(path);
		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(addToast).toHaveBeenCalledWith(
				"Couldn't save Markdown file",
				expect.stringContaining("permission denied"),
				"error",
			),
		);
		expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Clarify");
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(writeAttempts).toBe(2));
		await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
		expect(expectedError).toHaveBeenCalledExactlyOnceWith("app", "writeTweakedSource: write failed", expect.any(Error));
	});

	// Catches: saving at the whole-list offset or under a neighboring task.
	it("writes a task comment below the selected bullet in the file", async () => {
		fileContent = "- first\n- [ ] chosen\n- third\n";
		const tabId = mdTabsStore.add("/repo", "docs/tasks.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const items = await waitFor(() => {
			const found = Array.from(container.querySelectorAll<HTMLElement>("li[data-comment-source-start]"));
			if (found.length !== 3) throw new Error("list-item targets not ready");
			return found;
		});
		items.forEach((item, index) => {
			const top = 20 + index * 40;
			item.getBoundingClientRect = () =>
				({
					left: 100,
					right: 500,
					top,
					bottom: top + 20,
					width: 400,
					height: 20,
					x: 100,
					y: top,
					toJSON() {},
				}) as DOMRect;
		});
		fireEvent.mouseMove(items[1].closest("#markdown-content")!.parentElement!, { clientX: 76, clientY: 65 });
		fireEvent.mouseDown(await screen.findByRole("button", { name: "Comment on this block" }));
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Clarify chosen" } });
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", {
				repoPath: "/repo",
				file: "docs/tasks.md",
				content: expect.stringMatching(
					/- first\n- \[ \] chosen\n {2}<!--tweak:item:\S+ @\S+\n {2}Clarify chosen-->\n- third\n$/,
				),
			}),
		);
	});

	it("saves consecutive question comments beside each selected item and keeps their highlights", async () => {
		fileContent = [
			"# Questions",
			"",
			...Array.from(
				{ length: 4 },
				(_, index) =>
					`${index + 1}. **Question ${index + 1}** asks for a decision.\n   **Recommendation:** option ${index + 1}.\n`,
			),
		].join("\n");
		mockInvoke.mockImplementation((command: string, args?: { content?: string }) => {
			if (command === "read_file" || command === "read_external_file") return Promise.resolve(fileContent);
			if (command === "write_file") fileContent = args?.content ?? fileContent;
			return Promise.resolve(undefined);
		});
		const tabId = mdTabsStore.add("/repo", "docs/questions.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		for (let index = 0; index < 4; index++) {
			const items = await waitFor(() => {
				const found = Array.from(container.querySelectorAll<HTMLElement>("li[data-comment-source-start]"));
				if (found.length !== 4) throw new Error(`item targets not ready after comment ${index}`);
				return found;
			});
			items.forEach((item, itemIndex) => {
				const top = 20 + itemIndex * 40;
				item.getBoundingClientRect = () =>
					({
						left: 100,
						right: 500,
						top,
						bottom: top + 20,
						width: 400,
						height: 20,
						x: 100,
						y: top,
						toJSON() {},
					}) as DOMRect;
			});
			fireEvent.mouseMove(items[index].closest("#markdown-content")!.parentElement!, {
				clientX: 76,
				clientY: 25 + index * 40,
			});
			fireEvent.mouseDown(await screen.findByRole("button", { name: "Comment on this block" }));
			fireEvent.input(document.body.querySelector("textarea")!, { target: { value: `Answer ${index + 1}` } });
			fireEvent.click(screen.getByRole("button", { name: "Save" }));
			await waitFor(() => expect(fileContent).toContain(`Answer ${index + 1}-->`));
			expect(parseTweakComments(fileContent).map((comment) => comment.comment)).toContain(`Answer ${index + 1}`);
			await waitFor(() =>
				expect(Number(container.querySelector("li")?.getAttribute("data-comment-source-start"))).toBeGreaterThan(
					CONVENTION_HEADER.length,
				),
			);
			await waitFor(() =>
				expect(container.querySelectorAll<HTMLElement>("li.tweak-block-highlight").length).toBe(index + 1),
			);
		}
		expect(fileContent.split("<!-- tweak-comments v1:")).toHaveLength(2);
		for (let index = 1; index <= 4; index++) {
			expect(fileContent).toMatch(new RegExp(`option ${index}\\.\\n   <!--tweak:item:[^\\n]+\\n   Answer ${index}-->`));
			fireEvent.click(container.querySelectorAll<HTMLElement>("li.tweak-block-highlight")[index - 1]);
			expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(`Answer ${index}`);
			fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
		}
	});

	// The block range comes from the render that was on screen when the popover
	// opened. If the file changes while the user types, saving at those offsets
	// would write the marker into unrelated text.
	it("refuses to save a block comment when the file changed after the popover opened", async () => {
		const expectedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		await startBlockComment("stale.md");

		fileContent = "# Heading\n\nAn agent inserted this.\n\nOriginal paragraph.\n";
		repositoriesStore.bumpRevision("/repo");
		await screen.findByText("An agent inserted this.");

		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(addToast).toHaveBeenCalledWith("Couldn't add comment", expect.stringContaining("changed"), "error"),
		);
		expect(mockInvoke).not.toHaveBeenCalledWith("write_file", expect.anything());
		expect(expectedError).toHaveBeenCalledExactlyOnceWith(
			"app",
			"handleTweakBlockSave failed: insertTweakBlockComment: source range no longer matches the rendered block",
		);
	});

	it("hides the agent review controls when the file has no tweak comments", async () => {
		fileContent = "# No pending review\n";
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/clean.md");
		const tab = mdTabsStore.get(tabId) as FileTab;

		render(() => <MarkdownTab tab={tab} />);
		await screen.findByRole("heading", { name: "No pending review" });

		expect(screen.queryByRole("combobox", { name: "Review agent" })).toBeNull();
		expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull();
	});

	it("shows the agent controls after a checkbox tick in a file without tweak comments, and sends without a tweak count", async () => {
		fileContent = "- [ ] Approve the plan\n";
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/answers.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const checkbox = await waitFor(() => {
			const box = container.querySelector<HTMLInputElement>('input[type="checkbox"]');
			expect(box).not.toBeNull();
			return box as HTMLInputElement;
		});
		expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull();

		fireEvent.click(checkbox);
		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		expect(screen.getByRole("combobox", { name: "Review agent" })).not.toBeNull();
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(send);

		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("enqueue_agent_command", {
				sessionId: "session-one",
				text: expect.stringMatching(
					/^Open \/repo\/docs\/answers\.md, re-read the whole file.*checkbox.*user's answer/s,
				),
			}),
		);
		const sent = mockRpc.mock.calls.find(([method]) => method === "enqueue_agent_command")?.[1].text as string;
		expect(sent).not.toMatch(/\b0 embedded|tweak review/);
		// The edit has been delivered: with no tweaks left, the controls go away until the next edit.
		await waitFor(() => expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
	});

	it.each([
		["../src/main.rs:42", "/repo/src/main.rs", "src/main.rs", 42],
		["../LICENSE", "/repo/LICENSE", "LICENSE", undefined],
		["./My%20File.tsx#L7", "/repo/docs/My File.tsx", "docs/My File.tsx", 7],
	] as const)("opens local source link %s in the editor", async (href, absolute, relative, line) => {
		fileContent = `[link](${href})`;
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: absolute,
							open_path: relative,
							is_directory: false,
							same_document: false,
							line,
						}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		const event = new MouseEvent("click", { bubbles: true, cancelable: true });
		link.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(true);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", {
				root: "/repo",
				currentFile: "docs/review.md",
				href,
			}),
		);
		expect(open).toHaveBeenCalledWith("/repo", relative, line, { fsRoot: "/repo" });
	});

	it("reveals a directory and toasts for a missing file", async () => {
		const expectedDebug = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		fileContent = "[folder](./subdir/) [missing](./gone.rs)";
		mockInvoke.mockImplementation((command: string, args: { href?: string }) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? args.href === "./gone.rs"
						? { kind: "missing", path: "./gone.rs" }
						: {
								kind: "file",
								absolute_path: "/repo/docs/subdir",
								open_path: "docs/subdir",
								is_directory: true,
								same_document: false,
							}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const links = await waitFor(() => {
			const elements = Array.from(container.querySelectorAll("a"));
			if (elements.length !== 2) throw new Error("Markdown links not rendered yet");
			return elements;
		});
		const toast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		fireEvent.click(links[0]);
		await Promise.resolve();
		expect(uiStore.state.fileBrowserExternalRoot).toBe("/repo/docs/subdir");
		expect(uiStore.state.fileBrowserPanelVisible).toBe(true);
		fireEvent.click(links[1]);
		await Promise.resolve();
		expect(toast).toHaveBeenCalledWith("File not found", "File not found: ./gone.rs", "error");
		// The File Browser visibility change persists UI prefs after a 500ms debounce.
		await new Promise((resolve) => setTimeout(resolve, 550));
		expect(expectedDebug).toHaveBeenCalledExactlyOnceWith("store", "Failed to save UI prefs", expect.any(Error));
	});

	it("opens a relative .md link in the markdown viewer when the backend sends anchor:null,line:null", async () => {
		// Real wire shape of Rust MarkdownLinkTarget::File before skip_serializing_if (story 1352).
		fileContent = "[next](./next.md)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/repo/docs/next.md",
							open_path: "docs/next.md",
							is_directory: false,
							same_document: false,
							anchor: null,
							line: null,
						}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const editor = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		const md = vi.spyOn(mdTabsStore, "add");
		fireEvent.click(link);
		await waitFor(() => expect(md).toHaveBeenCalledWith("/repo", "docs/next.md", "/repo"));
		expect(editor).not.toHaveBeenCalled();
	});

	it("opens a relative path outside the filesystem root", async () => {
		fileContent = "[escape](../../secret.rs)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/secret.rs",
							open_path: "/secret.rs",
							is_directory: false,
							same_document: false,
						}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		fireEvent.click(link);
		await waitFor(() => expect(open).toHaveBeenCalledWith("/repo", "/secret.rs", undefined, { fsRoot: "/repo" }));
	});

	it("opens a symlink target outside the filesystem root", async () => {
		fileContent = "[escape](./outside.rs)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/secret.rs",
							open_path: "/secret.rs",
							is_directory: false,
							same_document: false,
						}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		fireEvent.click(link);
		await waitFor(() => expect(open).toHaveBeenCalledWith("/repo", "/secret.rs", undefined, { fsRoot: "/repo" }));
	});

	it("scrolls a same-document heading without probing the filesystem", async () => {
		fileContent = "# Target Heading\n\n[go](#target-heading)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link" ? { kind: "heading", anchor: "target-heading" } : fileContent,
			),
		);
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const tabId = mdTabsStore.add("/repo", "docs/review.md");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: "start" }));
			expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", {
				root: "/repo",
				currentFile: "docs/review.md",
				href: "#target-heading",
			});
		} finally {
			HTMLElement.prototype.scrollIntoView = original;
		}
	});

	it("scrolls a heading in a virtual plan tab", async () => {
		const provider = markdownProviderRegistry.register("test-plan", {
			provideContent: async () => "# Target Heading\n\n[go](#target-heading)",
		});
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const id = mdTabsStore.addVirtual("Plan", "test-plan:example");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(id)!} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("virtual Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			expect(scroll).toHaveBeenCalledWith({ block: "start" });
		} finally {
			provider.dispose();
			HTMLElement.prototype.scrollIntoView = original;
		}
	});

	it("opens a linked Markdown file as a preview tab", async () => {
		fileContent = "[next](./next.md)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? {
							kind: "file",
							absolute_path: "/repo/docs/next.md",
							open_path: "docs/next.md",
							is_directory: false,
							same_document: false,
						}
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const add = vi.spyOn(mdTabsStore, "add").mockReturnValue("linked-tab");
		fireEvent.click(link);
		await waitFor(() => expect(add).toHaveBeenCalledWith("/repo", "docs/next.md", "/repo"));
	});

	it("scrolls to a heading after the linked Markdown tab loads", async () => {
		fileContent = "[next](./next.md#target-heading)";
		mockInvoke.mockImplementation((command: string, args: { file?: string }) => {
			if (command === "resolve_markdown_link")
				return Promise.resolve({
					kind: "file",
					absolute_path: "/repo/docs/next.md",
					open_path: "docs/next.md",
					is_directory: false,
					same_document: false,
					anchor: "target-heading",
				});
			return Promise.resolve(args.file === "docs/next.md" ? "# Target Heading" : fileContent);
		});
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const tabId = mdTabsStore.add("/repo", "docs/review.md");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			const next = await waitFor(() => {
				const tab = mdTabsStore.getActive();
				if (tab?.type !== "file" || tab.filePath !== "docs/next.md") throw new Error("Linked tab not open yet");
				return tab;
			});
			render(() => <MarkdownTab tab={next} />);
			await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: "start" }));
		} finally {
			HTMLElement.prototype.scrollIntoView = original;
		}
	});
});

describe("MarkdownTab preview paging keys", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(command === "read_file" || command === "read_external_file" ? "# Long document" : undefined),
		);
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
		mdTabsStore.clearAll();
	});

	// jsdom has no layout: give the scroll container a 500px viewport over 2000px of content.
	async function renderPreview() {
		const tabId = mdTabsStore.add("/repo", "docs/long.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		await waitFor(() => expect(container.textContent).toContain("Long document"));
		const wrapper = container.querySelector<HTMLElement>('[data-focus-target="md-tab"]') as HTMLElement;
		const scroller = container.querySelector<HTMLElement>("[class*='content']") as HTMLElement;
		Object.defineProperty(scroller, "clientHeight", { configurable: true, value: 500 });
		Object.defineProperty(scroller, "scrollHeight", { configurable: true, value: 2000 });
		return { wrapper, scroller };
	}

	it("PageDown on the focused wrapper scrolls the preview down by one visible page", async () => {
		const { wrapper, scroller } = await renderPreview();
		wrapper.focus();
		fireEvent.keyDown(wrapper, { key: "PageDown" });
		expect(scroller.scrollTop).toBeGreaterThan(0);
		expect(scroller.scrollTop).toBeLessThanOrEqual(500);
	});

	it("PageUp scrolls back up and Home/End jump to the ends", async () => {
		const { wrapper, scroller } = await renderPreview();
		fireEvent.keyDown(wrapper, { key: "End" });
		expect(scroller.scrollTop).toBe(1500);
		fireEvent.keyDown(wrapper, { key: "PageUp" });
		expect(scroller.scrollTop).toBeLessThan(1500);
		expect(scroller.scrollTop).toBeGreaterThan(900);
		fireEvent.keyDown(wrapper, { key: "Home" });
		expect(scroller.scrollTop).toBe(0);
	});

	it("leaves modified keys (Shift+PageDown terminal scroll) alone", async () => {
		const { wrapper, scroller } = await renderPreview();
		fireEvent.keyDown(wrapper, { key: "PageDown", shiftKey: true });
		expect(scroller.scrollTop).toBe(0);
	});

	it("does not hijack paging keys typed in a form control", async () => {
		const { wrapper, scroller } = await renderPreview();
		const input = document.createElement("input");
		wrapper.appendChild(input);
		fireEvent.keyDown(input, { key: "PageDown" });
		expect(scroller.scrollTop).toBe(0);
	});
});
