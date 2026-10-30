// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { aiChatTabs } from "../../stores/aiChatTabs";
import { rpc } from "../../transport";
import { FilesScreen } from "../screens/FilesScreen";

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const files = new Map([
	["src/hello.txt", "hello\n"],
	["src/guide.md", "# Guide\n\n**Important** note.\n"],
	["src/null.dat", "abc\0def"],
	["src/abs.md", "![In](/repo-one/src/images/in.png) ![Out](/elsewhere/out.png)"],
	[
		"src/deep/guide.md",
		"![Local](images/diagram.png) ![Web](https://example.com/logo.png) ![Inline](data:image/png;base64,AAAA)",
	],
]);
const calls: string[] = [];
const linkedTarget = vi.hoisted(() => ({
	value: { absolute_path: "/repo-one/src/deep/guide.md", is_directory: false },
}));
let failSave = false;
let delayedSearch: Promise<Array<{ name: string; path: string; is_dir: boolean; size: number }>> | null = null;

vi.mock("../../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(async (command: string, args?: Record<string, string>) => {
		calls.push(command);
		if (command === "load_repositories") return { repos: { "/repo-one": {}, "/repo-two": {} } };
		if (command === "list_directory") {
			if (args?.subdir === "src")
				return [
					{ name: "hello.txt", path: "src/hello.txt", is_dir: false, size: 6 },
					{ name: "guide.md", path: "src/guide.md", is_dir: false, size: 29 },
					{ name: "huge.txt", path: "src/huge.txt", is_dir: false, size: 1_048_577 },
					{ name: "image.bin", path: "src/image.bin", is_dir: false, size: 32 },
					{ name: "null.dat", path: "src/null.dat", is_dir: false, size: 7 },
				];
			return [
				{ name: ".claude", path: ".claude", is_dir: true, size: 0 },
				{ name: "src", path: "src", is_dir: true, size: 0 },
				{ name: ".mdkb", path: ".mdkb", is_dir: true, size: 0 },
				{ name: "docs", path: "docs", is_dir: true, size: 0 },
			];
		}
		if (command === "search_files") {
			if (delayedSearch) return delayedSearch;
			return args?.query === "config"
				? [{ name: "config.ts", path: "src/deep/config.ts", is_dir: false, size: 12 }]
				: [];
		}
		if (command === "stat_path") return { exists: true, is_dir: false, size: 110 };
		if (command === "resolve_terminal_path") return linkedTarget.value;
		if (command === "fs_read_file") {
			if (args?.file === "src/image.bin") throw new Error("Failed to read file: stream did not contain valid UTF-8");
			return files.get(args?.file ?? "") ?? "";
		}
		if (command === "write_file_if_unchanged") {
			if (failSave) throw new Error("disk full");
			if (files.get(args?.file ?? "") !== args?.expected) return false;
			files.set(args?.file ?? "", args?.content ?? "");
			return true;
		}
		throw new Error(`Unexpected command: ${command}`);
	}),
}));

const openRepo = async (getByRole: ReturnType<typeof render>["getByRole"]) => {
	await fireEvent.click(getByRole("button", { name: /repo-one/ }));
	await fireEvent.click(await waitFor(() => getByRole("button", { name: /src/ })));
};

afterEach(() => {
	cleanup();
	aiChatDraft.reset();
	localStorage.removeItem("tuic-ai-chat-tabs");
	aiChatTabs.resetMemory();
	window.getSelection()?.removeAllRanges();
	files.set("src/hello.txt", "hello\n");
	files.set("src/guide.md", "# Guide\n\n**Important** note.\n");
	calls.length = 0;
	failSave = false;
	delayedSearch = null;
	linkedTarget.value = { absolute_path: "/repo-one/src/deep/guide.md", is_directory: false };
});

describe("FilesScreen", () => {
	// Catches discarding saved desktop group order, display names, or the second path line.
	it("renders desktop group and repository order with names and short paths", async () => {
		vi.mocked(rpc).mockResolvedValueOnce({
			repos: {
				"/Gits/personal/a": { displayName: "Alpha" },
				"/Gits/work/b": { displayName: "Beta" },
				"/Gits/work/c": { displayName: "Gamma" },
				"/Gits/personal/parked": { parked: true },
				"C:\\Gits\\other\\d": { displayName: "Delta" },
			},
			groupOrder: ["work", "personal"],
			groups: {
				personal: { name: "Personal", repoOrder: ["/Gits/personal/a", "/Gits/personal/parked"] },
				work: { name: "Work", repoOrder: ["/Gits/work/c", "/Gits/work/b", "/missing"] },
			},
			repoOrder: ["/Gits/personal/a", "C:\\Gits\\other\\d", "/Gits/work/b"],
		});
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByText("Gamma")).toBeTruthy());
		expect(Array.from(view.container.querySelectorAll("h2"), (node) => node.textContent)).toEqual([
			"Work2",
			"Personal1",
		]);
		const rows = view.getAllByRole("button");
		expect(rows.map((row) => row.textContent)).toEqual([
			"Gammawork/c",
			"Betawork/b",
			"Alphapersonal/a",
			"Deltaother/d",
		]);
		expect(rows.every((row) => row.querySelectorAll("svg").length === 2)).toBe(true);
		expect(rows[0].title).toBe("/Gits/work/c");
		fireEvent.click(rows[0]);
		await waitFor(() =>
			expect(view.getByRole("navigation", { name: "File location" }).textContent).toBe("Files / Gamma"),
		);
		expect(view.getByRole("button", { name: "src" }).querySelectorAll("svg")).toHaveLength(2);
	});

	it("retries a repository opened from the picker when its first directory read failed", async () => {
		const [revision, setRevision] = createSignal(0);
		const view = render(() => <FilesScreen recoveryRevision={revision()} />);
		const repository = await waitFor(() => view.getByRole("button", { name: /repo-one/ }));
		vi.mocked(rpc).mockRejectedValueOnce(new Error("Failed to fetch"));
		await fireEvent.click(repository);
		await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Could not open directory"));
		setRevision(1);
		await waitFor(() => expect(view.getByRole("searchbox", { name: "Search files" })).toBeTruthy());
		expect(view.queryByRole("alert")).toBeNull();
	});

	it("does not discard a linked file draft on a second recovery after initialization failed", async () => {
		const [revision, setRevision] = createSignal(0);
		vi.mocked(rpc).mockRejectedValueOnce(new Error("Failed to fetch"));
		const view = render(() => (
			<FilesScreen
				recoveryRevision={revision()}
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "src/deep/guide.md" }}
			/>
		));
		await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Could not load repositories"));
		setRevision(1);
		await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "Edit" })));
		await fireEvent.input(view.getByRole("textbox", { name: "File content" }), {
			target: { value: "My unsaved review" },
		});
		setRevision(2);
		// Drain the successful retry's promise chain before checking the retained editor.
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(view.getByRole("textbox", { name: "File content" })).toHaveValue("My unsaved review");
	});

	it("opens a backend-resolved chat directory without treating it as a file", async () => {
		linkedTarget.value = { absolute_path: "/home/boss/Gits/project/docs", is_directory: true };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/home/boss/Gits", worktreePath: "/home/boss/Gits" }}
				initialLink={{ candidate: "project/docs/" }}
			/>
		));
		await waitFor(() => expect(view.getByRole("searchbox", { name: "Search files" })).toBeTruthy());
		expect(calls).toContain("resolve_terminal_path");
		expect(calls).toContain("list_directory");
		expect(calls).not.toContain("stat_path");
	});
	it("refuses a resolved directory outside the allowed workspace", async () => {
		linkedTarget.value = { absolute_path: "/secret/private", is_directory: true };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "/secret/private" }}
			/>
		));
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/outside.*registered repository/i));
		expect(calls).not.toContain("list_directory");
	});

	it("shows the complete repository path on a long press", async () => {
		const view = render(() => <FilesScreen />);
		const repository = await waitFor(() => view.getByRole("button", { name: /repo-one/ }));
		await fireEvent.touchStart(repository);
		await waitFor(
			() => expect(view.getByRole("dialog", { name: "Repository path" }).textContent).toContain("/repo-one"),
			{
				timeout: 900,
			},
		);
		await fireEvent.touchEnd(repository);
		await fireEvent.click(repository);
		expect(calls).not.toContain("list_directory");
	});

	it("keeps file actions in the title bar in both viewing and editing", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByRole("button", { name: "Edit" })).toBeTruthy());
		expect(view.getByRole("button", { name: "Edit" }).closest("header")).toBe(
			view.getByRole("button", { name: "Back" }).closest("header"),
		);
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		expect(view.getByRole("button", { name: "Cancel" }).closest("header")).toBeTruthy();
		expect(view.getByRole("button", { name: "Save" }).closest("header")).toBeTruthy();
	});
	it("places normal folders before hidden folders in the tree", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		await waitFor(() => expect(view.getByRole("button", { name: /docs/ })).toBeTruthy());
		const order = Array.from(view.container.querySelectorAll("button"))
			.map((button) => button.textContent?.trim())
			.filter(Boolean);
		expect(order.indexOf("src")).toBeLessThan(order.indexOf(".claude"));
		expect(order.indexOf("docs")).toBeLessThan(order.indexOf(".mdkb"));
	});

	it("finds a file in a nested folder from the repository tree", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		await fireEvent.input(view.getByRole("searchbox", { name: "Search files" }), {
			target: { value: "config" },
		});
		await waitFor(() => expect(view.getByRole("button", { name: /src\/deep\/config.ts/ })).toBeTruthy());
		expect(view.queryByRole("button", { name: /\.claude/ })).toBeNull();
	});

	it("restores the tree when a cleared search returns late", async () => {
		let finishSearch!: (results: Array<{ name: string; path: string; is_dir: boolean; size: number }>) => void;
		delayedSearch = new Promise((resolve) => {
			finishSearch = resolve;
		});
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		const search = view.getByRole("searchbox", { name: "Search files" });
		await fireEvent.input(search, { target: { value: "config" } });
		await fireEvent.input(search, { target: { value: "" } });
		finishSearch([{ name: "config.ts", path: "src/deep/config.ts", is_dir: false, size: 12 }]);
		await waitFor(() => expect(view.getByRole("button", { name: /src/ })).toBeTruthy());
		expect(view.queryByRole("button", { name: /src\/deep\/config.ts/ })).toBeNull();
	});
	it("lists configured repositories and navigates into and out of a directory", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		expect(view.getByRole("button", { name: /hello.txt/ })).toBeTruthy();
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		expect(view.getByRole("button", { name: /src/ })).toBeTruthy();
	});

	it("opens read-only text and saves only after explicit edit", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByText("hello")).toBeTruthy());
		expect(view.container.querySelector("pre")?.textContent).toBe("hello\n");
		expect(view.container.querySelector("#markdown-content")).toBeNull();
		expect(view.queryByRole("textbox")).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "changed\n" } });
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(files.get("src/hello.txt")).toBe("changed\n"));
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("renders a Markdown file in View, edits source, and returns to rendered View after Save", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /guide.md/ }));
		await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Guide"));
		expect(view.container.querySelector("#markdown-content strong")?.textContent).toBe("Important");
		expect(view.queryByRole("textbox")).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe("# Guide\n\n**Important** note.\n");
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "## Revised\n\nSaved from mobile.\n" } });
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(view.container.querySelector("#markdown-content h2")?.textContent).toBe("Revised"));
		expect(files.get("src/guide.md")).toBe("## Revised\n\nSaved from mobile.\n");
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("serves nested Markdown images from the file directory while retaining absolute image sources", async () => {
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "src/deep/guide.md" }}
			/>
		));
		await waitFor(() => expect(view.container.querySelectorAll("#markdown-content img").length).toBe(3));
		const images = view.container.querySelectorAll<HTMLImageElement>("#markdown-content img");
		const local = new URL(images[0].src);
		expect(local.pathname).toBe("/fs/markdown-image");
		expect(local.searchParams.get("repoPath")).toBe("/repo-one");
		expect(local.searchParams.get("file")).toBe("src/deep/images/diagram.png");
		expect(images[1].getAttribute("src")).toBe("https://example.com/logo.png");
		expect(images[2].getAttribute("src")).toBe("data:image/png;base64,AAAA");
	});

	// Catches: an absolute image path being joined to the file directory ("src//repo-one/..."), a 404.
	it("maps an absolute image inside the repository to its repo-relative path and leaves one outside alone", async () => {
		linkedTarget.value = { absolute_path: "/repo-one/src/abs.md", is_directory: false };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "src/abs.md" }}
			/>
		));
		await waitFor(() => expect(view.container.querySelectorAll("#markdown-content img").length).toBe(1));
		const url = new URL(view.container.querySelector<HTMLImageElement>("#markdown-content img")!.src);
		expect(url.pathname).toBe("/fs/markdown-image");
		expect(url.searchParams.get("file")).toBe("src/images/in.png");
		// No route serves a path outside the repository: a visible placeholder replaces the broken img.
		expect(view.getByRole("img", { name: "Image unavailable" })).toBeTruthy();
	});

	it("refuses large and binary files without offering an editor", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /huge.txt/ }));
		expect(view.getByText(/too large/i)).toBeTruthy();
		expect(calls).not.toContain("fs_read_file");
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		await fireEvent.click(view.getByRole("button", { name: /image.bin/ }));
		await waitFor(() => expect(view.getByText(/binary or non-text/i)).toBeTruthy());
		expect(view.queryByRole("button", { name: "Edit" })).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		await fireEvent.click(view.getByRole("button", { name: /null.dat/ }));
		await waitFor(() => expect(view.getByText(/binary or non-text/i)).toBeTruthy());
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("keeps the draft when saving fails and restores the original text on cancel", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByText("hello")).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "unsaved" } });
		failSave = true;
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(view.getByText(/could not save/i)).toBeTruthy());
		expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe("unsaved");
		expect(files.get("src/hello.txt")).toBe("hello\n");
		await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
		expect(view.getByText("hello")).toBeTruthy();
	});

	describe("Markdown review", () => {
		// Catches: mobile selection handles not revealing the action, or routing text to a stale session draft.
		it("stages a selected table cell for the open chat and navigates there without writing the file", async () => {
			// Catches JSX prop conditionals allocating unowned memos during selection events.
			const warn = vi.spyOn(console, "warn");
			onTestFinished(() => warn.mockRestore());
			files.set("src/guide.md", "# Guide\n\n| Name |\n| --- |\n| **Important** |\n");
			aiChatTabs.add("global", "mobile-reference");
			aiChatDraft.activate("another-chat");
			aiChatDraft.set("Unrelated question");
			const openChat = vi.fn();
			const view = render(() => <FilesScreen onOpenAiChat={openChat} />);
			await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
			await openRepo(view.getByRole);
			await fireEvent.click(view.getByRole("button", { name: /guide.md/ }));
			await waitFor(() => expect(view.container.querySelector("td strong")).not.toBeNull());
			const cell = view.container.querySelector("td strong")!;
			const range = document.createRange();
			range.selectNodeContents(cell);
			const rect = { left: 10, right: 80, top: 100, bottom: 120, height: 20 } as DOMRect;
			range.getClientRects = () => [rect] as unknown as DOMRectList;
			range.getBoundingClientRect = () => rect;
			window.getSelection()!.removeAllRanges();
			window.getSelection()!.addRange(range);
			fireEvent(document, new Event("selectionchange"));
			const action = document.body.querySelector<HTMLButtonElement>('[aria-label="Send to AI Chat"]')!;
			expect(action).not.toBeNull();
			fireEvent.click(action);
			expect(aiChatDraft.text()).toBe("/repo-one/src/guide.md:5-5\n\n> Important");
			expect(openChat).toHaveBeenCalledOnce();
			expect(calls).not.toContain("write_file_if_unchanged");
			aiChatDraft.activate("another-chat");
			expect(aiChatDraft.text()).toBe("Unrelated question");
			expect(warn).not.toHaveBeenCalled();
		});

		const openGuide = async (text: string) => {
			files.set("src/guide.md", text);
			const view = render(() => <FilesScreen />);
			await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
			await openRepo(view.getByRole);
			await fireEvent.click(view.getByRole("button", { name: /guide.md/ }));
			await waitFor(() => expect(view.container.querySelector("#markdown-content")).toBeTruthy());
			return view;
		};
		const boxes = (view: ReturnType<typeof render>) =>
			view.container.querySelectorAll<HTMLInputElement>('#markdown-content input[type="checkbox"]');

		it("writes only the tapped task line to disk", async () => {
			// Catches a toggle applied to the wrong line (or the whole file rewritten) when several boxes are open.
			const view = await openGuide("- [ ] first\n- [ ] second\n- [x] third\n");
			await fireEvent.click(boxes(view)[1]);
			await waitFor(() => expect(files.get("src/guide.md")).toBe("- [ ] first\n- [x] second\n- [x] third\n"));
			await waitFor(() => expect(boxes(view)[1].checked).toBe(true));
		});

		it("toggles a checkbox from a tap in its enlarged 44px target, not only on the 13px box", async () => {
			// Catches a tap just beside the box being ignored, which is unusable with a thumb.
			const view = await openGuide("- [ ] first\n- [ ] second\n");
			const box = boxes(view)[0];
			box.getBoundingClientRect = () => new DOMRect(20, 100, 13, 13);
			const item = box.closest("li") as HTMLElement;
			await fireEvent.click(item, { clientX: 8, clientY: 118 });
			await waitFor(() => expect(files.get("src/guide.md")).toBe("- [x] first\n- [ ] second\n"));
		});

		it("refuses to overwrite a file edited on disk meanwhile and reloads it", async () => {
			// Catches a stale view silently replacing newer text written by the desktop.
			const view = await openGuide("- [ ] first\n- [ ] second\n");
			files.set("src/guide.md", "- [ ] first\n- [ ] second\nadded on desktop\n");
			await fireEvent.click(boxes(view)[0]);
			await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/changed on disk/i));
			expect(files.get("src/guide.md")).toBe("- [ ] first\n- [ ] second\nadded on desktop\n");
			await waitFor(() =>
				expect(view.container.querySelector("#markdown-content")?.textContent).toContain("added on desktop"),
			);
		});

		it("saves a block comment in the desktop tweak format", async () => {
			// Catches a second, mobile-only comment format the desktop viewer cannot read back.
			const view = await openGuide("# Digest\n\nShip the release on Friday.\n");
			await waitFor(() => expect(view.container.querySelector("[data-comment-source-start]")).toBeTruthy());
			await fireEvent.click(view.getByText("Ship the release on Friday."));
			await fireEvent.click(view.getByRole("button", { name: "Comment" }));
			await fireEvent.input(view.getByRole("textbox", { name: "Comment text" }), {
				target: { value: "Monday instead" },
			});
			await fireEvent.click(view.getByRole("button", { name: "Save comment" }));
			await waitFor(() =>
				expect(files.get("src/guide.md")).toMatch(
					/<!--tweak:block:c_\S+ @\S+\nMonday instead-->\nShip the release on Friday\./,
				),
			);
			await waitFor(() => expect(view.container.querySelector(".tweak-block-highlight")).toBeTruthy());
		});

		it("keeps the comment draft when the file changed on disk", async () => {
			// Catches a conflict discarding the text the user just typed.
			const view = await openGuide("# Digest\n\nShip the release on Friday.\n");
			await waitFor(() => expect(view.container.querySelector("[data-comment-source-start]")).toBeTruthy());
			await fireEvent.click(view.getByText("Ship the release on Friday."));
			await fireEvent.click(view.getByRole("button", { name: "Comment" }));
			await fireEvent.input(view.getByRole("textbox", { name: "Comment text" }), {
				target: { value: "Monday instead" },
			});
			files.set("src/guide.md", "# Digest\n\nShip the release on Friday.\n\nmore\n");
			await fireEvent.click(view.getByRole("button", { name: "Save comment" }));
			await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/changed on disk/i));
			expect(files.get("src/guide.md")).not.toContain("tweak");
			await waitFor(() => expect(view.container.querySelector("#markdown-content")?.textContent).toContain("more"));
			await waitFor(() => expect(view.container.querySelector("[data-comment-source-start]")).toBeTruthy());
			await fireEvent.click(view.getByText("Ship the release on Friday."));
			await fireEvent.click(view.getByRole("button", { name: "Comment" }));
			expect((view.getByRole("textbox", { name: "Comment text" }) as HTMLTextAreaElement).value).toBe("Monday instead");
		});
	});
});
