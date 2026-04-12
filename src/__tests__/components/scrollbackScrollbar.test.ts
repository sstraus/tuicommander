/**
 * Test: custom scrollbar geometry calculations for scrollback overlay.
 *
 * Validates thumb height, thumb position, and click-on-track target
 * scrollTop computation.
 */
import { describe, it, expect } from "vitest";

const MIN_THUMB_HEIGHT = 30; // px

/** Compute the thumb height in pixels. */
function computeThumbHeight(opts: {
  viewportHeight: number;
  totalContentHeight: number;
  trackHeight: number;
}): number {
  if (opts.totalContentHeight <= opts.viewportHeight) return opts.trackHeight;
  const ratio = opts.viewportHeight / opts.totalContentHeight;
  return Math.max(MIN_THUMB_HEIGHT, Math.round(ratio * opts.trackHeight));
}

/** Compute the thumb top position in pixels. */
function computeThumbTop(opts: {
  scrollTop: number;
  maxScrollTop: number;
  trackHeight: number;
  thumbHeight: number;
}): number {
  if (opts.maxScrollTop <= 0) return 0;
  const ratio = opts.scrollTop / opts.maxScrollTop;
  return Math.round(ratio * (opts.trackHeight - opts.thumbHeight));
}

/** Compute the scrollTop from a click position on the track. */
function computeScrollTopFromTrackClick(opts: {
  clickY: number;
  trackHeight: number;
  thumbHeight: number;
  maxScrollTop: number;
}): number {
  // Center the thumb on the click position
  const thumbCenter = opts.clickY;
  const thumbTop = thumbCenter - opts.thumbHeight / 2;
  const maxThumbTop = opts.trackHeight - opts.thumbHeight;
  const clampedThumbTop = Math.max(0, Math.min(thumbTop, maxThumbTop));
  if (maxThumbTop <= 0) return 0;
  return Math.round((clampedThumbTop / maxThumbTop) * opts.maxScrollTop);
}

describe("scrollback scrollbar geometry", () => {
  describe("computeThumbHeight", () => {
    it("returns trackHeight when content fits in viewport", () => {
      expect(computeThumbHeight({
        viewportHeight: 500,
        totalContentHeight: 400,
        trackHeight: 500,
      })).toBe(500);
    });

    it("is proportional to viewport/content ratio", () => {
      const h = computeThumbHeight({
        viewportHeight: 500,
        totalContentHeight: 5000,
        trackHeight: 500,
      });
      // ratio = 0.1, so thumb = 50px
      expect(h).toBe(50);
    });

    it("enforces minimum thumb height of 30px", () => {
      const h = computeThumbHeight({
        viewportHeight: 500,
        totalContentHeight: 1_800_000, // 100K lines * 18px
        trackHeight: 500,
      });
      expect(h).toBe(MIN_THUMB_HEIGHT);
    });

    it("handles equal viewport and content", () => {
      expect(computeThumbHeight({
        viewportHeight: 500,
        totalContentHeight: 500,
        trackHeight: 500,
      })).toBe(500);
    });
  });

  describe("computeThumbTop", () => {
    it("returns 0 when scrolled to top", () => {
      expect(computeThumbTop({
        scrollTop: 0,
        maxScrollTop: 10000,
        trackHeight: 500,
        thumbHeight: 50,
      })).toBe(0);
    });

    it("returns trackHeight - thumbHeight when scrolled to bottom", () => {
      expect(computeThumbTop({
        scrollTop: 10000,
        maxScrollTop: 10000,
        trackHeight: 500,
        thumbHeight: 50,
      })).toBe(450);
    });

    it("returns proportional position for mid-scroll", () => {
      const top = computeThumbTop({
        scrollTop: 5000,
        maxScrollTop: 10000,
        trackHeight: 500,
        thumbHeight: 50,
      });
      expect(top).toBe(225); // 0.5 * (500 - 50)
    });

    it("returns 0 when maxScrollTop is 0 (no scrolling possible)", () => {
      expect(computeThumbTop({
        scrollTop: 0,
        maxScrollTop: 0,
        trackHeight: 500,
        thumbHeight: 500,
      })).toBe(0);
    });
  });

  describe("computeScrollTopFromTrackClick", () => {
    it("clicking at top of track scrolls to top", () => {
      const st = computeScrollTopFromTrackClick({
        clickY: 0,
        trackHeight: 500,
        thumbHeight: 50,
        maxScrollTop: 10000,
      });
      expect(st).toBe(0);
    });

    it("clicking at bottom of track scrolls to bottom", () => {
      const st = computeScrollTopFromTrackClick({
        clickY: 500,
        trackHeight: 500,
        thumbHeight: 50,
        maxScrollTop: 10000,
      });
      expect(st).toBe(10000);
    });

    it("clicking at middle scrolls to ~50%", () => {
      const st = computeScrollTopFromTrackClick({
        clickY: 250,
        trackHeight: 500,
        thumbHeight: 50,
        maxScrollTop: 10000,
      });
      expect(st).toBe(5000);
    });

    it("returns 0 when maxScrollTop is 0", () => {
      expect(computeScrollTopFromTrackClick({
        clickY: 250,
        trackHeight: 500,
        thumbHeight: 500,
        maxScrollTop: 0,
      })).toBe(0);
    });
  });
});
