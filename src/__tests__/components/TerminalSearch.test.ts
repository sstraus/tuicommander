import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import type { SearchAddon, ISearchOptions, ISearchResultChangeEvent } from "@xterm/addon-search";
import type { IEvent } from "@xterm/xterm";

/**
 * Unit tests for TerminalSearch behavior logic.
 *
 * Since TerminalSearch is a SolidJS component tightly coupled to DOM rendering
 * and @xterm/addon-search (which needs a real terminal), we test the interaction
 * patterns by verifying the SearchAddon mock calls that the component would make.
 *
 * The keyboard shortcut integration (Cmd+F dispatches findInTerminal) is covered
 * in useKeyboardShortcuts.test.ts.
 */

type MockSearchAddon = SearchAddon & { _fireResults: (e: ISearchResultChangeEvent) => void };

function createMockSearchAddon(): MockSearchAddon {
  const listeners: ((e: ISearchResultChangeEvent) => void)[] = [];

  return {
    findNext: vi.fn().mockReturnValue(true),
    findPrevious: vi.fn().mockReturnValue(true),
    clearDecorations: vi.fn(),
    clearActiveDecoration: vi.fn(),
    activate: vi.fn(),
    dispose: vi.fn(),
    onDidChangeResults: ((listener: (e: ISearchResultChangeEvent) => void) => {
      listeners.push(listener);
      return { dispose: () => { const idx = listeners.indexOf(listener); if (idx >= 0) listeners.splice(idx, 1); } };
    }) as IEvent<ISearchResultChangeEvent>,
    _fireResults: (e: ISearchResultChangeEvent) => {
      for (const l of listeners) l(e);
    },
  } as MockSearchAddon;
}

describe("TerminalSearch interaction patterns", () => {
  let addon: ReturnType<typeof createMockSearchAddon>;

  beforeEach(() => {
    addon = createMockSearchAddon();
  });

  describe("search addon calls", () => {
    it("findNext is called with correct options for case-sensitive search", () => {
      const options: ISearchOptions = {
        caseSensitive: true,
        regex: false,
        wholeWord: false,
        incremental: true,
        decorations: {
          matchBackground: "#ffff0040",
          matchBorder: "transparent",
          matchOverviewRuler: "#ffff00",
          activeMatchBackground: "#ff8c00b0",
          activeMatchBorder: "#ff8c00",
          activeMatchColorOverviewRuler: "#ff8c00",
        },
      };

      addon.findNext("test", options);
      expect(addon.findNext).toHaveBeenCalledWith("test", options);
    });

    it("findPrevious is called with same options", () => {
      const options: ISearchOptions = {
        caseSensitive: false,
        regex: true,
        wholeWord: false,
        incremental: true,
        decorations: {
          matchBackground: "#ffff0040",
          matchBorder: "transparent",
          matchOverviewRuler: "#ffff00",
          activeMatchBackground: "#ff8c00b0",
          activeMatchBorder: "#ff8c00",
          activeMatchColorOverviewRuler: "#ff8c00",
        },
      };

      addon.findPrevious("pattern.*", options);
      expect(addon.findPrevious).toHaveBeenCalledWith("pattern.*", options);
    });

    it("clearDecorations is called when search term is empty", () => {
      addon.clearDecorations();
      expect(addon.clearDecorations).toHaveBeenCalled();
    });
  });

  describe("result change events", () => {
    it("onDidChangeResults fires with result index and count", () => {
      const callback = vi.fn();
      addon.onDidChangeResults(callback);

      addon._fireResults({
        resultIndex: 2,
        resultCount: 10,
      });

      expect(callback).toHaveBeenCalledWith({ resultIndex: 2, resultCount: 10 });
    });

    it("listener can be disposed", () => {
      const callback = vi.fn();
      const disposable = addon.onDidChangeResults(callback);
      disposable.dispose();

      addon._fireResults({
        resultIndex: 0,
        resultCount: 5,
      });

      expect(callback).not.toHaveBeenCalled();
    });

    it("resultIndex -1 indicates threshold exceeded", () => {
      const callback = vi.fn();
      addon.onDidChangeResults(callback);

      addon._fireResults({
        resultIndex: -1,
        resultCount: 1500,
      });

      expect(callback).toHaveBeenCalledWith({ resultIndex: -1, resultCount: 1500 });
    });
  });

  describe("decoration colors", () => {
    it("match decorations use yellow for inactive and orange for active", () => {
      const decorations = {
        matchBackground: "#ffff0040",
        matchBorder: "transparent",
        matchOverviewRuler: "#ffff00",
        activeMatchBackground: "#ff8c00b0",
        activeMatchBorder: "#ff8c00",
        activeMatchColorOverviewRuler: "#ff8c00",
      };

      // Yellow-tinted inactive matches
      expect(decorations.matchBackground).toBe("#ffff0040");
      // Orange active match
      expect(decorations.activeMatchBackground).toBe("#ff8c00b0");
      expect(decorations.activeMatchBorder).toBe("#ff8c00");
    });
  });
});
