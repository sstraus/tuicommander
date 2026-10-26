import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("../../invoke", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../invoke")>();
	return { ...actual, invoke: invokeMock };
});

import { type ActionEntry, getActionEntries } from "../../actions/actionRegistry";
import { CommandPalette, isBrowserCommandPaletteAction } from "../../components/CommandPalette/CommandPalette";
import type { ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";
import { emitLocalEvent } from "../../invoke";
import { commandPaletteStore } from "../../stores/commandPalette";
import { repositoriesStore } from "../../stores/repositories";
import type { ContentSearchResult, DirEntry } from "../../types/fs";

function setTauriEnv(on: boolean) {
	const global = globalThis as Record<string, unknown>;
	if (on) {
		global.__TAURI_INTERNALS__ = {};
		delete global.__TAURI_SHIM__;
	} else {
		delete global.__TAURI_INTERNALS__;
	}
}

function action(id: string, label: string, execute = vi.fn()): ActionEntry {
	return { id, label, category: "Test", keybinding: "", execute };
}

async function flushPromises(): Promise<void> {
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
}

const FILE_RESULT: DirEntry = {
	name: "App.tsx",
	path: "src/App.tsx",
	is_dir: false,
	size: 100,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
};

const CONTENT_RESULT: ContentSearchResult = {
	matches: [
		{
			path: "src/App.tsx",
			line_number: 10,
			line_text: "const browserNeedle = true;",
			match_start: 6,
			match_end: 19,
		},
	],
	files_searched: 12,
	files_skipped: 0,
	truncated: false,
	repos_pending: 0,
	repos_indexing: 0,
	repos_searched: 0,
};

describe("CommandPalette browser mode", () => {
	beforeEach(() => {
		setTauriEnv(false);
		vi.useFakeTimers();
		invokeMock.mockReset().mockResolvedValue(undefined);
		commandPaletteStore.close();
		localStorage.clear();
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setActive("/repo");
	});

	afterEach(() => {
		commandPaletteStore.close();
		repositoriesStore._testCancelPendingSave();
		setTauriEnv(true);
		vi.useRealTimers();
	});

	it("exposes only actions explicitly verified for browser execution", () => {
		const supported = action("search-files", "Search Files");
		const designMode = action("start-design-mode-edge", "Start Design Mode in Edge");
		const designModeChrome = action("start-design-mode-chrome", "Start Design Mode in Chrome");
		const stopDesignMode = action("stop-design-mode", "Stop Design Mode");
		const nativeDialog = action("open-file", "Open file");
		const hostAdministration = action("show-remote-qr", "QR for Remote Mobile Connection");
		const unknown = action("future-native-action", "Future native action");
		const { container } = render(() => (
			<CommandPalette actions={[supported, designMode, nativeDialog, hostAdministration, unknown]} browserMode />
		));

		commandPaletteStore.open();

		expect(isBrowserCommandPaletteAction(supported)).toBe(true);
		expect(isBrowserCommandPaletteAction(designMode)).toBe(true);
		expect(isBrowserCommandPaletteAction(designModeChrome)).toBe(true);
		// Stop must reach a browser client too: otherwise the tab menu is its only way out.
		expect(isBrowserCommandPaletteAction(stopDesignMode)).toBe(true);
		// The arm path opens this tab's own microphone over the voice socket.
		expect(isBrowserCommandPaletteAction(action("toggle-hands-free", "Start hands-free conversation"))).toBe(true);
		expect(isBrowserCommandPaletteAction(nativeDialog)).toBe(false);
		expect(isBrowserCommandPaletteAction(hostAdministration)).toBe(false);
		expect(container.textContent).toContain("Search Files");
		expect(container.textContent).toContain("Start Design Mode in Edge");
		expect(container.textContent).not.toContain("Open file");
		expect(container.textContent).not.toContain("QR for Remote Mobile Connection");
		expect(container.textContent).not.toContain("Future native action");
	});

	// Catches: the Plans and Stories entry is registered but missing from BROWSER_ACTION_IDS, so a browser or phone client never sees it.
	it("lists the real Plans and Stories entry in browser mode", () => {
		const entry = getActionEntries({} as ShortcutHandlers).find((candidate) => candidate.id === "plans-stories");
		expect(entry).toBeDefined();
		expect(isBrowserCommandPaletteAction(entry as ActionEntry)).toBe(true);
	});

	it("keeps the complete desktop action list when browser mode is off", () => {
		const nativeDialog = action("open-file", "Open file");
		const { container } = render(() => <CommandPalette actions={[nativeDialog]} />);

		commandPaletteStore.open();

		expect(container.textContent).toContain("Open file");
	});

	it("shows the empty state when no browser-supported commands match", () => {
		const { container } = render(() => <CommandPalette actions={[action("open-file", "Open file")]} browserMode />);

		commandPaletteStore.open();

		expect(container.textContent).toContain("No matching commands");
	});

	it("executes a browser-supported command and closes", () => {
		const execute = vi.fn();
		const { getByText } = render(() => (
			<CommandPalette actions={[action("search-files", "Search Files", execute)]} browserMode />
		));
		commandPaletteStore.open();

		fireEvent.click(getByText("Search Files"));

		expect(execute).toHaveBeenCalledOnce();
		expect(commandPaletteStore.state.isOpen).toBe(false);
	});

	it("completes filename search through the shared invoke transport", async () => {
		invokeMock.mockImplementation((command: string) =>
			command === "search_files" ? Promise.resolve([FILE_RESULT]) : Promise.resolve(undefined),
		);
		const { container } = render(() => <CommandPalette actions={[]} browserMode />);
		commandPaletteStore.open();
		const input = container.querySelector<HTMLInputElement>('[aria-label="Command palette search"]')!;

		fireEvent.input(input, { target: { value: "! App" } });
		await vi.advanceTimersByTimeAsync(300);
		await flushPromises();

		expect(invokeMock).toHaveBeenCalledWith("search_files", {
			repoPath: "/repo",
			query: "App",
			limit: 50,
		});
		expect(commandPaletteStore.state.filenameSearching).toBe(false);
		expect(container.textContent).toContain("src/App.tsx");
	});

	/**
	 * The empty state used to say "N still indexing, retry shortly" for every
	 * unsearched repo. Under the default `active_and_switch` strategy nothing
	 * schedules an unvisited repo, so retrying changed nothing — measured at 40
	 * pending repos across four polls over 80s. The palette must spend the
	 * backend's `repos_indexing` count, not assume pending means building.
	 */
	it("does not promise a retry for repos that nothing is building", async () => {
		invokeMock.mockImplementation((command: string) =>
			command === "search_content"
				? Promise.resolve({
						matches: [],
						files_searched: 3,
						files_skipped: 0,
						truncated: false,
						repos_pending: 40,
						repos_indexing: 0,
						repos_searched: 1,
					})
				: Promise.resolve(undefined),
		);
		const { container } = render(() => <CommandPalette actions={[]} browserMode />);
		commandPaletteStore.open();
		const input = container.querySelector<HTMLInputElement>('[aria-label="Command palette search"]')!;

		fireEvent.input(input, { target: { value: "? missingNeedle" } });
		await vi.advanceTimersByTimeAsync(300);
		await flushPromises();

		expect(container.textContent).toContain("No results in 1 repo — 40 not indexed");
		expect(container.textContent).not.toContain("retry shortly");
	});

	/** The other half: a build really in flight earns the retry sentence. */
	it("promises a retry only for repos with a build in flight", async () => {
		invokeMock.mockImplementation((command: string) =>
			command === "search_content"
				? Promise.resolve({
						matches: [],
						files_searched: 3,
						files_skipped: 0,
						truncated: false,
						repos_pending: 2,
						repos_indexing: 2,
						repos_searched: 3,
					})
				: Promise.resolve(undefined),
		);
		const { container } = render(() => <CommandPalette actions={[]} browserMode />);
		commandPaletteStore.open();
		const input = container.querySelector<HTMLInputElement>('[aria-label="Command palette search"]')!;

		fireEvent.input(input, { target: { value: "? missingNeedle" } });
		await vi.advanceTimersByTimeAsync(300);
		await flushPromises();

		expect(container.textContent).toContain("No results in 3 repos — 2 still indexing (retry shortly)");
	});

	it("completes HTTP content search without accepting another window's batch", async () => {
		let resolveContent!: (result: ContentSearchResult) => void;
		const response = new Promise<ContentSearchResult>((resolve) => {
			resolveContent = resolve;
		});
		invokeMock.mockImplementation((command: string) =>
			command === "search_content" ? response : Promise.resolve(undefined),
		);
		const { container } = render(() => <CommandPalette actions={[]} browserMode />);
		commandPaletteStore.open();
		const input = container.querySelector<HTMLInputElement>('[aria-label="Command palette search"]')!;

		fireEvent.input(input, { target: { value: "? browserNeedle" } });
		await vi.advanceTimersByTimeAsync(300);
		await flushPromises();

		emitLocalEvent("content-search-batch", {
			search_id: "another-window",
			matches: [{ ...CONTENT_RESULT.matches[0], line_text: "wrong window result" }],
			is_final: true,
			files_searched: 1,
			files_skipped: 0,
			truncated: false,
			repos_pending: 0,
			repos_indexing: 0,
			repos_searched: 0,
		});
		expect(container.textContent).not.toContain("wrong window result");
		expect(commandPaletteStore.state.contentSearching).toBe(true);

		resolveContent(CONTENT_RESULT);
		await flushPromises();

		expect(invokeMock).toHaveBeenCalledWith(
			"search_content",
			expect.objectContaining({
				repoPath: "/repo",
				query: "browserNeedle",
				caseSensitive: false,
				useRegex: false,
				wholeWord: false,
				searchId: expect.any(String),
			}),
		);
		expect(commandPaletteStore.state.contentSearching).toBe(false);
		expect(container.textContent).toContain("const browserNeedle = true;");
	});
});
