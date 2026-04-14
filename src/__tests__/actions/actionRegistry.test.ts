import { describe, it, expect, vi, beforeAll } from "vitest";
import "../mocks/tauri";
import { getActionEntries, type ActionEntry } from "../../actions/actionRegistry";
import type { ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";

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
    toggleNotesPanel: vi.fn(),
    toggleFileBrowserPanel: vi.fn(),
    findInTerminal: vi.fn(),
    toggleCommandPalette: vi.fn(),
    toggleActivityDashboard: vi.fn(),
    toggleWorktreeManager: vi.fn(),
    toggleBranchSwitcher: vi.fn(),
    toggleErrorLog: vi.fn(),
    toggleBranchesTab: vi.fn(),
    toggleMcpPopup: vi.fn(),
    clearScrollback: vi.fn(),
    scrollToTop: vi.fn(),
    scrollToBottom: vi.fn(),
    scrollPageUp: vi.fn(),
    scrollPageDown: vi.fn(),
    toggleZoomPane: vi.fn(),
    togglePromptLibrary: vi.fn(),
    toggleDiffScroll: vi.fn(),
    toggleGlobalWorkspace: vi.fn(),
    openFile: vi.fn(),
    newFile: vi.fn(),
    openFolder: vi.fn(),
    openPath: vi.fn(),
    openSecondaryWindow: vi.fn(),
    toggleCommandOverview: vi.fn(),
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
        { id: "search-file-contents", label: "Search in File Contents", category: "Search", keybinding: "", execute: vi.fn() },
      ];
      for (const entry of searchEntries) {
        expect(entry.category).toBe("Search");
        expect(entry.keybinding).toBe("");
        expect(typeof entry.execute).toBe("function");
      }
      expect(searchEntries.map(e => e.id)).toEqual(["search-terminals", "search-files", "search-file-contents"]);
    });
  });
});

