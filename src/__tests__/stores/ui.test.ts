import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("uiStore", () => {
  let store: typeof import("../../stores/ui").uiStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/ui")).uiStore;
  });

  describe("sidebar", () => {
    it("defaults to visible", () => {
      testInScope(() => {
        expect(store.state.sidebarVisible).toBe(true);
      });
    });

    it("toggleSidebar toggles visibility", () => {
      testInScope(() => {
        store.toggleSidebar();
        expect(store.state.sidebarVisible).toBe(false);
        store.toggleSidebar();
        expect(store.state.sidebarVisible).toBe(true);
      });
    });

    it("persists sidebar state via invoke", () => {
      testInScope(() => {
        store.toggleSidebar();
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_visible: false }),
        });
      });
    });

    it("setSidebarVisible sets directly", () => {
      testInScope(() => {
        store.setSidebarVisible(false);
        expect(store.state.sidebarVisible).toBe(false);
      });
    });
  });

  describe("fileBrowserExternalRoot", () => {
    it("defaults to null", () => {
      testInScope(() => {
        expect(store.state.fileBrowserExternalRoot).toBeNull();
      });
    });

    it("setFileBrowserExternalRoot stores the given path", () => {
      testInScope(() => {
        store.setFileBrowserExternalRoot("/tmp/foo");
        expect(store.state.fileBrowserExternalRoot).toBe("/tmp/foo");
      });
    });

    it("setFileBrowserExternalRoot(null) clears it", () => {
      testInScope(() => {
        store.setFileBrowserExternalRoot("/tmp/foo");
        store.setFileBrowserExternalRoot(null);
        expect(store.state.fileBrowserExternalRoot).toBeNull();
      });
    });

    it("is ephemeral — does not persist via save_ui_prefs", () => {
      testInScope(() => {
        mockInvoke.mockClear();
        store.setFileBrowserExternalRoot("/tmp/foo");
        const persistCalls = mockInvoke.mock.calls.filter(
          (c) => c[0] === "save_ui_prefs",
        );
        expect(persistCalls).toHaveLength(0);
      });
    });
  });

  describe("hydrate()", () => {
    it("loads sidebar state from Rust backend", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false, sidebar_width: 280 });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(false);
        expect(store.state.sidebarWidth).toBe(280);
        expect(mockInvoke).toHaveBeenCalledWith("load_ui_prefs");
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-sidebar-visible", "false");
      localStorage.setItem("tui-commander-sidebar-width", "350");
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs migration
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false, sidebar_width: 350 }); // load_ui_prefs

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-visible")).toBeNull();
        expect(localStorage.getItem("tui-commander-sidebar-width")).toBeNull();
      });
    });

    it("keeps defaults on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(true);
        expect(store.state.sidebarWidth).toBe(300);
      });
    });

    it("migrates only sidebar-visible from localStorage", async () => {
      localStorage.setItem("tui-commander-sidebar-visible", "true");
      // No width key set
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs migration
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: true, sidebar_width: 300 }); // load_ui_prefs

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-visible")).toBeNull();
      });
    });

    it("clamps NaN width during migration", async () => {
      localStorage.setItem("tui-commander-sidebar-width", "not-a-number");
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs
      mockInvoke.mockResolvedValueOnce({ sidebar_width: 300 }); // load_ui_prefs

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-width")).toBeNull();
        // Should have used default 300 since NaN was parsed
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_width: 300 }),
        });
      });
    });

    it("handles null from load_ui_prefs", async () => {
      mockInvoke.mockResolvedValueOnce(null);

      await testInScopeAsync(async () => {
        await store.hydrate();
        // Should keep defaults
        expect(store.state.sidebarVisible).toBe(true);
        expect(store.state.sidebarWidth).toBe(300);
      });
    });

    it("handles partial loaded data (only visible)", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(false);
        expect(store.state.sidebarWidth).toBe(300); // unchanged
      });
    });

    it("handles partial loaded data (only width)", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_width: 400 });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(true); // unchanged
        expect(store.state.sidebarWidth).toBe(400);
      });
    });
  });

  describe("sidebar width", () => {
    it("defaults to 300", () => {
      testInScope(() => {
        expect(store.state.sidebarWidth).toBe(300);
      });
    });

    it("setSidebarWidth updates width", () => {
      testInScope(() => {
        store.setSidebarWidth(250);
        expect(store.state.sidebarWidth).toBe(250);
      });
    });

    it("setSidebarWidth clamps to min/max", () => {
      testInScope(() => {
        store.setSidebarWidth(100);
        expect(store.state.sidebarWidth).toBe(200);
        store.setSidebarWidth(600);
        expect(store.state.sidebarWidth).toBe(500);
      });
    });

    it("persists sidebar width via invoke", () => {
      testInScope(() => {
        store.setSidebarWidth(350);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_width: 350 }),
        });
      });
    });
  });

  describe("markdown panel", () => {
    it("toggleMarkdownPanel toggles", () => {
      testInScope(() => {
        store.toggleMarkdownPanel();
        expect(store.state.markdownPanelVisible).toBe(true);
      });
    });

    it("setMarkdownPanelVisible sets directly", () => {
      testInScope(() => {
        store.setMarkdownPanelVisible(true);
        expect(store.state.markdownPanelVisible).toBe(true);
      });
    });
  });

  describe("dropdowns", () => {
    it("toggleIdeDropdown sets activeDropdown to ide", () => {
      testInScope(() => {
        store.toggleIdeDropdown();
        expect(store.state.activeDropdown).toBe("ide");
      });
    });

    it("toggleFontDropdown replaces active dropdown", () => {
      testInScope(() => {
        store.toggleIdeDropdown();
        store.toggleFontDropdown();
        expect(store.state.activeDropdown).toBe("font");
      });
    });

    it("toggleAgentDropdown sets activeDropdown to agent", () => {
      testInScope(() => {
        store.toggleAgentDropdown();
        expect(store.state.activeDropdown).toBe("agent");
      });
    });

    it("toggling the same dropdown again closes it", () => {
      testInScope(() => {
        store.toggleIdeDropdown();
        store.toggleIdeDropdown();
        expect(store.state.activeDropdown).toBeNull();
      });
    });

    it("closeAllDropdowns sets activeDropdown to null", () => {
      testInScope(() => {
        store.toggleIdeDropdown();
        store.closeAllDropdowns();
        expect(store.state.activeDropdown).toBeNull();
      });
    });
  });

  describe("panel widths", () => {
    it("defaults to expected widths", () => {
      testInScope(() => {
        expect(store.state.markdownPanelWidth).toBe(400);
        expect(store.state.notesPanelWidth).toBe(350);
        expect(store.state.settingsNavWidth).toBe(180);
      });
    });

    it("setMarkdownPanelWidth updates and persists", () => {
      testInScope(() => {
        store.setMarkdownPanelWidth(450);
        expect(store.state.markdownPanelWidth).toBe(450);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ markdown_panel_width: 450 }),
        });
      });
    });

    it("setNotesPanelWidth updates and persists", () => {
      testInScope(() => {
        store.setNotesPanelWidth(300);
        expect(store.state.notesPanelWidth).toBe(300);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ notes_panel_width: 300 }),
        });
      });
    });

    it("setSettingsNavWidth updates state without persisting (persist on drag-end)", () => {
      testInScope(() => {
        store.setSettingsNavWidth(220);
        expect(store.state.settingsNavWidth).toBe(220);
        // setSettingsNavWidth no longer calls save_ui_prefs directly (IPC storm fix);
        // callers must call persistUIPrefs() explicitly after drag-end
      });
    });

    it("persistUIPrefs saves current state to backend", () => {
      testInScope(() => {
        store.setSettingsNavWidth(220);
        mockInvoke.mockClear();
        store.persistUIPrefs();
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ settings_nav_width: 220 }),
        });
      });
    });

    it("hydrate loads panel widths from backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        sidebar_visible: true,
        sidebar_width: 300,
        markdown_panel_width: 450,
        notes_panel_width: 320,
        settings_nav_width: 200,
      });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.markdownPanelWidth).toBe(450);
        expect(store.state.notesPanelWidth).toBe(320);
        expect(store.state.settingsNavWidth).toBe(200);
      });
    });

    it("hydrate keeps panel width defaults when not in loaded data", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: true });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.markdownPanelWidth).toBe(400);
        expect(store.state.notesPanelWidth).toBe(350);
        expect(store.state.settingsNavWidth).toBe(180);
      });
    });

    it("save_ui_prefs includes all panel widths", () => {
      testInScope(() => {
        store.setSidebarWidth(300);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({
            sidebar_visible: true,
            sidebar_width: 300,
            markdown_panel_width: 400,
            notes_panel_width: 350,
            settings_nav_width: 180,
          }),
        });
      });
    });
  });

  describe("AI Chat panel", () => {
    it("defaults to hidden", () => {
      testInScope(() => {
        expect(store.state.aiChatPanelVisible).toBe(false);
      });
    });

    it("toggleAiChatPanel toggles visibility", () => {
      testInScope(() => {
        store.toggleAiChatPanel();
        expect(store.state.aiChatPanelVisible).toBe(true);
        store.toggleAiChatPanel();
        expect(store.state.aiChatPanelVisible).toBe(false);
      });
    });

    it("setAiChatPanelVisible sets directly", () => {
      testInScope(() => {
        store.setAiChatPanelVisible(true);
        expect(store.state.aiChatPanelVisible).toBe(true);
      });
    });

    it("opening AI Chat closes other exclusive panels", () => {
      testInScope(() => {
        store.toggleMarkdownPanel();
        expect(store.state.markdownPanelVisible).toBe(true);
        store.toggleAiChatPanel();
        expect(store.state.aiChatPanelVisible).toBe(true);
        expect(store.state.markdownPanelVisible).toBe(false);
      });
    });

    it("opening another exclusive panel closes AI Chat", () => {
      testInScope(() => {
        store.toggleAiChatPanel();
        expect(store.state.aiChatPanelVisible).toBe(true);
        store.toggleGitPanel();
        expect(store.state.gitPanelVisible).toBe(true);
        expect(store.state.aiChatPanelVisible).toBe(false);
      });
    });

    it("aiChatPanelWidth defaults to 500", () => {
      testInScope(() => {
        expect(store.state.aiChatPanelWidth).toBe(500);
      });
    });

    it("setAiChatPanelWidth updates and persists", () => {
      testInScope(() => {
        store.setAiChatPanelWidth(600);
        expect(store.state.aiChatPanelWidth).toBe(600);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ ai_chat_panel_width: 600 }),
        });
      });
    });

    it("hydrate loads AI Chat panel width from backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        sidebar_visible: true,
        sidebar_width: 300,
        ai_chat_panel_width: 550,
      });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.aiChatPanelWidth).toBe(550);
      });
    });
  });

  describe("loading state", () => {
    it("setLoading sets loading and message", () => {
      testInScope(() => {
        store.setLoading(true, "Loading...");
        expect(store.state.isLoading).toBe(true);
        expect(store.state.loadingMessage).toBe("Loading...");
      });
    });

    it("setLoading clears message when no message provided", () => {
      testInScope(() => {
        store.setLoading(true, "Loading...");
        store.setLoading(false);
        expect(store.state.isLoading).toBe(false);
        expect(store.state.loadingMessage).toBe("");
      });
    });
  });
});
