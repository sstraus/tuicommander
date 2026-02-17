import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

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
      createRoot((dispose) => {
        expect(store.state.sidebarVisible).toBe(true);
        dispose();
      });
    });

    it("toggleSidebar toggles visibility", () => {
      createRoot((dispose) => {
        store.toggleSidebar();
        expect(store.state.sidebarVisible).toBe(false);
        store.toggleSidebar();
        expect(store.state.sidebarVisible).toBe(true);
        dispose();
      });
    });

    it("persists sidebar state via invoke", () => {
      createRoot((dispose) => {
        store.toggleSidebar();
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_visible: false }),
        });
        dispose();
      });
    });

    it("setSidebarVisible sets directly", () => {
      createRoot((dispose) => {
        store.setSidebarVisible(false);
        expect(store.state.sidebarVisible).toBe(false);
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads sidebar state from Rust backend", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false, sidebar_width: 280 });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(false);
        expect(store.state.sidebarWidth).toBe(280);
        expect(mockInvoke).toHaveBeenCalledWith("load_ui_prefs");
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-sidebar-visible", "false");
      localStorage.setItem("tui-commander-sidebar-width", "350");
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs migration
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false, sidebar_width: 350 }); // load_ui_prefs

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-visible")).toBeNull();
        expect(localStorage.getItem("tui-commander-sidebar-width")).toBeNull();
        dispose();
      });
    });

    it("keeps defaults on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(true);
        expect(store.state.sidebarWidth).toBe(300);
        dispose();
      });
    });

    it("migrates only sidebar-visible from localStorage", async () => {
      localStorage.setItem("tui-commander-sidebar-visible", "true");
      // No width key set
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs migration
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: true, sidebar_width: 300 }); // load_ui_prefs

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-visible")).toBeNull();
        dispose();
      });
    });

    it("clamps NaN width during migration", async () => {
      localStorage.setItem("tui-commander-sidebar-width", "not-a-number");
      mockInvoke.mockResolvedValueOnce(undefined); // save_ui_prefs
      mockInvoke.mockResolvedValueOnce({ sidebar_width: 300 }); // load_ui_prefs

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-sidebar-width")).toBeNull();
        // Should have used default 300 since NaN was parsed
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_width: 300 }),
        });
        dispose();
      });
    });

    it("handles null from load_ui_prefs", async () => {
      mockInvoke.mockResolvedValueOnce(null);

      await createRoot(async (dispose) => {
        await store.hydrate();
        // Should keep defaults
        expect(store.state.sidebarVisible).toBe(true);
        expect(store.state.sidebarWidth).toBe(300);
        dispose();
      });
    });

    it("handles partial loaded data (only visible)", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_visible: false });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(false);
        expect(store.state.sidebarWidth).toBe(300); // unchanged
        dispose();
      });
    });

    it("handles partial loaded data (only width)", async () => {
      mockInvoke.mockResolvedValueOnce({ sidebar_width: 400 });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.sidebarVisible).toBe(true); // unchanged
        expect(store.state.sidebarWidth).toBe(400);
        dispose();
      });
    });
  });

  describe("sidebar width", () => {
    it("defaults to 300", () => {
      createRoot((dispose) => {
        expect(store.state.sidebarWidth).toBe(300);
        dispose();
      });
    });

    it("setSidebarWidth updates width", () => {
      createRoot((dispose) => {
        store.setSidebarWidth(250);
        expect(store.state.sidebarWidth).toBe(250);
        dispose();
      });
    });

    it("setSidebarWidth clamps to min/max", () => {
      createRoot((dispose) => {
        store.setSidebarWidth(100);
        expect(store.state.sidebarWidth).toBe(200);
        store.setSidebarWidth(600);
        expect(store.state.sidebarWidth).toBe(500);
        dispose();
      });
    });

    it("persists sidebar width via invoke", () => {
      createRoot((dispose) => {
        store.setSidebarWidth(350);
        expect(mockInvoke).toHaveBeenCalledWith("save_ui_prefs", {
          config: expect.objectContaining({ sidebar_width: 350 }),
        });
        dispose();
      });
    });
  });

  describe("diff panel", () => {
    it("defaults to hidden", () => {
      createRoot((dispose) => {
        expect(store.state.diffPanelVisible).toBe(false);
        dispose();
      });
    });

    it("toggleDiffPanel toggles", () => {
      createRoot((dispose) => {
        store.toggleDiffPanel();
        expect(store.state.diffPanelVisible).toBe(true);
        store.toggleDiffPanel();
        expect(store.state.diffPanelVisible).toBe(false);
        dispose();
      });
    });

    it("setDiffPanelVisible sets directly", () => {
      createRoot((dispose) => {
        store.setDiffPanelVisible(true);
        expect(store.state.diffPanelVisible).toBe(true);
        dispose();
      });
    });
  });

  describe("markdown panel", () => {
    it("toggleMarkdownPanel toggles", () => {
      createRoot((dispose) => {
        store.toggleMarkdownPanel();
        expect(store.state.markdownPanelVisible).toBe(true);
        dispose();
      });
    });

    it("setMarkdownPanelVisible sets directly", () => {
      createRoot((dispose) => {
        store.setMarkdownPanelVisible(true);
        expect(store.state.markdownPanelVisible).toBe(true);
        dispose();
      });
    });
  });

  describe("diff repo", () => {
    it("setCurrentDiffRepo sets repo path", () => {
      createRoot((dispose) => {
        store.setCurrentDiffRepo("/path/to/repo");
        expect(store.state.currentDiffRepo).toBe("/path/to/repo");
        dispose();
      });
    });
  });

  describe("dropdowns", () => {
    it("toggleIdeDropdown toggles and closes others", () => {
      createRoot((dispose) => {
        store.toggleIdeDropdown();
        expect(store.state.ideDropdownVisible).toBe(true);
        expect(store.state.fontDropdownVisible).toBe(false);
        expect(store.state.agentDropdownVisible).toBe(false);
        dispose();
      });
    });

    it("toggleFontDropdown toggles and closes others", () => {
      createRoot((dispose) => {
        store.toggleIdeDropdown();
        store.toggleFontDropdown();
        expect(store.state.fontDropdownVisible).toBe(true);
        expect(store.state.ideDropdownVisible).toBe(false);
        dispose();
      });
    });

    it("toggleAgentDropdown toggles and closes others", () => {
      createRoot((dispose) => {
        store.toggleAgentDropdown();
        expect(store.state.agentDropdownVisible).toBe(true);
        dispose();
      });
    });

    it("closeAllDropdowns closes all", () => {
      createRoot((dispose) => {
        store.toggleIdeDropdown();
        store.closeAllDropdowns();
        expect(store.state.ideDropdownVisible).toBe(false);
        expect(store.state.fontDropdownVisible).toBe(false);
        expect(store.state.agentDropdownVisible).toBe(false);
        dispose();
      });
    });
  });

  describe("loading state", () => {
    it("setLoading sets loading and message", () => {
      createRoot((dispose) => {
        store.setLoading(true, "Loading...");
        expect(store.state.isLoading).toBe(true);
        expect(store.state.loadingMessage).toBe("Loading...");
        dispose();
      });
    });

    it("setLoading clears message when no message provided", () => {
      createRoot((dispose) => {
        store.setLoading(true, "Loading...");
        store.setLoading(false);
        expect(store.state.isLoading).toBe(false);
        expect(store.state.loadingMessage).toBe("");
        dispose();
      });
    });
  });
});
