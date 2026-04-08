import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("settingsStore", () => {
  let store: typeof import("../../stores/settings").settingsStore;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.resetModules();
    localStorage.clear();
    mockInvoke.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/settings")).settingsStore;
  });

  afterEach(() => {
    vi.useRealTimers();
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

    it("persists IDE via debounced save_config", async () => {
      await testInScopeAsync(async () => {
        store.setIde("cursor");
        // Not yet saved (debounced)
        expect(mockInvoke).not.toHaveBeenCalledWith("save_config", expect.anything());
        // Advance past debounce
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ ide: "cursor" }),
        });
      });
    });

    it("coalesces rapid changes into single save", async () => {
      await testInScopeAsync(async () => {
        store.setIde("cursor");
        store.setIde("zed");
        store.setIde("windsurf");
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        // Only one save_config call with the final value
        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_config"
        );
        expect(saveCalls).toHaveLength(1);
        expect(saveCalls[0][1].config.ide).toBe("windsurf");
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

    it("persists font via debounced save_config", async () => {
      await testInScopeAsync(async () => {
        store.setFont("Fira Code");
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
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
    it("applies font from hydrated config cache", async () => {
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
        await store.hydrate();
        // Change font locally
        store.setFont("Fira Code");
        expect(store.state.font).toBe("Fira Code");
        // Re-apply from cache (no IPC)
        store.loadFontFromConfig();
        expect(store.state.font).toBe("Hack");
        // No extra load_config call — uses hydrate cache
        const loadCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "load_config"
        );
        expect(loadCalls).toHaveLength(1); // only from hydrate
      });
    });

    it("no-op before hydrate", () => {
      testInScope(() => {
        store.loadFontFromConfig();
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
    it("sets custom shell", () => {
      testInScope(() => {
        store.setShell("/bin/zsh");
        expect(store.state.shell).toBe("/bin/zsh");
      });
    });

    it("trims whitespace and sets null for empty string", () => {
      testInScope(() => {
        store.setShell("  ");
        expect(store.state.shell).toBeNull();
      });
    });

    it("persists shell via debounced save", async () => {
      await testInScopeAsync(async () => {
        store.setShell("/bin/zsh");
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ shell: "/bin/zsh" }),
        });
      });
    });
  });

  describe("setTheme()", () => {
    it("sets theme and persists via debounced save", async () => {
      await testInScopeAsync(async () => {
        store.setTheme("dracula");
        expect(store.state.theme).toBe("dracula");
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ theme: "dracula" }),
        });
      });
    });
  });

  describe("setSplitTabMode()", () => {
    it("sets split tab mode and persists via debounced save", async () => {
      await testInScopeAsync(async () => {
        store.setSplitTabMode("unified");
        expect(store.state.splitTabMode).toBe("unified");
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ split_tab_mode: "unified" }),
        });
      });
    });
  });

  describe("setConfirmBeforeQuit()", () => {
    it("updates state", () => {
      testInScope(() => {
        store.setConfirmBeforeQuit(false);
        expect(store.state.confirmBeforeQuit).toBe(false);
      });
    });
  });

  describe("setConfirmBeforeClosingTab()", () => {
    it("updates state", () => {
      testInScope(() => {
        store.setConfirmBeforeClosingTab(false);
        expect(store.state.confirmBeforeClosingTab).toBe(false);
      });
    });
  });

  describe("autoShowPrPopover", () => {
    it("defaults to true", () => {
      testInScope(() => {
        expect(store.state.autoShowPrPopover).toBe(true);
      });
    });

    it("sets autoShowPrPopover and persists via debounced save", async () => {
      await testInScopeAsync(async () => {
        store.setAutoShowPrPopover(false);
        expect(store.state.autoShowPrPopover).toBe(false);
        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();
        expect(mockInvoke).toHaveBeenCalledWith("save_config", {
          config: expect.objectContaining({ auto_show_pr_popover: false }),
        });
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
