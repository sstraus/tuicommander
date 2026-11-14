// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn(), openLocalPath: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
	emitTo: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { activityStore } from "../../stores/activityStore";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { toastsStore } from "../../stores/toasts";

const DOC = "/Users/boss/.tmp/decisions.md";
const HOME_FILE = "/Users/boss/x/y.html";
const BODY = ["Open `~/x/y.html`, then `~/x/gone.html`.", "", "```", "~/x/fenced.html", "```"].join("\n");

describe("Markdown tab links bare paths in prose and inline code", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockImplementation((command: string, args: { href?: string }) => {
			if (command !== "resolve_markdown_link") return Promise.resolve(BODY);
			if (args.href === "~/x/y.html") {
				return Promise.resolve({
					kind: "file",
					absolute_path: HOME_FILE,
					open_path: HOME_FILE,
					is_directory: false,
					same_document: false,
				});
			}
			return Promise.resolve({ kind: "missing", path: args.href });
		});
		mdTabsStore.clearAll();
	});

	afterEach(async () => {
		await new Promise((resolve) => setTimeout(resolve, 50));
		cleanup();
		mdTabsStore.clearAll();
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		activityStore.clearAll();
	});

	// Catches: MarkdownTab not enabling autoLinkFiles, so a backticked ~/ path stays inert code.
	it("renders a ~/ inline-code path as a link that resolves to the home file, keeping fences and dead paths as text", async () => {
		const id = mdTabsStore.addMcpFile("decisions", "/repo", DOC, false, false);
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		await waitFor(() => expect(container.querySelector("a[data-tuic-href]")).not.toBeNull());
		const hrefs = () =>
			[...container.querySelectorAll<HTMLAnchorElement>("a[data-tuic-href]")].map((a) => a.dataset.tuicHref);
		expect(hrefs()).toEqual(["~/x/y.html", "~/x/gone.html"]);
		expect(container.querySelector("pre a")).toBeNull();

		fireEvent.click(container.querySelector("a[data-tuic-href='~/x/y.html']") as HTMLAnchorElement);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", expect.objectContaining({ href: "~/x/y.html" })),
		);

		fireEvent.click(container.querySelector("a[data-tuic-href='~/x/gone.html']") as HTMLAnchorElement);
		await waitFor(() => expect(hrefs()).toEqual(["~/x/y.html"]));
	});
});
