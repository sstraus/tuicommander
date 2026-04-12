/**
 * Test: wheel handler logic for scrollback overlay activation.
 *
 * Validates the gate logic that decides when wheel-up should open the
 * scrollback overlay vs letting xterm handle the scroll natively.
 * Also verifies that when the overlay is visible, wheel events are
 * consumed (handler returns false to suppress xterm scroll).
 */
import { describe, it, expect } from "vitest";

/** Minimal buffer shape matching xterm's IBuffer */
interface MockBuffer {
  viewportY: number;
  length: number;
  type: "normal" | "alternate";
}

/** Minimal cache shape */
interface MockCache {
  total: number;
}

/**
 * Replicates the gate logic from Terminal.tsx handleWheelForOverlay.
 * Returns: "open" (open overlay), "suppress" (overlay already visible),
 * or "passthrough" (let xterm handle).
 */
function evaluateWheelGate(opts: {
  deltaY: number;
  scrollbackVisible: boolean;
  cache: MockCache | null;
  buffer: MockBuffer;
}): "open" | "suppress" | "passthrough" {
  // Overlay already visible → suppress xterm scroll
  if (opts.scrollbackVisible) return "suppress";
  // Only wheel-up triggers overlay
  if (opts.deltaY >= 0) return "passthrough";
  // No cache → passthrough
  if (!opts.cache) return "passthrough";
  // Gate 1: xterm at top
  const atTop = opts.buffer.viewportY === 0;
  // Gate 2: VtLog has more history
  const hasExtraHistory = opts.cache.total > opts.buffer.length;
  if (atTop && hasExtraHistory) return "open";
  return "passthrough";
}

describe("scrollback wheel handler gate logic", () => {
  const altBuffer: MockBuffer = { viewportY: 0, length: 24, type: "alternate" };
  const normalBufferAtTop: MockBuffer = { viewportY: 0, length: 1024, type: "normal" };
  const normalBufferScrolled: MockBuffer = { viewportY: 50, length: 1024, type: "normal" };

  describe("overlay activation", () => {
    it("opens overlay on wheel-up in alt-buffer when VtLog has history", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: { total: 500 },
        buffer: altBuffer,
      })).toBe("open");
    });

    it("opens overlay on wheel-up in normal buffer at top when VtLog has extra history", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: { total: 2000 },
        buffer: normalBufferAtTop,
      })).toBe("open");
    });
  });

  describe("passthrough (let xterm handle)", () => {
    it("passes through wheel-down", () => {
      expect(evaluateWheelGate({
        deltaY: 120,
        scrollbackVisible: false,
        cache: { total: 500 },
        buffer: altBuffer,
      })).toBe("passthrough");
    });

    it("passes through when no cache exists", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: null,
        buffer: altBuffer,
      })).toBe("passthrough");
    });

    it("passes through when xterm has scrollback remaining (not at top)", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: { total: 2000 },
        buffer: normalBufferScrolled,
      })).toBe("passthrough");
    });

    it("passes through when VtLog has no extra history", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: { total: 24 },
        buffer: altBuffer,
      })).toBe("passthrough");
    });

    it("passes through on fresh session (VtLog total equals buffer length)", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: false,
        cache: { total: 1024 },
        buffer: normalBufferAtTop,
      })).toBe("passthrough");
    });
  });

  describe("suppress (overlay already visible)", () => {
    it("suppresses xterm scroll when overlay is visible", () => {
      expect(evaluateWheelGate({
        deltaY: -120,
        scrollbackVisible: true,
        cache: { total: 500 },
        buffer: altBuffer,
      })).toBe("suppress");
    });

    it("suppresses wheel-down too when overlay is visible", () => {
      expect(evaluateWheelGate({
        deltaY: 120,
        scrollbackVisible: true,
        cache: { total: 500 },
        buffer: altBuffer,
      })).toBe("suppress");
    });
  });
});
