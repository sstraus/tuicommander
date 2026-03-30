import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { commandPaletteStore } from "../../stores/commandPalette";

describe("commandPaletteStore", () => {
  beforeEach(() => {
    commandPaletteStore.close();
    localStorage.clear();
  });

  it("starts closed with empty query", () => {
    expect(commandPaletteStore.state.isOpen).toBe(false);
    expect(commandPaletteStore.state.query).toBe("");
  });

  it("open() sets isOpen to true and clears query", () => {
    commandPaletteStore.setQuery("test");
    commandPaletteStore.open();
    expect(commandPaletteStore.state.isOpen).toBe(true);
    expect(commandPaletteStore.state.query).toBe("");
  });

  it("close() sets isOpen to false and clears query", () => {
    commandPaletteStore.open();
    commandPaletteStore.setQuery("test");
    commandPaletteStore.close();
    expect(commandPaletteStore.state.isOpen).toBe(false);
    expect(commandPaletteStore.state.query).toBe("");
  });

  it("toggle() opens when closed and closes when open", () => {
    commandPaletteStore.toggle();
    expect(commandPaletteStore.state.isOpen).toBe(true);
    commandPaletteStore.toggle();
    expect(commandPaletteStore.state.isOpen).toBe(false);
  });

  it("setQuery() updates the query", () => {
    commandPaletteStore.setQuery("zoom");
    expect(commandPaletteStore.state.query).toBe("zoom");
  });

  it("recordUsage() adds action to recent list", () => {
    commandPaletteStore.recordUsage("zoom-in");
    expect(commandPaletteStore.state.recentActions).toContain("zoom-in");
  });

  it("recordUsage() moves action to front if already present", () => {
    commandPaletteStore.recordUsage("zoom-in");
    commandPaletteStore.recordUsage("zoom-out");
    commandPaletteStore.recordUsage("zoom-in");
    expect(commandPaletteStore.state.recentActions[0]).toBe("zoom-in");
    expect(commandPaletteStore.state.recentActions[1]).toBe("zoom-out");
  });

  it("recordUsage() persists to localStorage", () => {
    commandPaletteStore.recordUsage("zoom-in");
    const stored = JSON.parse(localStorage.getItem("tui-commander-recent-actions") || "[]");
    expect(stored).toContain("zoom-in");
  });

  it("recordUsage() caps at 10 items", () => {
    for (let i = 0; i < 15; i++) {
      commandPaletteStore.recordUsage(`action-${i}`);
    }
    expect(commandPaletteStore.state.recentActions.length).toBeLessThanOrEqual(10);
  });

  describe("content search mode", () => {
    it("mode() returns 'content' when query starts with !", () => {
      commandPaletteStore.setQuery("!search");
      expect(commandPaletteStore.mode()).toBe("content");
    });

    it("mode() returns 'command' for normal queries", () => {
      commandPaletteStore.setQuery("zoom");
      expect(commandPaletteStore.mode()).toBe("command");
    });

    it("mode() returns 'command' for empty query", () => {
      commandPaletteStore.setQuery("");
      expect(commandPaletteStore.mode()).toBe("command");
    });

    it("contentQuery() strips ! prefix", () => {
      commandPaletteStore.setQuery("!findme");
      expect(commandPaletteStore.contentQuery()).toBe("findme");
    });

    it("contentQuery() returns empty for non-content queries", () => {
      commandPaletteStore.setQuery("zoom");
      expect(commandPaletteStore.contentQuery()).toBe("");
    });

    it("close() resets content state", () => {
      commandPaletteStore.open();
      commandPaletteStore.setQuery("!test");
      commandPaletteStore.close();
      expect(commandPaletteStore.state.contentResults).toEqual([]);
      expect(commandPaletteStore.state.contentSearching).toBe(false);
      expect(commandPaletteStore.state.contentError).toBeNull();
    });

    it("switching from ! to non-! clears content state", () => {
      commandPaletteStore.setQuery("!test");
      commandPaletteStore.setQuery("test");
      expect(commandPaletteStore.state.contentResults).toEqual([]);
      expect(commandPaletteStore.state.contentSearching).toBe(false);
    });
  });
});
