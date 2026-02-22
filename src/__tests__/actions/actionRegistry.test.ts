import { describe, it, expect, vi, beforeAll } from "vitest";
import "../mocks/tauri";
import { getActionEntries, type ActionEntry } from "../../actions/actionRegistry";
import type { ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";

function createMockHandlers(): ShortcutHandlers {
  return {
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    zoomReset: vi.fn(),
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
    lazygitAvailable: vi.fn().mockReturnValue(true),
    spawnLazygit: vi.fn(),
    openLazygitPane: vi.fn(),
    toggleDiffPanel: vi.fn(),
    toggleMarkdownPanel: vi.fn(),
    toggleSidebar: vi.fn(),
    togglePromptLibrary: vi.fn(),
    toggleSettings: vi.fn(),
    toggleTaskQueue: vi.fn(),
    toggleGitOpsPanel: vi.fn(),
    toggleHelpPanel: vi.fn(),
    toggleNotesPanel: vi.fn(),
    toggleFileBrowserPanel: vi.fn(),
    findInTerminal: vi.fn(),
    toggleCommandPalette: vi.fn(),
    toggleActivityDashboard: vi.fn(),
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
      expect(ids).toContain("toggle-diff");
      expect(ids).toContain("zoom-in");
      expect(ids).toContain("toggle-sidebar");
      expect(ids).toContain("command-palette");
    });

    it("entries have correct categories", () => {
      const terminalEntry = entries.find((e) => e.id === "new-terminal");
      expect(terminalEntry?.category).toBe("Terminal");

      const panelEntry = entries.find((e) => e.id === "toggle-diff");
      expect(panelEntry?.category).toBe("Panels");

      const gitEntry = entries.find((e) => e.id === "open-lazygit");
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

  describe("lazygit guard", () => {
    it("does not call spawnLazygit when lazygitAvailable returns false", () => {
      const handlers = createMockHandlers();
      handlers.lazygitAvailable = vi.fn().mockReturnValue(false);
      const testEntries = getActionEntries(handlers);

      const entry = testEntries.find((e) => e.id === "open-lazygit");
      entry?.execute();

      expect(handlers.spawnLazygit).not.toHaveBeenCalled();
    });

    it("calls spawnLazygit when lazygitAvailable returns true", () => {
      const handlers = createMockHandlers();
      handlers.lazygitAvailable = vi.fn().mockReturnValue(true);
      const testEntries = getActionEntries(handlers);

      const entry = testEntries.find((e) => e.id === "open-lazygit");
      entry?.execute();

      expect(handlers.spawnLazygit).toHaveBeenCalled();
    });
  });
});
