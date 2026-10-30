// @vitest-environment jsdom
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HtmlPreviewTab } from "../../components/HtmlPreviewTab/HtmlPreviewTab";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import { FilesScreen } from "../../mobile/screens/FilesScreen";
import type { HtmlPreviewTab as PreviewTab } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { rewriteLocalImages } from "../../utils/repoImageUrl";

vi.mock("../../transport", async (original) => ({
	...(await original<object>()),
	isTauri: () => false,
	rpc: vi.fn(async (command: string) => {
		if (command === "resolve_terminal_path") return { absolute_path: "/repo/docs/page.md", is_directory: false };
		if (command === "load_repositories") return { repos: { "/repo": {} } };
		if (command === "stat_path") return { exists: true, is_dir: false, size: 20 };
		if (command === "fs_read_file") return "![x](my%20shot.png)";
		throw new Error(`Unexpected command: ${command}`);
	}),
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue('<html><head></head><body><img src="shot.png"></body></html>'),
	listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ readFile: vi.fn().mockResolvedValue('<img src="shot.png">') }),
}));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn(), openLocalPath: vi.fn() }));

beforeEach(() => repositoriesStore.add({ path: "/repo", displayName: "Repo" }));
afterEach(() => {
	cleanup();
	repositoriesStore._testCancelPendingSave();
});
function tab(filePath: string): PreviewTab {
	return {
		id: "preview",
		title: "Preview",
		type: "html-preview",
		repoPath: "",
		filePath,
		fileName: filePath.split("/").pop() ?? "",
	};
}
function imageUrl(container: HTMLElement): URL {
	const src = container.querySelector("img")?.getAttribute("src");
	if (!src) throw new Error("Expected a repository image");
	return new URL(src);
}
describe("browser repository preview boundary", () => {
	it.each(["![relative](shot.png)", "![absolute](/outside/shot.png)"])(
		"replaces an external Markdown image with a placeholder — catches: bare local paths producing broken images (%s)",
		(content) => {
			const { container, getByRole } = render(() => <ContentRenderer content={content} baseDir="/outside" />);
			expect(container.querySelector("img")).toBeNull();
			expect(getByRole("img", { name: "Image unavailable" }).textContent).toContain("outside open repositories");
		},
	);
	it.each(["![relative](shot.png)", "![absolute](/repo/docs/shot.png)"])(
		"routes an in-repository Markdown image — catches: absolute paths or baseDir bypassing the HTTP image route (%s)",
		(content) => {
			const { container } = render(() => <ContentRenderer content={content} baseDir="/repo/docs" />);
			const url = imageUrl(container);
			expect(url.pathname).toBe("/fs/markdown-image");
			expect(url.searchParams.get("repoPath")).toBe("/repo");
			expect(url.searchParams.get("file")).toBe("docs/shot.png");
		},
	);
	it.each(["<img src='shot.png'>", "<img src=shot.png>"])(
		"resolves HTML image attribute forms — catches: only double-quoted sources being mapped (%s)",
		(html) => {
			const doc = new DOMParser().parseFromString(rewriteLocalImages(html, "/repo/docs"), "text/html");
			expect(new URL(doc.querySelector("img")?.getAttribute("src") ?? "").searchParams.get("file")).toBe(
				"docs/shot.png",
			);
		},
	);
	it("requests the filename the mobile file view resolves — catches: the browser fallback or an undecoded path overriding the mobile route", async () => {
		const { container } = render(() => (
			<FilesScreen initialRepo={{ cwd: "/repo", worktreePath: "/repo" }} initialLink={{ candidate: "docs/page.md" }} />
		));
		await waitFor(() => expect(container.querySelector("#markdown-content img")).not.toBeNull());
		const url = new URL(container.querySelector("#markdown-content img")?.getAttribute("src") ?? "");
		expect(url.pathname).toBe("/fs/markdown-image");
		expect(url.searchParams.get("repoPath")).toBe("/repo");
		expect(url.searchParams.get("file")).toBe("docs/my shot.png");
	});
	it("replaces an external mobile absolute path — catches: mobile imageSrc returning an unservable local path", () => {
		const { container, getByRole } = render(() => (
			<ContentRenderer content="![x](/outside/shot.png)" imageSrc={(path) => path} />
		));
		expect(container.querySelector("img")).toBeNull();
		expect(getByRole("img", { name: "Image unavailable" })).toBeDefined();
	});
	it("routes an absolute repository image tab — catches: absolute tab paths falling through to convertFileSrc", () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/repo/docs/shot.png")} />);
		expect(imageUrl(container).searchParams.get("file")).toBe("docs/shot.png");
	});
	it("routes an explicitly opened external image — catches: local image tabs replaced by a placeholder", () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/outside/shot.png")} />);
		const url = imageUrl(container);
		expect(url.pathname).toBe("/fs/markdown-image");
		expect(url.searchParams.get("repoPath")).toBe("/outside");
		expect(url.searchParams.get("file")).toBe("shot.png");
	});
	it("asks the backend to read an external HTML tab — catches: frontend registration checks blocking local previews", async () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/outside/page.html")} />);
		await waitFor(() => expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("<html>"));
	});
	it("routes local images in an HTML preview — catches: HTML base tags pointing browser images at bare paths", async () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/repo/docs/page.html")} />);
		await waitFor(() =>
			expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("/fs/markdown-image"),
		);
		expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("docs%2Fshot.png");
	});
});
