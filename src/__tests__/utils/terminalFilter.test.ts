import { describe, it, expect } from "vitest";
import { filterValidTerminals } from "../../utils/terminalFilter";

describe("filterValidTerminals", () => {
  it("returns empty when branch has no terminals", () => {
    expect(filterValidTerminals([], ["term-1", "term-2"])).toEqual([]);
  });

  it("returns empty when branch terminals is undefined", () => {
    expect(filterValidTerminals(undefined, ["term-1"])).toEqual([]);
  });

  it("returns empty when no existing terminal IDs are provided", () => {
    expect(filterValidTerminals(["term-1"], [])).toEqual([]);
  });

  it("returns only terminals that exist in the terminal store", () => {
    const branchTerminals = ["term-1", "term-2", "term-3"];
    const existingIds = ["term-1", "term-3", "term-5"];
    expect(filterValidTerminals(branchTerminals, existingIds)).toEqual(["term-1", "term-3"]);
  });

  it("returns all terminals when all exist", () => {
    const branchTerminals = ["term-1", "term-2"];
    const existingIds = ["term-1", "term-2", "term-3"];
    expect(filterValidTerminals(branchTerminals, existingIds)).toEqual(["term-1", "term-2"]);
  });

  it("returns empty when none of the branch terminals exist", () => {
    const branchTerminals = ["term-1", "term-2"];
    const existingIds = ["term-3", "term-4"];
    expect(filterValidTerminals(branchTerminals, existingIds)).toEqual([]);
  });

  it("preserves order of branch terminals", () => {
    const branchTerminals = ["term-3", "term-1", "term-2"];
    const existingIds = ["term-1", "term-2", "term-3"];
    expect(filterValidTerminals(branchTerminals, existingIds)).toEqual(["term-3", "term-1", "term-2"]);
  });
});
