import { render } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { createComponent } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emitTo: vi.fn(), listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../../utils/filePreview", () => ({ openFileAction: vi.fn() }));
vi.mock("../../hooks/initPanelWindow", () => ({ initPanelWindow: vi.fn().mockResolvedValue(undefined) }));
const captured = vi.hoisted(() => ({ reloadToken: undefined as (() => number) | undefined }));
vi.mock("../../components/MarkdownTab", () => ({
	MarkdownTab: (props: { reloadToken?: () => number }) => {
		captured.reloadToken = props.reloadToken;
		return null;
	},
}));

import { createMarkdownDocumentPanelAdapter } from "../../panelAdapters/markdownDocument";
import { mdTabsStore } from "../../stores/mdTabs";
import { openFileAction } from "../../utils/filePreview";
import { consumePendingHeading } from "../../utils/pendingHeadings";

const REPO = "/repo";

function link(over: Record<string, unknown> = {}) {
	return {
		kind: "file",
		absolute_path: `${REPO}/docs/other.md`,
		open_path: "docs/other.md",
		is_directory: false,
		same_document: false,
		anchor: "section",
		line: null,
		...over,
	};
}

describe("detached Markdown document adapter open-link", () => {
	let tabId: string;

	beforeEach(() => {
		vi.mocked(openFileAction).mockClear();
		tabId = mdTabsStore.add(REPO, "docs/index.md") as string;
	});

	// catches: the detached path forwards only target.line, so other.md#section opens at the top
	it("queues the #anchor for the file it opens in the main window", () => {
		createMarkdownDocumentPanelAdapter(tabId).handleAction?.("open-link", link());

		expect(vi.mocked(openFileAction).mock.calls[0]?.[0]).toBe("docs/other.md");
		expect(consumePendingHeading(`${REPO}/docs/other.md`)).toBe("section");
	});

	// catches: queueing an anchor for a link that has none leaves a stale scroll target
	it("queues nothing for a link without an anchor", () => {
		createMarkdownDocumentPanelAdapter(tabId).handleAction?.("open-link", link({ anchor: null }));

		expect(consumePendingHeading(`${REPO}/docs/other.md`)).toBeUndefined();
	});
});

describe("detached Markdown document reload", () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	// catches: the document reloads every 2 s with no file change
	it("markdown_detached_no_reload_without_change", async () => {
		vi.useFakeTimers();
		const tabId = mdTabsStore.add(REPO, "docs/index.md") as string;
		const adapter = createMarkdownDocumentPanelAdapter(tabId);
		const params = new URLSearchParams({ tabId, filePath: "docs/index.md", repoPath: REPO });
		captured.reloadToken = undefined;
		const view = render(() => createComponent(adapter.Component, { params }));
		await vi.advanceTimersByTimeAsync(0);
		expect(captured.reloadToken).toBeDefined();
		const start = captured.reloadToken!();

		await vi.advanceTimersByTimeAsync(6000);
		expect(captured.reloadToken!()).toBe(start);

		const onRepoChanged = vi.mocked(listen).mock.calls.find(([event]) => event === "repo-changed")?.[1];
		expect(onRepoChanged).toBeDefined();
		const emit = (repo_path: string) =>
			onRepoChanged!({ event: "repo-changed", id: 0, payload: { repo_path, kind: "working-tree" } });
		emit("/elsewhere");
		expect(captured.reloadToken!()).toBe(start);
		emit(REPO);
		expect(captured.reloadToken!()).toBe(start + 1);

		window.dispatchEvent(new Event("focus"));
		expect(captured.reloadToken!()).toBe(start + 2);
		view.unmount();
	});
});
