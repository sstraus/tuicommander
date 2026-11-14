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

function resetStores() {
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
	for (const id of mdTabsStore.getIds()) mdTabsStore.remove(id);
	for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
}

function deps(): AppInitDeps {
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

type Emit = (p: { id: string; url: string; pinned?: boolean; focus?: boolean }) => void;

async function boot(): Promise<Emit> {
	let cb: ((e: { payload: Record<string, unknown> }) => void) | null = null;
	vi.mocked(listen).mockImplementation(((event: string, handler: (e: { payload: unknown }) => void) => {
		if (event === "ui-tab") cb = handler as typeof cb;
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	await initApp(deps());
	return (p) => cb!({ payload: { title: "t", html: "", pinned: false, focus: true, ...p } });
}

describe("tuic://open image tabs (1335)", () => {
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

	// Catches: an MCP id first opened as an image and then re-opened as a repo Markdown file leaves the
	// image preview tab behind (the md branches never close an html-preview owned by the same id).
	it("reusing an id for a repo markdown file closes the image preview that id owned", async () => {
		repositoriesStore.add({ path: "/repos/a", displayName: "a" });
		const emit = await boot();
		emit({ id: "slot", url: "tuic://open//repos/a/shot.png" });
		emit({ id: "slot", url: "tuic://open//repos/a/notes.md" });
		const previews = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "html-preview");
		expect(previews).toHaveLength(0);
	});

	// Catches: same stale preview when the replacement is a Markdown file outside any repo.
	it("reusing an id for an external markdown file closes the image preview that id owned", async () => {
		const emit = await boot();
		emit({ id: "slot", url: "tuic://open//Users/boss/Gits/.tmp/shot.png" });
		emit({ id: "slot", url: "tuic://open//Users/boss/Gits/.tmp/notes.md" });
		const previews = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "html-preview");
		expect(previews).toHaveLength(0);
	});

	// Catches: re-emitting the same image id spawns a second tab instead of replacing the first.
	it("re-opening the same image id keeps exactly one preview tab", async () => {
		const emit = await boot();
		emit({ id: "slot", url: "tuic://open//Users/boss/Gits/.tmp/shot.png" });
		emit({ id: "slot", url: "tuic://open//Users/boss/Gits/.tmp/shot.png" });
		expect(Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "html-preview")).toHaveLength(1);
	});

	// Catches: pinned flag from the MCP open dropped for image tabs (markdown tabs honour it).
	it("honours pinned for an image opened via MCP", async () => {
		const emit = await boot();
		emit({ id: "pin", url: "tuic://open//Users/boss/Gits/.tmp/shot.png", pinned: true });
		const tab = Object.values(mdTabsStore.state.tabs).find((t) => t.type === "html-preview");
		expect(tab?.pinned).toBe(true);
	});

	// Catches: a user-opened preview of the same file being replaced/closed by the MCP image open (or the
	// reverse: addHtmlPreview handing back the MCP-owned tab, which a later MCP close would then destroy).
	it("a user-opened preview of the same image survives an MCP open and close of it", async () => {
		const path = "/Users/boss/Gits/.tmp/shot.png";
		const userTab = mdTabsStore.addHtmlPreview("", path);
		const emit = await boot();
		emit({ id: "slot", url: `tuic://open/${path}` });
		expect(mdTabsStore.state.tabs[userTab]).toBeDefined();
		mdTabsStore.closeMcpFile("slot");
		expect(mdTabsStore.state.tabs[userTab]).toBeDefined();
		expect(Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "html-preview")).toHaveLength(1);
	});

	// Catches: upper-case extension (SHOT.PNG) not recognised as an image and sent to the editor.
	it("treats an upper-case image extension as an image", async () => {
		const emit = await boot();
		emit({ id: "up", url: "tuic://open//Users/boss/Gits/.tmp/SHOT.PNG" });
		expect(mdTabsStore.getActive()).toMatchObject({ type: "html-preview" });
		expect(editorTabsStore.getActive()).toBeUndefined();
	});

	// Catches: a dotted directory name ("shots.png/readme") misread as an image extension.
	it("does not treat a file inside a dir named *.png as an image", async () => {
		const emit = await boot();
		emit({ id: "d", url: "tuic://open//Users/boss/Gits/.tmp/shots.png/readme" });
		expect(Object.values(mdTabsStore.state.tabs).some((t) => t.type === "html-preview")).toBe(false);
	});

	// Catches: a background image open (focus=false) stealing the active tab.
	it("keeps the active tab when an image is opened with focus=false", async () => {
		const emit = await boot();
		const keep = mdTabsStore.addHtmlPreview("", "/x/other.png");
		emit({ id: "bg", url: "tuic://open//Users/boss/Gits/.tmp/shot.png", focus: false });
		expect(mdTabsStore.getActive()?.id).toBe(keep);
	});
});
