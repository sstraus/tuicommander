import { describe, it, expect } from "vitest";
import { searchTerminalBuffer } from "../../utils/terminalSearch";

describe("searchTerminalBuffer", () => {
  const lines = [
    "$ npm install",
    "added 1234 packages",
    "npm warn deprecated some-package@1.0.0",
    "npm error ERESOLVE unable to resolve dependency tree",
    "$ echo hello",
    "hello",
    "",
    "$ grep -r error src/",
    "src/app.ts:42: throw new Error('something failed')",
  ];

  it("finds case-insensitive matches across lines", () => {
    const results = searchTerminalBuffer(lines, "error", "term-1", "Shell");
    expect(results).toHaveLength(3);
    expect(results[0].lineIndex).toBe(3);
    expect(results[0].lineText).toBe(lines[3]);
    expect(results[1].lineIndex).toBe(7);
    expect(results[2].lineIndex).toBe(8);
  });

  it("returns correct match offsets", () => {
    const results = searchTerminalBuffer(lines, "npm", "term-1", "Shell");
    // Line 0: "$ npm install" → match at 2..5
    expect(results[0].matchStart).toBe(2);
    expect(results[0].matchEnd).toBe(5);
    // Line 2: "npm warn..." → match at 0..3
    expect(results[1].matchStart).toBe(0);
    expect(results[1].matchEnd).toBe(3);
  });

  it("finds multiple matches on the same line (first match only per line)", () => {
    const results = searchTerminalBuffer(
      ["error: error happened"],
      "error",
      "term-1",
      "Shell",
    );
    // Only first match per line to avoid flooding results
    expect(results).toHaveLength(1);
    expect(results[0].matchStart).toBe(0);
    expect(results[0].matchEnd).toBe(5);
  });

  it("returns empty array when no matches", () => {
    const results = searchTerminalBuffer(lines, "nonexistent", "term-1", "Shell");
    expect(results).toEqual([]);
  });

  it("sets terminalId and terminalName on each match", () => {
    const results = searchTerminalBuffer(["test error"], "error", "term-5", "My Terminal");
    expect(results[0].terminalId).toBe("term-5");
    expect(results[0].terminalName).toBe("My Terminal");
  });

  it("is case-insensitive", () => {
    const results = searchTerminalBuffer(["ERROR: failed", "Error: oops"], "error", "t", "T");
    expect(results).toHaveLength(2);
  });

  it("handles empty lines array", () => {
    expect(searchTerminalBuffer([], "test", "t", "T")).toEqual([]);
  });

  it("handles empty query gracefully", () => {
    expect(searchTerminalBuffer(lines, "", "t", "T")).toEqual([]);
  });
});
