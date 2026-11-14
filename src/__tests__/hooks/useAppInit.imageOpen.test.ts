import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn().mockResolvedValue(undefined) }));

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { listen } from "@tauri-apps/api/event";
import { type AppInitDeps, initApp } from "../../hooks/useAppInit";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

type UiTabPayload = { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };

function resetStores() {
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
	for (const id of mdTabsStore.getIds()) mdTabsStore.remove(id);
	for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
}

function createMockDeps(): AppInitDeps {
	return {
		pty: { listActiveSessions: vi.fn().mockResolvedValue([]), close: vi.fn().mockResolvedValue(undefined) },
		setStatusInfo: vi.fn(),
		handleBranchSelect: vi.fn().mockResolvedValue(undefined),
		refreshAllBranchStats: vi.fn(),
		getDefaultFontSize: () => 14,
		stores: {
			hydrate: vi.fn().mockResolvedValue(undefined),
			startPolling: vi.fn(),
			stopPolling: vi.fn(),
			startAutoFetch: vi.fn(),
			startPrNotificationTimer: vi.fn(),
			loadFontFromConfig: vi.fn(),
			startDictationSettingsSync: vi.fn(() => vi.fn()),
			refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
			startUserActivityListening: vi.fn(),
		},
		applyPlatformClass: vi.fn().mockReturnValue("macos"),
		registerRepo: vi.fn().mockResolvedValue(undefined),
	};
}

async function startWithUiTabSender() {
	let handler: ((event: { payload: UiTabPayload }) => void) | null = null;
	vi.mocked(listen).mockImplementation(((event: string, h: (event: { payload: unknown }) => void) => {
		if (event === "ui-tab") handler = h as typeof handler;
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	await initApp(createMockDeps());
	return (id: string, url: string, focus: boolean) =>
		handler!({ payload: { id, title: id, html: "", pinned: false, url, focus } });
}

function addRepo(path: string) {
	repositoriesStore.add({ path, displayName: path.split("/").pop()! });
	repositoriesStore.setWorkspace(path, "main", { branchName: "main", worktreePath: path });
	repositoriesStore.setActiveWorkspace(path, "main");
}

describe("tuic://open of an image (1335)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		resetStores();
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	// Catches: preview branch ignores focus=false, so a background image open steals the
	// active tab in a repo the tab bar is not showing (the ghost tab the md branch avoids).
	it("keeps an image opened with focus=false in the background", async () => {
		const send = await startWithUiTabSender();
		addRepo("/repos/source");
		addRepo("/repos/target");
		repositoriesStore.setActive("/repos/source");
		send("img", "tuic://open//repos/target/shots/a.png", false);
		const tab = Object.values(mdTabsStore.state.tabs).find((t) => t.type === "html-preview");
		expect(tab).toBeDefined();
		expect(repositoriesStore.state.activeRepoPath).toBe("/repos/source");
		expect(mdTabsStore.getActive()?.id).not.toBe(tab!.id);
	});

	// Catches: an image outside every repo with no repo registered creates a tab the bar filters out.
	it("shows an image outside any repository when no repository exists", async () => {
		const send = await startWithUiTabSender();
		send("img", "tuic://open//Users/boss/Gits/.tmp/boss/a.png", true);
		const tab = mdTabsStore.getActive();
		expect(tab).toMatchObject({ type: "html-preview", filePath: "/Users/boss/Gits/.tmp/boss/a.png" });
		expect(mdTabsStore.getVisibleIds(null)).toContain(tab!.id);
	});

	// Catches: the preview tab is not keyed by the MCP ui id, so re-sending the same id with a
	// different file leaves the previous preview open instead of replacing it.
	it("replaces the previous tab when the same id is re-sent for another file", async () => {
		const send = await startWithUiTabSender();
		send("same", "tuic://open//Users/boss/Gits/.tmp/boss/a.png", true);
		send("same", "tuic://open//Users/boss/Gits/.tmp/boss/b.png", true);
		const previews = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "html-preview");
		expect(previews.map((t) => t.filePath)).toEqual(["/Users/boss/Gits/.tmp/boss/b.png"]);
	});

	// Catches: a repo-owned image keeps an absolute path, so HtmlPreviewTab loses the repo root.
	it("stores a repo-owned image relative to its repository", async () => {
		const send = await startWithUiTabSender();
		addRepo("/repos/owner");
		send("img", "tuic://open//repos/owner/docs/a.png", true);
		expect(mdTabsStore.getActive()).toMatchObject({
			type: "html-preview",
			repoPath: "/repos/owner",
			filePath: "docs/a.png",
		});
	});
});
