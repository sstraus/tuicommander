// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn() }));
vi.mock("../../transport", async (original) => ({ ...(await original<object>()), isTauri: () => false, rpc }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));

import { FilesScreen } from "../screens/FilesScreen";

afterEach(cleanup);
describe("mobile rendered tuic links", () => {
	it.each(
		["/repo", "/other-worktree", "/unregistered"].flatMap((root) =>
			["target.md", "image.png", "index.html", "directory"].map((name) => [root, name]),
		),
	)("opens %s/%s — catches: mobile Markdown has no local-link navigation handler", async (root, name) => {
		const path = `${root}/${name}`;
		const href = `tuic://open/${path}`;
		rpc.mockReset();
		rpc.mockImplementation(async (command: string, args?: Record<string, string>) => {
			if (command === "load_repositories") return { repos: { "/repo": {}, "/other-worktree": {} } };
			if (command === "list_directory") return [{ name: "report.md", path: "report.md", is_dir: false, size: 100 }];
			if (command === "fs_read_file" || command === "read_file")
				return args?.file === "report.md"
					? `[target](${href})`
					: args?.file === "index.html"
						? "<h1>Opened HTML target</h1>"
						: "# Opened Markdown target";
			if (command === "read_external_file")
				return name === "index.html" ? "<h1>Opened HTML target</h1>" : "# Opened Markdown target";
			if (command === "resolve_markdown_link")
				return {
					kind: "file",
					absolute_path: path,
					open_path: path,
					is_directory: name === "directory",
					same_document: false,
				};
			if (command === "stat_path") return { exists: true, is_dir: false, size: 100 };
			throw new Error(`Unexpected ${command}`);
		});
		const { getByRole, getByText, container } = render(() => (
			<FilesScreen initialRepo={{ cwd: "/repo", worktreePath: "/repo" }} />
		));
		fireEvent.click(await waitFor(() => getByRole("button", { name: "report.md" })));
		fireEvent.click(await waitFor(() => getByText("target")));
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("resolve_markdown_link", { root: "/repo", currentFile: "report.md", href }),
		);
		if (name === "directory")
			await waitFor(() => expect(rpc).toHaveBeenCalledWith("list_directory", { repoPath: path, subdir: "" }));
		else if (name === "image.png")
			await waitFor(() => expect(container.querySelector("img")?.getAttribute("src")).toContain("/fs/markdown-image"));
		else if (name === "index.html")
			await waitFor(() =>
				expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("Opened HTML target"),
			);
		else await waitFor(() => expect(getByRole("heading", { name: "Opened Markdown target" })).toBeTruthy());
	});
});
