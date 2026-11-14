import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const KEY = "tui-commander-mcp-markdown-reload";

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: vi.fn().mockResolvedValue(undefined),
}));

const stored = () => JSON.parse(sessionStorage.getItem(KEY) ?? "null") as { tabs: { mcpUiId: string }[] } | null;
const seed = (ids: string[]) =>
	sessionStorage.setItem(
		KEY,
		JSON.stringify({
			tabs: ids.map((id) => ({ mcpUiId: id, repoPath: "", filePath: `/d/${id}.md`, pinned: false })),
		}),
	);

async function boot(handleBranchSelect: () => Promise<void>) {
	vi.resetModules();
	const { listen } = await import("@tauri-apps/api/event");
	const handlers: Record<string, (event: { payload: unknown }) => void> = {};
	vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
		handlers[event] = handler;
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	const { initApp } = await import("../../hooks/useAppInit");
	const { mdTabsStore } = await import("../../stores/mdTabs");
	const { repositoriesStore } = await import("../../stores/repositories");
	repositoriesStore.add({ path: "/repo", displayName: "repo" });
	repositoriesStore.setWorkspace("/repo", "main", { branchName: "main", worktreePath: "/repo" });
	repositoriesStore.setActiveWorkspace("/repo", "main");
	repositoriesStore.setActive("/repo");
	const deps = {
		pty: { listActiveSessions: vi.fn().mockResolvedValue([]), close: vi.fn().mockResolvedValue(undefined) },
		setStatusInfo: vi.fn(),
		handleBranchSelect,
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
	} as unknown as Parameters<typeof initApp>[0];
	return { initApp, deps, mdTabsStore, handlers };
}

describe("initApp MCP Markdown restore (1424)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		sessionStorage.clear();
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
	});

	it("a ui-tab event during a slow branch restore keeps the old snapshot until arming, then saves both — catches: arming or a save before the awaited branch restore finishes drops the unread tab", async () => {
		seed(["old"]);
		let release!: () => void;
		const gate = new Promise<void>((resolve) => {
			release = resolve;
		});
		const { initApp, deps, mdTabsStore, handlers } = await boot(() => gate);
		const done = initApp(deps);
		await vi.waitFor(() => expect(handlers["ui-tab"]).toBeTypeOf("function"));
		handlers["ui-tab"]({
			payload: { id: "fresh", title: "F", html: "", pinned: false, url: "tuic://open//d/fresh.md" },
		});
		expect(stored()?.tabs.map((t) => t.mcpUiId)).toEqual(["old"]);
		release();
		await done;
		expect(
			mdTabsStore
				.getIds()
				.map((id) => mdTabsStore.get(id)?.mcpUiId)
				.sort(),
		).toEqual(["fresh", "old"]);
		expect(
			stored()
				?.tabs.map((t) => t.mcpUiId)
				.sort(),
		).toEqual(["fresh", "old"]);
	});

	it("a synchronous throw from handleBranchSelect still propagates and still arms — catches: finally replaced by catch that swallows, or restore skipped on sync throw", async () => {
		seed(["old"]);
		const failure = new Error("sync boom");
		const { initApp, deps, mdTabsStore } = await boot(() => {
			throw failure;
		});
		await expect(initApp(deps)).rejects.toBe(failure);
		expect(mdTabsStore.getCount()).toBe(1);
		mdTabsStore.addMcpFile("later", "", "/d/later.md", false, true);
		expect(
			stored()
				?.tabs.map((t) => t.mcpUiId)
				.sort(),
		).toEqual(["later", "old"]);
	});

	it("a second initApp in the same document does not replay a snapshot written after the first — catches: restore not idempotent per document", async () => {
		const { initApp, deps, mdTabsStore } = await boot(() => Promise.resolve());
		await initApp(deps);
		seed(["stale"]);
		await initApp(deps);
		expect(mdTabsStore.getCount()).toBe(0);
	});
});
