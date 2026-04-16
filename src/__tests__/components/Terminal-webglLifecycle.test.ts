import { describe, it, expect, vi } from "vitest";
import { WebglLifecycle, ATLAS_BASE_FONT, ATLAS_BASE_MIN_PAGES, ATLAS_BASE_MIN_INTERVAL_MS } from "../../components/Terminal/webglLifecycle";

/** Minimal mock matching the subset of Terminal used by WebglLifecycle */
function mockTerminal() {
  return {
    loadAddon: vi.fn(),
  };
}

/** Minimal mock matching WebglAddon shape */
function mockWebglAddon() {
  const listeners: Record<string, (() => void)[]> = {};
  return {
    dispose: vi.fn(),
    onContextLoss: vi.fn((cb: () => void) => { listeners["contextLoss"] = listeners["contextLoss"] || []; listeners["contextLoss"].push(cb); }),
    onAddTextureAtlasCanvas: vi.fn((cb: () => void) => { listeners["atlasCanvas"] = listeners["atlasCanvas"] || []; listeners["atlasCanvas"].push(cb); }),
    _fire(event: string) { (listeners[event] || []).forEach(cb => cb()); },
    _listeners: listeners,
  };
}

describe("WebglLifecycle", () => {
  describe("threshold calculation", () => {
    it("uses base values at base font size (14px)", () => {
      const wl = new WebglLifecycle(() => mockWebglAddon() as any);
      wl.updateThresholds(ATLAS_BASE_FONT);
      expect(wl.minPages).toBe(ATLAS_BASE_MIN_PAGES);
      expect(wl.minIntervalMs).toBe(ATLAS_BASE_MIN_INTERVAL_MS);
    });

    it("scales down thresholds for larger fonts", () => {
      const wl = new WebglLifecycle(() => mockWebglAddon() as any);
      // At 28px (2× base), ratio=2 → pages=round(3/2)=2, interval=round(30000/2)=15000
      wl.updateThresholds(28);
      expect(wl.minPages).toBe(2);
      expect(wl.minIntervalMs).toBe(15_000);
    });

    it("clamps minimum pages to 1 and interval to 5000ms", () => {
      const wl = new WebglLifecycle(() => mockWebglAddon() as any);
      // At 56px (4× base), ratio=4 → pages=round(3/4)=1, interval=round(30000/4)=7500
      wl.updateThresholds(56);
      expect(wl.minPages).toBe(1);
      expect(wl.minIntervalMs).toBe(7_500);

      // At very large font, interval clamps to 5000
      wl.updateThresholds(100);
      expect(wl.minPages).toBe(1);
      expect(wl.minIntervalMs).toBe(5_000);
    });

    it("ignores font sizes smaller than base (ratio clamped to 1)", () => {
      const wl = new WebglLifecycle(() => mockWebglAddon() as any);
      wl.updateThresholds(10);
      expect(wl.minPages).toBe(ATLAS_BASE_MIN_PAGES);
      expect(wl.minIntervalMs).toBe(ATLAS_BASE_MIN_INTERVAL_MS);
    });
  });

  describe("attach / dispose", () => {
    it("creates addon and loads it into the terminal", () => {
      const term = mockTerminal();
      const addon = mockWebglAddon();
      const wl = new WebglLifecycle(() => addon as any);
      wl.attach(term as any);
      expect(term.loadAddon).toHaveBeenCalledWith(addon);
      expect(wl.addon).toBe(addon);
    });

    it("returns undefined and sets addon to undefined if factory throws", () => {
      const term = mockTerminal();
      const wl = new WebglLifecycle(() => { throw new Error("no WebGL"); });
      wl.attach(term as any);
      expect(wl.addon).toBeUndefined();
    });

    it("dispose() disposes the current addon", () => {
      const addon = mockWebglAddon();
      const wl = new WebglLifecycle(() => addon as any);
      wl.attach(mockTerminal() as any);
      wl.dispose();
      expect(addon.dispose).toHaveBeenCalled();
      expect(wl.addon).toBeUndefined();
    });

    it("dispose() is safe to call without an addon", () => {
      const wl = new WebglLifecycle(() => mockWebglAddon() as any);
      expect(() => wl.dispose()).not.toThrow();
    });
  });

  describe("atlas page tracking", () => {
    it("does not rebuild before reaching page threshold", () => {
      const addon = mockWebglAddon();
      let rebuildCount = 0;
      const wl = new WebglLifecycle(() => addon as any);
      wl.onRebuild = () => { rebuildCount++; };
      wl.attach(mockTerminal() as any);
      wl.updateThresholds(ATLAS_BASE_FONT); // minPages=3

      // Fire 2 atlas page additions (below threshold of 3)
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      expect(rebuildCount).toBe(0);
    });

    it("triggers rebuild when page threshold reached and interval elapsed", () => {
      const addon = mockWebglAddon();
      let rebuildCount = 0;
      const wl = new WebglLifecycle(() => addon as any);
      wl.onRebuild = () => { rebuildCount++; };
      wl.attach(mockTerminal() as any);
      wl.updateThresholds(ATLAS_BASE_FONT); // minPages=3, minInterval=30000

      // Simulate enough time elapsed
      vi.spyOn(performance, "now").mockReturnValue(50_000);

      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      expect(rebuildCount).toBe(1);
    });

    it("does not rebuild if interval has not elapsed", () => {
      const addon = mockWebglAddon();
      let rebuildCount = 0;
      const wl = new WebglLifecycle(() => addon as any);
      wl.onRebuild = () => { rebuildCount++; };
      wl.attach(mockTerminal() as any);
      wl.updateThresholds(ATLAS_BASE_FONT);

      // First trigger at time=50000 — succeeds
      const nowSpy = vi.spyOn(performance, "now").mockReturnValue(50_000);
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      expect(rebuildCount).toBe(1);

      // Second trigger at time=51000 — only 1s later, should NOT rebuild
      nowSpy.mockReturnValue(51_000);
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      addon._fire("atlasCanvas");
      expect(rebuildCount).toBe(1);

      // Third trigger at time=90000 — 40s later, should rebuild
      nowSpy.mockReturnValue(90_000);
      // pages counter was reset to 0 after first rebuild, need 3 more
      addon._fire("atlasCanvas"); // already 3 from above that didn't trigger
      expect(rebuildCount).toBe(2);
    });
  });
});
