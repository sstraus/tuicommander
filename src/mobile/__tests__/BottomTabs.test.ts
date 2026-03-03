import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const css = readFileSync(
  resolve(__dirname, "../components/BottomTabs.module.css"),
  "utf-8",
);

describe("BottomTabs frosted glass CSS", () => {
  it("background is semi-transparent (alpha < 1)", () => {
    const match = css.match(/background:\s*rgba\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*,\s*([\d.]+)\s*\)/);
    expect(match, "rgba background not found in BottomTabs.module.css").toBeTruthy();
    const alpha = parseFloat(match![1]);
    expect(alpha).toBeLessThan(1);
  });

  it("backdrop-filter blur is >= 10px", () => {
    const match = css.match(/backdrop-filter:[^;]*blur\(\s*(\d+)px\s*\)/);
    expect(match, "backdrop-filter blur not found in BottomTabs.module.css").toBeTruthy();
    const blurPx = parseInt(match![1], 10);
    expect(blurPx).toBeGreaterThanOrEqual(10);
  });

  it("safe-area-inset-bottom uses max() fallback pattern", () => {
    expect(css).toMatch(/max\(\s*\d+px\s*,\s*env\(safe-area-inset-bottom[^)]*\)\s*\)/);
  });
});
