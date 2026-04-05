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

  it("finds case-insensitive matches across lines and reranks by BM25", () => {
    const results = searchTerminalBuffer(lines, "error", "term-1", "Shell");
    expect(results).toHaveLength(3);
    // All three lines match "error" with tf=1; BM25 length normalization
    // pushes the shortest line ("$ grep -r error src/", line 7) to the top.
    const indices = results.map((r) => r.lineIndex);
    expect(indices).toContain(3);
    expect(indices).toContain(7);
    expect(indices).toContain(8);
    expect(results[0].lineIndex).toBe(7);
  });

  it("returns correct match offsets", () => {
    const results = searchTerminalBuffer(lines, "npm", "term-1", "Shell");
    // Offsets must point to the matched substring regardless of rank order.
    for (const r of results) {
      expect(r.lineText.slice(r.matchStart, r.matchEnd).toLowerCase()).toBe("npm");
    }
    // Shortest matching line ("$ npm install", line 0) wins under BM25.
    expect(results[0].lineIndex).toBe(0);
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

  it("BM25 ranks the best-match line at the top for an ambiguous query", () => {
    // Ambiguous query: three lines contain "auth" but one line has it
    // multiple times AND is short — clearest BM25 winner. Without rerank,
    // scrollback order would return the first line (index 0) first.
    const scrollback = [
      "$ curl https://example.com/api/users?auth=x&limit=10&cursor=abc123",
      "HTTP/1.1 200 OK, content-type: application/json, body 14kb",
      "auth auth auth",
      "$ ls -la",
    ];
    const results = searchTerminalBuffer(scrollback, "auth", "term-1", "Shell");
    expect(results[0].lineIndex).toBe(2);
    expect(results[0].lineText).toBe("auth auth auth");
  });
});
