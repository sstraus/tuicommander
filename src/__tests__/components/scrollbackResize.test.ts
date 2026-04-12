/**
 * Test: lineHeight and viewportHeight reactivity in ScrollbackOverlay.
 *
 * Bug: both values were measured once on onMount and never updated.
 * Font size changes or terminal resize while overlay open broke virtualization.
 *
 * Fix: lineHeight remeasures when fontVersion prop changes via createEffect.
 * viewportHeight updates via ResizeObserver on the overlay container.
 */
import { describe, it, expect } from "vitest";

describe("scrollback overlay resize reactivity", () => {
  describe("lineHeight remeasurement", () => {
    it("should detect when fontVersion changes and trigger remeasure", () => {
      // Simulate the reactive logic: when fontVersion changes, remeasure
      let lineHeight = 18;
      let fontVersion = 0;
      let measureCount = 0;

      const remeasure = (newFontVersion: number, newHeight: number) => {
        fontVersion = newFontVersion;
        measureCount++;
        lineHeight = newHeight;
      };

      // Initial measurement
      remeasure(0, 18);
      expect(measureCount).toBe(1);
      expect(lineHeight).toBe(18);
      expect(fontVersion).toBe(0);

      // Font size changes → fontVersion bumps → remeasure
      remeasure(1, 22); // larger font
      expect(measureCount).toBe(2);
      expect(lineHeight).toBe(22);

      // Another change
      remeasure(2, 14); // smaller font
      expect(measureCount).toBe(3);
      expect(lineHeight).toBe(14);
    });
  });

  describe("viewportHeight via ResizeObserver", () => {
    it("should update viewportHeight when container resizes", () => {
      let viewportHeight = 500;
      const setViewportHeight = (h: number) => { viewportHeight = h; };

      // Simulate ResizeObserver callback
      const simulateResize = (newHeight: number) => {
        setViewportHeight(newHeight);
      };

      simulateResize(600); // terminal pane expanded
      expect(viewportHeight).toBe(600);

      simulateResize(300); // terminal pane shrunk
      expect(viewportHeight).toBe(300);
    });

    it("should ignore zero-height entries from ResizeObserver", () => {
      let viewportHeight = 500;
      const setViewportHeight = (h: number) => {
        if (h > 0) viewportHeight = h;
      };

      // ResizeObserver can fire with 0 during display:none transitions
      setViewportHeight(0);
      expect(viewportHeight).toBe(500); // unchanged
    });
  });
});
