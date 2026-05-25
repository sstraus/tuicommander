/**
 * Tests for storiesTickerPlugin.
 *
 * The plugin calls invoke("list_directory") on each refresh and listen("dir-changed")
 * to watch for filesystem changes. We mock both via vi.mock("../../invoke").
 *
 * Tests cover:
 * - Ticker reflects open story count from repeated refreshes
 * - Error handling in refresh (not-found vs real errors)
 * - Listener leak fix: rapid repo switch scenario where watchedDir changes
 *   before the listen().then() promise resolves — the stale unlisten must be
 *   called immediately instead of stored.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue([]),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { invoke, listen } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { storiesTickerPlugin } from "../../plugins/storiesTickerPlugin";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { statusBarTicker } from "../../stores/statusBarTicker";
import type { DirEntry } from "../../types/fs";

const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);

/** Flush all pending microtasks/promises */
const flush = () => new Promise<void>((resolve) => queueMicrotask(resolve));
/** Flush twice for nested promise chains (.then inside .then) */
const flushAll = async () => {
	await flush();
	await flush();
	await flush();
};

/** Build a minimal DirEntry fixture for a story file */
function storyFile(name: string): DirEntry {
	return {
		name,
		is_dir: false,
		path: `/repo/stories/${name}`,
		size: 100,
		modified_at: 0,
		git_status: "",
		is_ignored: false,
	};
}

beforeEach(() => {
	pluginRegistry.clear();
	statusBarTicker.clear();
	repositoriesStore.setActive(null);
	mockedInvoke.mockReset().mockResolvedValue([]);
	mockedListen.mockReset().mockResolvedValue(() => {});
});

afterEach(() => {
	pluginRegistry.clear();
	statusBarTicker.clear();
	repositoriesStore.setActive(null);
	repositoriesStore._testCancelPendingSave();
});

// ---------------------------------------------------------------------------
// Refresh counting — ticker reflects latest open count
// ---------------------------------------------------------------------------

describe("refresh: ticker reflects open story count", () => {
	it("sets ticker when there are open stories on load", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		mockedInvoke.mockResolvedValue([storyFile("abc-in_progress-feature.md")]);

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		const msg = statusBarTicker.getAll().find((m) => m.id === "open-count");
		expect(msg).toBeDefined();
		expect(msg?.text).toBe("1 open");
	});

	it("clears ticker when all stories are closed", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		// Complete status in filename → not open
		mockedInvoke.mockResolvedValue([storyFile("abc-complete-feature.md")]);

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		const msg = statusBarTicker.getAll().find((m) => m.id === "open-count");
		expect(msg).toBeUndefined();
	});

	it("counts only non-closed .md files", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		mockedInvoke.mockResolvedValue([
			storyFile("abc-in_progress-feature.md"), // open
			storyFile("def-complete-other.md"), // closed
			storyFile("ghi-wontfix-bug.md"), // closed
			storyFile("jkl-ready-task.md"), // open
			{
				name: "notes.txt",
				is_dir: false,
				path: "/repo/stories/notes.txt",
				size: 50,
				modified_at: 0,
				git_status: "",
				is_ignored: false,
			}, // non-md
		]);

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		const msg = statusBarTicker.getAll().find((m) => m.id === "open-count");
		expect(msg?.text).toBe("2 open");
	});

	it("updates ticker count after repo-changed emits a new refresh", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");

		// First refresh: 1 story
		mockedInvoke.mockResolvedValueOnce([storyFile("abc-ready-feature.md")]);
		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		// Second refresh: 3 stories — triggered by repo-changed
		mockedInvoke.mockResolvedValue([
			storyFile("abc-ready-feature.md"),
			storyFile("def-in_progress-task.md"),
			storyFile("ghi-ready-bug.md"),
		]);
		pluginRegistry.notifyStateChange({
			type: "repo-changed",
			sessionId: null,
			terminalId: "t1",
			detail: "/repo",
		});
		await flushAll();

		const msg = statusBarTicker.getAll().find((m) => m.id === "open-count");
		expect(msg?.text).toBe("3 open");
	});
});

// ---------------------------------------------------------------------------
// Error handling in refresh
// ---------------------------------------------------------------------------

describe("refresh: error handling", () => {
	it("does NOT warn when stories dir is not found", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		mockedInvoke.mockRejectedValue(new Error("not found"));

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		expect(appLogger.warn).not.toHaveBeenCalledWith(
			"plugin",
			expect.stringContaining("Failed to scan stories"),
			expect.anything(),
		);
	});

	it("does NOT warn when stories dir returns 'No such file'", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		mockedInvoke.mockRejectedValue(new Error("No such file or directory"));

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		expect(appLogger.warn).not.toHaveBeenCalledWith(
			"plugin",
			expect.stringContaining("Failed to scan stories"),
			expect.anything(),
		);
	});

	it("warns on real errors (not a missing-dir error)", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		mockedInvoke.mockRejectedValue(new Error("Permission denied"));

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		expect(appLogger.warn).toHaveBeenCalledWith("plugin", "[stories-ticker] Failed to scan stories", expect.any(Error));
	});

	it("clears ticker on error", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		// First a successful refresh to set a ticker
		mockedInvoke.mockResolvedValueOnce([storyFile("abc-ready-feature.md")]);
		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();
		expect(statusBarTicker.getAll().find((m) => m.id === "open-count")).toBeDefined();

		// Now a failed refresh
		mockedInvoke.mockRejectedValue(new Error("disk error"));
		pluginRegistry.notifyStateChange({
			type: "repo-changed",
			sessionId: null,
			terminalId: "t1",
			detail: "/repo",
		});
		await flushAll();

		expect(statusBarTicker.getAll().find((m) => m.id === "open-count")).toBeUndefined();
	});
});

// ---------------------------------------------------------------------------
// Listener leak fix: rapid repo switch
// ---------------------------------------------------------------------------

describe("listener leak fix: rapid repo switch", () => {
	it("calls unlisten immediately when watchedDir changed before .then() resolves", async () => {
		repositoriesStore.add({ path: "/repo-a", displayName: "repo-a" });
		repositoriesStore.add({ path: "/repo-b", displayName: "repo-b" });
		repositoriesStore.setActive("/repo-a");

		// listen() returns a promise that we control — hold it pending initially
		let resolveListenA!: (unlisten: () => void) => void;
		const unlistenA = vi.fn();
		const listenPromiseA = new Promise<() => void>((resolve) => {
			resolveListenA = resolve;
		});

		// First call to listen (for repo-a) holds
		mockedListen.mockReturnValueOnce(listenPromiseA as ReturnType<typeof listen>);

		pluginRegistry.register(storiesTickerPlugin);
		// At this point, watchStoriesDir("/repo-a") called listen — promise is pending

		// Switch to repo-b before the listen promise for repo-a resolves
		repositoriesStore.setActive("/repo-b");
		pluginRegistry.notifyStateChange({
			type: "repo-changed",
			sessionId: null,
			terminalId: "t1",
			detail: "/repo-b",
		});
		await flushAll();
		// watchedDir is now "/repo-b/stories"; the repo-a listen is still pending

		// Now resolve the repo-a listen promise — the plugin must detect the mismatch
		// and call unlisten immediately (not store it)
		resolveListenA(unlistenA);
		await flushAll();

		expect(unlistenA).toHaveBeenCalledOnce();
	});

	it("stores unlisten handle when watchedDir matches at resolution time", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");

		const unlistenFn = vi.fn();
		// listen resolves synchronously via mockResolvedValue
		mockedListen.mockResolvedValue(unlistenFn);

		pluginRegistry.register(storiesTickerPlugin);
		await flushAll();

		// No repo switch happened — unlistenFn should NOT have been called
		expect(unlistenFn).not.toHaveBeenCalled();

		// Cleanup: unregister calls stopWatch which should call it
		pluginRegistry.unregister(storiesTickerPlugin.id);
		expect(unlistenFn).toHaveBeenCalledOnce();
	});
});
