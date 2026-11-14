// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, openConfirmedLink } = vi.hoisted(() => ({ mockInvoke: vi.fn(), openConfirmedLink: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: mockInvoke, listen: vi.fn().mockResolvedValue(vi.fn()) }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn(), openLocalPath: vi.fn(), openConfirmedLink }));
vi.mock("@tauri-apps/api/event", () => ({
	emitTo: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { editorTabsStore } from "../../stores/editorTabs";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";

const TARGET = "//nas.example/share/spec.md";

describe("Markdown tab network links", () => {
	afterEach(async () => {
		await new Promise((resolve) => setTimeout(resolve, 50));
		cleanup();
		vi.restoreAllMocks();
		mdTabsStore.clearAll();
	});

	// Catches: an `external` resolution being dropped (or opened as a local file) instead of
	// reaching the owner-confirmed handler route on the click.
	it("sends a clicked UNC link to the confirmed handler route and opens no tab", async () => {
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(command === "resolve_markdown_link" ? { kind: "external", target: TARGET } : `[spec](${TARGET})`),
		);
		const id = mdTabsStore.addMcpFile("net", "/repo", "/repo/doc.md", false, false);
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		await waitFor(() => expect(container.querySelector("a")).not.toBeNull());
		expect(openConfirmedLink).not.toHaveBeenCalled();
		const openMd = vi.spyOn(mdTabsStore, "add");
		const openEditor = vi.spyOn(editorTabsStore, "add");
		fireEvent.click(container.querySelector("a") as HTMLAnchorElement);
		await waitFor(() => expect(openConfirmedLink).toHaveBeenCalledWith(TARGET));
		expect(openMd).not.toHaveBeenCalled();
		expect(openEditor).not.toHaveBeenCalled();
	});
});
