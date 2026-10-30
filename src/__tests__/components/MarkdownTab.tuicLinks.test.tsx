// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke, openFile } = vi.hoisted(() => ({ invoke: vi.fn(), openFile: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../utils/filePreview", () => ({ openFileAction: openFile }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn(),
	emitTo: vi.fn(),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { uiStore } from "../../stores/ui";

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
	mdTabsStore.clearAll();
	uiStore._testCancelPendingSave();
});

describe("local tuic links in a Markdown tab", () => {
	it.each(
		["/repo", "/other-worktree", "/unregistered"].flatMap((root) =>
			["target.md", "image.png", "index.html", "directory"].map((name) => [root, name]),
		),
	)("opens %s/%s — catches: tuic open links discarded by the rendered-link scheme guard", async (root, name) => {
		const path = `${root}/${name}`;
		const href = `tuic://open/${path}`;
		invoke.mockReset();
		openFile.mockReset();
		invoke.mockImplementation(async (command: string) =>
			command === "resolve_markdown_link"
				? {
						kind: "file",
						absolute_path: path,
						open_path: path,
						is_directory: name === "directory",
						same_document: false,
					}
				: `[target](${href})`,
		);
		const openDirectory = vi.spyOn(uiStore, "setFileBrowserExternalRoot");
		const id = mdTabsStore.add("/repo", "report.md");
		const { getByText } = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		fireEvent.click(await waitFor(() => getByText("target")));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("resolve_markdown_link", { root: "/repo", currentFile: "report.md", href }),
		);
		if (name === "directory") await waitFor(() => expect(openDirectory).toHaveBeenCalledWith(path));
		else await waitFor(() => expect(openFile).toHaveBeenCalledWith(path, "/repo", "/repo", undefined));
	});
});
