import { describe, it, expect, vi, beforeAll, afterAll } from "vitest";
import { installScrollbarVisibilityFix } from "../../components/Terminal/scrollbarFix";

// Happy-dom's MutationObserver.observe requires real DOM nodes. Stub it
// globally so our plain-object mocks work without full DOM setup.
const OriginalMO = globalThis.MutationObserver;
beforeAll(() => {
  globalThis.MutationObserver = class {
    observe() {}
    disconnect() {}
    takeRecords() { return []; }
  } as any;
});
afterAll(() => { globalThis.MutationObserver = OriginalMO; });

/** Minimal mock of xterm Terminal for scrollbar fix */
function mockTerminal(bufferLength: number, rows: number) {
  const disposable = () => ({ dispose: vi.fn() });
  return {
    buffer: { active: { length: bufferLength } },
    rows,
    onLineFeed: vi.fn(disposable),
    onScroll: vi.fn(disposable),
    onResize: vi.fn(disposable),
  };
}

/** Create a minimal container DOM mock with a scrollbar element */
function mockContainer(hasScrollbar = true) {
  const scrollbar = hasScrollbar
    ? {
        style: {
          setProperty: vi.fn(),
          removeProperty: vi.fn(),
        },
      }
    : null;

  return {
    querySelector: vi.fn(() => scrollbar),
    _scrollbar: scrollbar,
  };
}

describe("installScrollbarVisibilityFix", () => {
  it("returns a cleanup function", () => {
    const term = mockTerminal(100, 24);
    const container = mockContainer();
    const cleanup = installScrollbarVisibilityFix(term as any, container as any);
    expect(typeof cleanup).toBe("function");
  });

  it("returns noop cleanup when scrollbar element is missing", () => {
    const term = mockTerminal(100, 24);
    const container = mockContainer(false);
    const cleanup = installScrollbarVisibilityFix(term as any, container as any);
    expect(typeof cleanup).toBe("function");
    // Should not throw
    cleanup();
  });

  it("registers onLineFeed, onScroll, and onResize listeners", () => {
    const term = mockTerminal(100, 24);
    const container = mockContainer();
    installScrollbarVisibilityFix(term as any, container as any);
    expect(term.onLineFeed).toHaveBeenCalled();
    expect(term.onScroll).toHaveBeenCalled();
    expect(term.onResize).toHaveBeenCalled();
  });

  it("forces scrollbar visible when buffer overflows viewport", () => {
    const term = mockTerminal(100, 24); // 100 > 24 → overflow
    const container = mockContainer();
    installScrollbarVisibilityFix(term as any, container as any);
    // Initial update runs on install
    expect(container._scrollbar!.style.setProperty).toHaveBeenCalledWith("opacity", "1", "important");
    expect(container._scrollbar!.style.setProperty).toHaveBeenCalledWith("pointer-events", "auto", "important");
  });

  it("removes override when buffer fits in viewport", () => {
    const term = mockTerminal(20, 24); // 20 < 24 → no overflow
    const container = mockContainer();
    installScrollbarVisibilityFix(term as any, container as any);
    expect(container._scrollbar!.style.removeProperty).toHaveBeenCalledWith("opacity");
    expect(container._scrollbar!.style.removeProperty).toHaveBeenCalledWith("pointer-events");
  });
});
