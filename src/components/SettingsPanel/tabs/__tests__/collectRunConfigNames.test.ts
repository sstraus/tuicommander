import { describe, it, expect } from "vitest";
import { collectRunConfigNames } from "../AgentsTab";

describe("collectRunConfigNames", () => {
  it("lowercases names across all agent groups", () => {
    const set = collectRunConfigNames([
      [{ name: "Default" }, { name: "Fast" }],
      [{ name: "EXPERIMENT" }],
    ]);
    expect(set).toEqual(new Set(["default", "fast", "experiment"]));
  });

  it("returns an empty set when there are no configs", () => {
    expect(collectRunConfigNames([])).toEqual(new Set());
    expect(collectRunConfigNames([[], []])).toEqual(new Set());
  });

  it("without excludeName, all names are present (add-form behaviour)", () => {
    const set = collectRunConfigNames([[{ name: "Alpha" }, { name: "Beta" }]]);
    expect(set.has("alpha")).toBe(true);
    expect(set.has("beta")).toBe(true);
  });

  it("with excludeName, drops that single entry from the duplicate set (rename-form behaviour)", () => {
    // The rename form edits a config whose previous name is "Alpha" — the user
    // typing "Alpha" again must NOT trigger a duplicate error for their own row.
    const set = collectRunConfigNames(
      [[{ name: "Alpha" }, { name: "Beta" }]],
      "Alpha",
    );
    expect(set.has("alpha")).toBe(false);
    expect(set.has("beta")).toBe(true);
  });

  it("excludeName is case-insensitive", () => {
    const set = collectRunConfigNames(
      [[{ name: "Alpha" }, { name: "Beta" }]],
      "alpha",
    );
    expect(set.has("alpha")).toBe(false);
  });

  it("excludeName removes only the first match — duplicates elsewhere remain flagged", () => {
    // If two stored configs share a name (shouldn't happen, but defensive),
    // excluding the one under edit must still surface the other as duplicate.
    const set = collectRunConfigNames(
      [[{ name: "Alpha" }], [{ name: "alpha" }]],
      "Alpha",
    );
    expect(set.has("alpha")).toBe(true);
  });
});
