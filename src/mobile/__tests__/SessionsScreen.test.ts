import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const source = readFileSync(
  resolve(__dirname, "../screens/SessionsScreen.tsx"),
  "utf-8",
);

describe("SessionsScreen refresh spinner", () => {
  it("gates spinner on refreshing AND sessions.length > 0", () => {
    // The spinner must only show when there are already loaded sessions,
    // otherwise the skeleton handles the empty-loading state.
    expect(source).toMatch(/props\.refreshing\s*&&\s*props\.sessions\.length\s*>\s*0/);
  });

  it("has a pull-to-refresh threshold constant", () => {
    // PULL_THRESHOLD controls how far user must drag before release triggers refresh.
    expect(source).toMatch(/PULL_THRESHOLD\s*=\s*\d+/);
  });

  it("triggers onRefresh when pull distance exceeds threshold", () => {
    expect(source).toMatch(/pullY\(\)\s*>=\s*PULL_THRESHOLD/);
    expect(source).toContain("props.onRefresh()");
  });
});
