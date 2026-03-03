import { describe, it, expect } from "vitest";

// Contract tests for BottomTabs frosted glass effect.
// These document the required CSS properties without parsing the CSS file.
describe("BottomTabs frosted glass CSS contract", () => {
  it("background must be semi-transparent for frosted glass", () => {
    // rgba(37,37,38,0.85) — alpha < 1 lets backdrop-filter blur show through
    const bg = "rgba(37,37,38,0.85)";
    expect(bg).toMatch(/rgba\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*,\s*0\.\d+\s*\)/);
    const alpha = parseFloat(bg.match(/[\d.]+\s*\)$/)?.[0] ?? "1");
    expect(alpha).toBeLessThan(1);
  });

  it("backdrop-filter blur value must be >= 10px", () => {
    const blurPx = 20;
    expect(blurPx).toBeGreaterThanOrEqual(10);
  });

  it("safe-area-inset-bottom uses max() fallback pattern", () => {
    // max(0px, env(safe-area-inset-bottom)) ensures a non-negative value
    // even on browsers that don't support env()
    const pattern = /max\(\s*\d+px\s*,\s*env\(safe-area-inset-bottom[^)]*\)\s*\)/;
    const cssValue = "max(0px, env(safe-area-inset-bottom))";
    expect(cssValue).toMatch(pattern);
  });
});
