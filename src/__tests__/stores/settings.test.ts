import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("settingsStore", () => {
  let store: typeof import("../../stores/settings").settingsStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    mockInvoke.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/settings")).settingsStore;
  });

  describe("defaults", () => {
    it("has correct default values", () => {
      testInScope(() => {
        expect(store.state.ide).toBe("vscode");
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.defaultFontSize).toBe(13);
        expect(store.state.confirmBeforeQuit).toBe(true);
        expect(store.state.confirmBeforeClosingTab).toBe(true);
        expect(store.state.splitTabMode).toBe("separate");
      });
    });
  });

  describe("setIde()", () => {
    it("updates IDE preference in state", () => {
      testInScope(() => {
        store.setIde("cursor");
        expect(store.state.ide).toBe("cursor");
      });
    });

    it("rolls back IDE on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setIde("cursor");
        expect(store.state.ide).toBe("vscode");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });

    it("persists IDE to Rust config via invoke", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "JetBrains Mono",
        font_size: 14,
        theme: "tokyo-night",

        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setIde("cursor");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ ide: "cursor" }),
        });
      });
    });
  });

  describe("setFont()", () => {
    it("updates font in store state", () => {
      testInScope(() => {
        store.setFont("Fira Code");
        expect(store.state.font).toBe("Fira Code");
      });
    });

    it("rolls back font on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setFont("Fira Code");
        expect(store.state.font).toBe("JetBrains Mono");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });

    it("persists font to Rust config via invoke", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "JetBrains Mono",
        font_size: 14,
        theme: "tokyo-night",

        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setFont("Fira Code");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ font_family: "Fira Code" }),
        });
      });
    });
  });

  describe("getFontFamily()", () => {
    it("returns CSS font family string", () => {
      testInScope(() => {
        const family = store.getFontFamily();
        expect(family).toContain("JetBrains");
        expect(family).toContain("monospace");
      });
    });
  });

  describe("getIdeName()", () => {
    it("returns display name for IDE", () => {
      testInScope(() => {
        expect(store.getIdeName()).toBe("VS Code");
        store.setIde("zed");
        expect(store.getIdeName()).toBe("Zed");
      });
    });
  });

  describe("loadFontFromConfig()", () => {
    it("loads font from Rust config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "Hack",
        font_size: 14,
        theme: "tokyo-night",

        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });

      await testInScopeAsync(async () => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("Hack");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
      });
    });

    it("falls back to default for invalid font in config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "Comic Sans",
        font_size: 14,
        theme: "tokyo-night",

        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });

      await testInScopeAsync(async () => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("JetBrains Mono");
      });
    });

    it("keeps default on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await testInScopeAsync(async () => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("JetBrains Mono");
      });
    });
  });

  describe("hydrate()", () => {
    it("loads settings from Rust config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "Hack",
        font_size: 14,
        theme: "tokyo-night",

        mcp_server_enabled: false,
        ide: "zed",
        default_font_size: 16,
      });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.font).toBe("Hack");
        expect(store.state.ide).toBe("zed");
        expect(store.state.defaultFontSize).toBe(16);
      });
    });

    it("migrates legacy IDE from localStorage", async () => {
      localStorage.setItem("tui-commander-default-ide", "cursor");
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      }); // load_config for migration
      mockInvoke.mockResolvedValueOnce(undefined); // save_config for migration
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "cursor", default_font_size: 12,
      }); // load_config after migration

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-default-ide")).toBeNull();
      });
    });

    it("falls back to defaults for invalid values from config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "Comic Sans", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "invalid-ide", default_font_size: 12,
      });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.ide).toBe("vscode");
      });
    });

    it("keeps defaults on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.ide).toBe("vscode");
      });
    });
  });

  describe("setDefaultFontSize()", () => {
    it("clamps font size to valid range", () => {
      testInScope(() => {
        store.setDefaultFontSize(5);
        expect(store.state.defaultFontSize).toBe(8);
        store.setDefaultFontSize(50);
        expect(store.state.defaultFontSize).toBe(32);
        store.setDefaultFontSize(16);
        expect(store.state.defaultFontSize).toBe(16);
      });
    });
  });

  describe("setShell()", () => {
    it("sets custom shell and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setShell("/bin/zsh");
        expect(store.state.shell).toBe("/bin/zsh");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ shell: "/bin/zsh" }),
        });
      });
    });

    it("trims whitespace and sets null for empty string", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setShell("  ");
        expect(store.state.shell).toBeNull();
      });
    });

    it("rolls back shell on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setShell("/bin/fish");
        expect(store.state.shell).toBeNull();
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("setTheme()", () => {
    it("sets theme and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setTheme("dracula");
        expect(store.state.theme).toBe("dracula");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ theme: "dracula" }),
        });
      });
    });

    it("rolls back theme on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setTheme("nord");
        expect(store.state.theme).toBe("commander");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("setSplitTabMode()", () => {
    it("sets split tab mode and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, split_tab_mode: "separate",
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setSplitTabMode("unified");
        expect(store.state.splitTabMode).toBe("unified");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ split_tab_mode: "unified" }),
        });
      });
    });

    it("rolls back splitTabMode on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setSplitTabMode("unified");
        expect(store.state.splitTabMode).toBe("separate");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("setConfirmBeforeQuit()", () => {
    it("rolls back confirmBeforeQuit on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setConfirmBeforeQuit(false);
        expect(store.state.confirmBeforeQuit).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("setConfirmBeforeClosingTab()", () => {
    it("rolls back confirmBeforeClosingTab on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setConfirmBeforeClosingTab(false);
        expect(store.state.confirmBeforeClosingTab).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });
  });

  describe("autoShowPrPopover", () => {
    it("defaults to true", () => {
      testInScope(() => {
        expect(store.state.autoShowPrPopover).toBe(true);
      });
    });

    it("sets autoShowPrPopover and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, auto_show_pr_popover: true,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.setAutoShowPrPopover(false);
        expect(store.state.autoShowPrPopover).toBe(false);
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ auto_show_pr_popover: false }),
        });
      });
    });

    it("rolls back autoShowPrPopover on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.setAutoShowPrPopover(false);
        expect(store.state.autoShowPrPopover).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
      });
    });

    it("hydrates autoShowPrPopover from config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, auto_show_pr_popover: false,
      });
      mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.autoShowPrPopover).toBe(false);
      });
    });
  });
});
