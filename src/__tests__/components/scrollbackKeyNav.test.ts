/**
 * Test: keyboard navigation logic for scrollback overlay.
 *
 * Validates the decision logic that routes keyboard events to the overlay
 * vs xterm when the scrollback overlay is visible.
 */
import { describe, it, expect } from "vitest";

type KeyNavAction =
  | "close-overlay"       // Arrow Down: close overlay and return to terminal
  | "page-up"             // Page Up: scroll overlay up by one page
  | "page-down"           // Page Down: scroll overlay down by one page
  | "close-search"        // Escape: close VtLog search bar only
  | "close-overlay-esc"   // Escape: close overlay (search already closed)
  | "passthrough";        // Let xterm handle the key normally

/**
 * Replicates the keyboard routing logic from Terminal.tsx attachCustomKeyEventHandler.
 * Only handles keys relevant to the scrollback overlay.
 */
function evaluateKeyNav(opts: {
  key: string;
  type: "keydown" | "keyup" | "keypress";
  scrollbackVisible: boolean;
  vtLogSearchVisible: boolean;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}): KeyNavAction | "passthrough" {
  if (opts.type !== "keydown") return "passthrough";

  const noModifiers = !opts.shiftKey && !opts.ctrlKey && !opts.metaKey && !opts.altKey;

  // Scrollback overlay keyboard navigation
  if (opts.scrollbackVisible) {
    // Arrow Down: close overlay immediately
    if (opts.key === "ArrowDown" && noModifiers) return "close-overlay";

    // Page Up: scroll overlay up by one page
    if (opts.key === "PageUp" && noModifiers) return "page-up";

    // Page Down: scroll overlay down by one page
    if (opts.key === "PageDown" && noModifiers) return "page-down";

    // Escape: close search if open, otherwise close overlay
    if (opts.key === "Escape") {
      if (opts.vtLogSearchVisible) return "close-search";
      return "close-overlay-esc";
    }
  }

  return "passthrough";
}

describe("scrollback overlay keyboard navigation", () => {
  describe("Arrow Down", () => {
    it("closes overlay on Arrow Down (no modifiers)", () => {
      expect(evaluateKeyNav({
        key: "ArrowDown",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("close-overlay");
    });

    it("passes through Arrow Down when overlay is not visible", () => {
      expect(evaluateKeyNav({
        key: "ArrowDown",
        type: "keydown",
        scrollbackVisible: false,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });

    it("passes through Arrow Down with modifiers (Cmd+Down = block nav)", () => {
      expect(evaluateKeyNav({
        key: "ArrowDown",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
        metaKey: true,
      })).toBe("passthrough");
    });

    it("ignores keyup events", () => {
      expect(evaluateKeyNav({
        key: "ArrowDown",
        type: "keyup",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });
  });

  describe("Page Up / Page Down", () => {
    it("scrolls overlay up on Page Up", () => {
      expect(evaluateKeyNav({
        key: "PageUp",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("page-up");
    });

    it("scrolls overlay down on Page Down", () => {
      expect(evaluateKeyNav({
        key: "PageDown",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("page-down");
    });

    it("passes through Page Up when overlay not visible", () => {
      expect(evaluateKeyNav({
        key: "PageUp",
        type: "keydown",
        scrollbackVisible: false,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });

    it("passes through Page Down when overlay not visible", () => {
      expect(evaluateKeyNav({
        key: "PageDown",
        type: "keydown",
        scrollbackVisible: false,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });

    it("works during search too", () => {
      expect(evaluateKeyNav({
        key: "PageUp",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: true,
      })).toBe("page-up");
    });
  });

  describe("Escape", () => {
    it("closes search bar only when search is visible (first Escape)", () => {
      expect(evaluateKeyNav({
        key: "Escape",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: true,
      })).toBe("close-search");
    });

    it("closes overlay when search is not visible (second Escape)", () => {
      expect(evaluateKeyNav({
        key: "Escape",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("close-overlay-esc");
    });

    it("passes through Escape when overlay not visible", () => {
      expect(evaluateKeyNav({
        key: "Escape",
        type: "keydown",
        scrollbackVisible: false,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });
  });

  describe("other keys", () => {
    it("passes through regular keys even when overlay is visible", () => {
      expect(evaluateKeyNav({
        key: "a",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });

    it("passes through Arrow Up when overlay is visible", () => {
      // Arrow Up should NOT close the overlay — only Arrow Down does
      expect(evaluateKeyNav({
        key: "ArrowUp",
        type: "keydown",
        scrollbackVisible: true,
        vtLogSearchVisible: false,
      })).toBe("passthrough");
    });
  });
});
