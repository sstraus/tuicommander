import { beforeAll, describe, expect, it, vi } from "vitest";
import { type ActionEntry, getActionEntries } from "../../actions/actionRegistry";
import * as handsFree from "../../actions/handsFreeConversation";
import type { ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";
import { automationsUi } from "../../stores/automations";
import { dictationStore } from "../../stores/dictation";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { mockInvoke } from "../mocks/tauri";

/** Turn dictation on the way the app does: by reading the backend config. */
async function enableDictation() {
	mockInvoke.mockImplementation(async (cmd: string) =>
		cmd === "get_dictation_config" ? { enabled: true, hotkey: "F5", language: "auto" } : undefined,
	);
	await dictationStore.refreshConfig();
	mockInvoke.mockReset().mockResolvedValue(undefined);
}

function createMockHandlers(): ShortcutHandlers {
	return {
		zoomIn: vi.fn(),
		zoomOut: vi.fn(),
		zoomReset: vi.fn(),
		zoomInAll: vi.fn(),
		zoomOutAll: vi.fn(),
		zoomResetAll: vi.fn(),
		createNewTerminal: vi.fn(),
		closeTerminal: vi.fn(),
		reopenClosedTab: vi.fn(),
		navigateTab: vi.fn(),
		focusLastTerminal: vi.fn(),
		historyBack: vi.fn(),
		historyForward: vi.fn(),
		jumpWaitingTerminal: vi.fn(),
		clearTerminal: vi.fn(),
		terminalIds: vi.fn().mockReturnValue([]),
		handleTerminalSelect: vi.fn(),
		handleSplit: vi.fn(),
		handleRunCommand: vi.fn(),
		switchToBranchByIndex: vi.fn(),
		isQuickSwitcherOpen: vi.fn().mockReturnValue(false),
		toggleMarkdownPanel: vi.fn(),
		toggleSidebar: vi.fn(),
		toggleSettings: vi.fn(),
		toggleTaskQueue: vi.fn(),
		toggleGitOpsPanel: vi.fn(),
		toggleHelpPanel: vi.fn(),
		toggleIdeasPanel: vi.fn(),
		toggleFileBrowserPanel: vi.fn(),
		requestFileBrowserContentSearch: vi.fn(),
		toggleOutlinePanel: vi.fn(),
		findInTerminal: vi.fn(),
		toggleCommandPalette: vi.fn(),
		toggleActivityDashboard: vi.fn(),
		toggleWorktreeManager: vi.fn(),
		toggleBranchSwitcher: vi.fn(),
		toggleErrorLog: vi.fn(),
		toggleBranchesTab: vi.fn(),
		toggleAiChatPanel: vi.fn(),
		toggleMcpPopup: vi.fn(),
		clearScrollback: vi.fn(),
		scrollToTop: vi.fn(),
		scrollToBottom: vi.fn(),
		scrollPageUp: vi.fn(),
		scrollPageDown: vi.fn(),
		toggleZoomPane: vi.fn(),
		toggleFocusMode: vi.fn(),
		closeActiveTabOrPane: vi.fn(),
		togglePromptLibrary: vi.fn(),
		toggleDiffScroll: vi.fn(),
		toggleGlobalWorkspace: vi.fn(),
		openFile: vi.fn(),
		newFile: vi.fn(),
		openFolder: vi.fn(),
		openPath: vi.fn(),
		openSecondaryWindow: vi.fn(),
		toggleCommandOverview: vi.fn(),
		toggleComposePanel: vi.fn(),
		detachActivityDashboard: vi.fn(),
		toggleProcessManager: vi.fn(),
		toggleGenerators: vi.fn(),
		showRemoteQr: vi.fn(),
		refreshTerminal: vi.fn(),
		blockPrev: vi.fn(),
		blockNext: vi.fn(),
		blockFoldToggle: vi.fn(),
		blockSearchToggle: vi.fn(),
		runSmartPromptByCombo: vi.fn(() => false),
	};
}

describe("actionRegistry", () => {
	describe("getActionEntries", () => {
		let entries: ActionEntry[];

		beforeAll(() => {
			entries = getActionEntries(createMockHandlers());
		});

		it("returns action entries for all mapped actions", () => {
			expect(entries.length).toBeGreaterThan(20);
		});

		it("every entry has required fields", () => {
			for (const entry of entries) {
				expect(entry.id).toBeTruthy();
				expect(entry.label).toBeTruthy();
				expect(entry.category).toBeTruthy();
				expect(typeof entry.execute).toBe("function");
			}
		});

		it("includes common actions", () => {
			const ids = entries.map((e) => e.id);
			expect(ids).toContain("new-terminal");
			expect(ids).toContain("toggle-markdown");
			expect(ids).toContain("zoom-in");
			expect(ids).toContain("toggle-sidebar");
			expect(ids).toContain("command-palette");
			expect(ids).toContain("quick-branch-switch");
			expect(ids).toContain("toggle-focus-mode");
		});

		it("Progress action advertises its shortcut and opens the active repository", () => {
			progressStore.resetForTests();
			repositoriesStore.add({ path: "/repo/progress", displayName: "Progress Repo" });
			repositoriesStore.setActive("/repo/progress");
			const action = getActionEntries(createMockHandlers()).find((entry) => entry.id === "progress");
			expect(action?.keybinding).toBeTruthy();
			action?.execute();
			expect(progressStore.dialogVisible()).toBe(true);
			expect(progressStore.requestedProject()).toBe("/repo/progress");
			progressStore.resetForTests();
			repositoriesStore.remove("/repo/progress");
			repositoriesStore._testCancelPendingSave();
		});

		it("toggle-focus-mode executes toggleFocusMode handler", () => {
			const handlers = createMockHandlers();
			const localEntries = getActionEntries(handlers);
			const entry = localEntries.find((e) => e.id === "toggle-focus-mode");
			expect(entry).toBeDefined();
			entry?.execute();
			expect(handlers.toggleFocusMode).toHaveBeenCalledOnce();
		});

		it("quick-branch-switch has correct category", () => {
			const entry = entries.find((e) => e.id === "quick-branch-switch");
			expect(entry).toBeDefined();
			expect(entry?.label).toBe("Quick branch switch");
			expect(entry?.category).toBe("Git");
		});

		it("entries have correct categories", () => {
			const terminalEntry = entries.find((e) => e.id === "new-terminal");
			expect(terminalEntry?.category).toBe("Terminal");

			const panelEntry = entries.find((e) => e.id === "toggle-markdown");
			expect(panelEntry?.category).toBe("Panels");

			const gitEntry = entries.find((e) => e.id === "toggle-git-ops");
			expect(gitEntry?.category).toBe("Git");
		});

		it("entries include keybinding display strings", () => {
			const newTerminal = entries.find((e) => e.id === "new-terminal");
			expect(newTerminal?.keybinding).toBeTruthy();
		});

		it("does not include numbered tab/branch switching", () => {
			const ids = entries.map((e) => e.id);
			expect(ids).not.toContain("switch-tab-1");
			expect(ids).not.toContain("switch-branch-1");
		});

		it("ActionEntry.id accepts arbitrary strings (for dynamic entries)", () => {
			const entry: ActionEntry = {
				id: "switch-repo:/some/path",
				label: "My Repo",
				category: "Repository",
				keybinding: "",
				execute: vi.fn(),
			};
			expect(entry.id).toBe("switch-repo:/some/path");
		});

		it("close-terminal delegates to the shared close-tab-or-pane handler", () => {
			// The split-aware decision (active pane vs active terminal, cancel an
			// empty split pane) lives in closeActiveTabOrPane so every entry point
			// — keyboard, palette, native menu — behaves identically.
			const handlers = createMockHandlers();

			const entry = getActionEntries(handlers).find((e) => e.id === "close-terminal");
			expect(entry).toBeDefined();
			entry?.execute();

			expect(handlers.closeActiveTabOrPane).toHaveBeenCalled();
			expect(handlers.closeTerminal).not.toHaveBeenCalled();
		});

		it("hides the hands-free toggle on a desktop where dictation is off", () => {
			const entry = getActionEntries(createMockHandlers()).find((e) => e.id === "toggle-hands-free");
			expect(entry).toBeUndefined();
		});

		it("registers the hands-free toggle, unbound and labelled for what it will do", async () => {
			await enableDictation();
			const entry = getActionEntries(createMockHandlers()).find((e) => e.id === "toggle-hands-free");
			expect(entry).toBeDefined();
			// Nothing is armed in a fresh store, so the palette offers to start.
			expect(entry?.label).toBe("Start hands-free conversation");
			expect(entry?.category).toBe("Dictation");
			expect(entry?.keybinding).toBe("");
		});

		it("routes the hands-free palette entry through the shared toggle", async () => {
			await enableDictation();
			const spy = vi.spyOn(handsFree, "toggleHandsFreeConversation").mockResolvedValue(undefined);
			const entry = getActionEntries(createMockHandlers()).find((e) => e.id === "toggle-hands-free");
			entry?.execute();
			expect(spy).toHaveBeenCalledOnce();
			spy.mockRestore();
		});

		it("execute calls the corresponding handler", () => {
			const handlers = createMockHandlers();
			const handlerEntries = getActionEntries(handlers);

			const zoomIn = handlerEntries.find((e) => e.id === "zoom-in");
			zoomIn?.execute();
			expect(handlers.zoomIn).toHaveBeenCalled();
		});
	});

	describe("search palette commands (dynamic, in App.tsx)", () => {
		it("search commands have correct shape when added as dynamic entries", () => {
			// These entries are added dynamically in App.tsx, not in getActionEntries.
			// We verify the expected shape here as a contract test.
			const searchEntries: ActionEntry[] = [
				{ id: "search-terminals", label: "Search Terminals", category: "Search", keybinding: "", execute: vi.fn() },
				{ id: "search-files", label: "Search Files", category: "Search", keybinding: "", execute: vi.fn() },
				{
					id: "search-file-contents",
					label: "Search in File Contents",
					category: "Search",
					keybinding: "",
					execute: vi.fn(),
				},
			];
			for (const entry of searchEntries) {
				expect(entry.category).toBe("Search");
				expect(entry.keybinding).toBe("");
				expect(typeof entry.execute).toBe("function");
			}
			expect(searchEntries.map((e) => e.id)).toEqual(["search-terminals", "search-files", "search-file-contents"]);
		});
	});
});

// A discoverable palette entry is the dialog's public entry point.
it("does not strand scheduled runs without an Automations palette entry", () => {
	const entry = getActionEntries(createMockHandlers()).find((action) => action.id === "automations");
	expect(entry?.label).toBe("Automations");
	entry?.execute();
	expect(automationsUi.visible()).toBe(true);
	automationsUi.close();
});
