import { describe, it, expect } from "vitest";
import { findOrphanTerminals } from "../../utils/terminalOrphans";

describe("findOrphanTerminals", () => {
  it("returns empty when no terminals exist", () => {
    expect(findOrphanTerminals([], {})).toEqual([]);
  });

  it("returns empty when all terminals are associated with branches", () => {
    const terminalIds = ["term-1", "term-2", "term-3"];
    const branchTerminalMap = {
      main: ["term-1"],
      feature: ["term-2", "term-3"],
    };
    expect(findOrphanTerminals(terminalIds, branchTerminalMap)).toEqual([]);
  });

  it("returns orphan IDs when some terminals are not associated", () => {
    const terminalIds = ["term-1", "term-2", "term-3"];
    const branchTerminalMap = {
      main: ["term-1"],
    };
    expect(findOrphanTerminals(terminalIds, branchTerminalMap)).toEqual(["term-2", "term-3"]);
  });

  it("returns all terminals when no branches exist", () => {
    const terminalIds = ["term-1", "term-2"];
    expect(findOrphanTerminals(terminalIds, {})).toEqual(["term-1", "term-2"]);
  });

  it("returns all terminals when branches have empty terminal lists", () => {
    const terminalIds = ["term-1", "term-2"];
    const branchTerminalMap = {
      main: [],
      feature: [],
    };
    expect(findOrphanTerminals(terminalIds, branchTerminalMap)).toEqual(["term-1", "term-2"]);
  });

  it("handles terminal appearing in multiple branches", () => {
    const terminalIds = ["term-1", "term-2"];
    const branchTerminalMap = {
      main: ["term-1"],
      feature: ["term-1"],
    };
    expect(findOrphanTerminals(terminalIds, branchTerminalMap)).toEqual(["term-2"]);
  });
});
