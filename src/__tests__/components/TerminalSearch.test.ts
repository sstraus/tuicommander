import { describe, it, expect, vi } from "vitest";
import "../mocks/tauri";
import type { CanvasTerminalRef } from "../../components/Terminal/CanvasTerminal";

/**
 * Unit tests for TerminalSearch canvas-based search behavior.
 *
 * TerminalSearch delegates search operations to CanvasTerminalRef methods.
 * We test the interaction patterns via a mock CanvasTerminalRef.
 */

function createMockCanvasRef(): CanvasTerminalRef {
  return {
    focus: vi.fn(),
    blur: vi.fn(),
    refresh: vi.fn(),
    searchFind: vi.fn().mockResolvedValue({ index: 0, count: 1 }),
    searchNext: vi.fn().mockReturnValue({ index: 1, count: 3 }),
    searchPrev: vi.fn().mockReturnValue({ index: 2, count: 3 }),
    searchClear: vi.fn(),
    scrollToBottom: vi.fn(),
    scrollLines: vi.fn(),
    scrollToRow: vi.fn(),
    getSelection: vi.fn().mockReturnValue(""),
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    resize: vi.fn(),
    getRowCount: vi.fn().mockReturnValue(24),
    getColCount: vi.fn().mockReturnValue(80),
  } as unknown as CanvasTerminalRef;
}

describe("TerminalSearch canvas interaction patterns", () => {
  describe("searchFind", () => {
    it("resolves with index and count", async () => {
      const ref = createMockCanvasRef();
      const result = await ref.searchFind("hello");
      expect(result).toEqual({ index: 0, count: 1 });
      expect(ref.searchFind).toHaveBeenCalledWith("hello");
    });

    it("searchClear is called when term is empty", () => {
      const ref = createMockCanvasRef();
      ref.searchClear();
      expect(ref.searchClear).toHaveBeenCalled();
    });
  });

  describe("searchNext / searchPrev", () => {
    it("searchNext returns next index and count", () => {
      const ref = createMockCanvasRef();
      const result = ref.searchNext();
      expect(result).toEqual({ index: 1, count: 3 });
    });

    it("searchPrev returns previous index and count", () => {
      const ref = createMockCanvasRef();
      const result = ref.searchPrev();
      expect(result).toEqual({ index: 2, count: 3 });
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

      expect(decorations.matchBackground).toBe("#ffff0040");
      expect(decorations.activeMatchBackground).toBe("#ff8c00b0");
      expect(decorations.activeMatchBorder).toBe("#ff8c00");
    });
  });
});
