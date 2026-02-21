import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

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
      createRoot((dispose) => {
        expect(store.state.ide).toBe("vscode");
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.defaultFontSize).toBe(12);
        expect(store.state.confirmBeforeQuit).toBe(true);
        expect(store.state.confirmBeforeClosingTab).toBe(true);
        expect(store.state.splitTabMode).toBe("separate");
        dispose();
      });
    });
  });

  describe("setIde()", () => {
    it("updates IDE preference in state", () => {
      createRoot((dispose) => {
        store.setIde("cursor");
        expect(store.state.ide).toBe("cursor");
        dispose();
      });
    });

    it("rolls back IDE on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setIde("cursor");
        expect(store.state.ide).toBe("vscode");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });

    it("persists IDE to Rust config via invoke", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "JetBrains Mono",
        font_size: 14,
        theme: "tokyo-night",
        worktree_dir: null,
        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setIde("cursor");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ ide: "cursor" }),
        });
        dispose();
      });
    });
  });

  describe("setFont()", () => {
    it("updates font in store state", () => {
      createRoot((dispose) => {
        store.setFont("Fira Code");
        expect(store.state.font).toBe("Fira Code");
        dispose();
      });
    });

    it("rolls back font on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setFont("Fira Code");
        expect(store.state.font).toBe("JetBrains Mono");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });

    it("persists font to Rust config via invoke", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "JetBrains Mono",
        font_size: 14,
        theme: "tokyo-night",
        worktree_dir: null,
        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setFont("Fira Code");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ font_family: "Fira Code" }),
        });
        dispose();
      });
    });
  });

  describe("getFontFamily()", () => {
    it("returns CSS font family string", () => {
      createRoot((dispose) => {
        const family = store.getFontFamily();
        expect(family).toContain("JetBrains");
        expect(family).toContain("monospace");
        dispose();
      });
    });
  });

  describe("getIdeName()", () => {
    it("returns display name for IDE", () => {
      createRoot((dispose) => {
        expect(store.getIdeName()).toBe("VS Code");
        store.setIde("zed");
        expect(store.getIdeName()).toBe("Zed");
        dispose();
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
        worktree_dir: null,
        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });

      await createRoot(async (dispose) => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("Hack");
        expect(mockInvoke).toHaveBeenCalledWith("load_config");
        dispose();
      });
    });

    it("falls back to default for invalid font in config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null,
        font_family: "Comic Sans",
        font_size: 14,
        theme: "tokyo-night",
        worktree_dir: null,
        mcp_server_enabled: false,
        ide: "vscode",
        default_font_size: 12,
      });

      await createRoot(async (dispose) => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("JetBrains Mono");
        dispose();
      });
    });

    it("keeps default on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await createRoot(async (dispose) => {
        await store.loadFontFromConfig();
        expect(store.state.font).toBe("JetBrains Mono");
        dispose();
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
        worktree_dir: null,
        mcp_server_enabled: false,
        ide: "zed",
        default_font_size: 16,
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.font).toBe("Hack");
        expect(store.state.ide).toBe("zed");
        expect(store.state.defaultFontSize).toBe(16);
        dispose();
      });
    });

    it("migrates legacy IDE from localStorage", async () => {
      localStorage.setItem("tui-commander-default-ide", "cursor");
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      }); // load_config for migration
      mockInvoke.mockResolvedValueOnce(undefined); // save_config for migration
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "cursor", default_font_size: 12,
      }); // load_config after migration

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-default-ide")).toBeNull();
        dispose();
      });
    });

    it("falls back to defaults for invalid values from config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "Comic Sans", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "invalid-ide", default_font_size: 12,
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.ide).toBe("vscode");
        dispose();
      });
    });

    it("keeps defaults on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("no backend"));

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.font).toBe("JetBrains Mono");
        expect(store.state.ide).toBe("vscode");
        dispose();
      });
    });
  });

  describe("setDefaultFontSize()", () => {
    it("clamps font size to valid range", () => {
      createRoot((dispose) => {
        store.setDefaultFontSize(5);
        expect(store.state.defaultFontSize).toBe(8);
        store.setDefaultFontSize(50);
        expect(store.state.defaultFontSize).toBe(32);
        store.setDefaultFontSize(16);
        expect(store.state.defaultFontSize).toBe(16);
        dispose();
      });
    });
  });

  describe("setShell()", () => {
    it("sets custom shell and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setShell("/bin/zsh");
        expect(store.state.shell).toBe("/bin/zsh");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ shell: "/bin/zsh" }),
        });
        dispose();
      });
    });

    it("trims whitespace and sets null for empty string", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setShell("  ");
        expect(store.state.shell).toBeNull();
        dispose();
      });
    });

    it("rolls back shell on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setShell("/bin/fish");
        expect(store.state.shell).toBeNull();
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setTheme()", () => {
    it("sets theme and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setTheme("dracula");
        expect(store.state.theme).toBe("dracula");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ theme: "dracula" }),
        });
        dispose();
      });
    });

    it("rolls back theme on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setTheme("nord");
        expect(store.state.theme).toBe("vscode-dark");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setSplitTabMode()", () => {
    it("sets split tab mode and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, split_tab_mode: "separate",
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setSplitTabMode("unified");
        expect(store.state.splitTabMode).toBe("unified");
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ split_tab_mode: "unified" }),
        });
        dispose();
      });
    });

    it("rolls back splitTabMode on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setSplitTabMode("unified");
        expect(store.state.splitTabMode).toBe("separate");
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setConfirmBeforeQuit()", () => {
    it("rolls back confirmBeforeQuit on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setConfirmBeforeQuit(false);
        expect(store.state.confirmBeforeQuit).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setConfirmBeforeClosingTab()", () => {
    it("rolls back confirmBeforeClosingTab on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setConfirmBeforeClosingTab(false);
        expect(store.state.confirmBeforeClosingTab).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("autoShowPrPopover", () => {
    it("defaults to true", () => {
      createRoot((dispose) => {
        expect(store.state.autoShowPrPopover).toBe(true);
        dispose();
      });
    });

    it("sets autoShowPrPopover and persists", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, auto_show_pr_popover: true,
      });
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.setAutoShowPrPopover(false);
        expect(store.state.autoShowPrPopover).toBe(false);
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ auto_show_pr_popover: false }),
        });
        dispose();
      });
    });

    it("rolls back autoShowPrPopover on persist failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("fail"));
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.setAutoShowPrPopover(false);
        expect(store.state.autoShowPrPopover).toBe(true);
        expect(errSpy).toHaveBeenCalled();
        errSpy.mockRestore();
        dispose();
      });
    });

    it("hydrates autoShowPrPopover from config", async () => {
      mockInvoke.mockResolvedValueOnce({
        shell: null, font_family: "JetBrains Mono", font_size: 14,
        theme: "tokyo-night", worktree_dir: null, mcp_server_enabled: false,
        ide: "vscode", default_font_size: 12, auto_show_pr_popover: false,
      });
      mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.autoShowPrPopover).toBe(false);
        dispose();
      });
    });
  });
});
