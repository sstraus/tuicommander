import { describe, it, expect } from "vitest";
import { buildIndex, tokenize } from "../../utils/bm25";

describe("tokenize", () => {
  it("lowercases and splits on non-alphanumeric", () => {
    expect(tokenize("New Terminal")).toEqual(["new", "terminal"]);
    expect(tokenize("Open-File_v2")).toEqual(["open", "file", "v2"]);
  });

  it("drops empty segments", () => {
    expect(tokenize("  hello   world  ")).toEqual(["hello", "world"]);
  });

  it("returns empty for blank input", () => {
    expect(tokenize("")).toEqual([]);
    expect(tokenize("!!!")).toEqual([]);
  });
});

describe("buildIndex", () => {
  const corpus = [
    { item: "new-terminal", text: "New Terminal" },
    { item: "terminal-settings", text: "Terminal Settings" },
    { item: "close-tab", text: "Close Tab" },
    { item: "open-file", text: "Open File" },
    { item: "open-recent-file", text: "Open Recent File" },
  ];

  it("returns empty for empty query", () => {
    const { score } = buildIndex(corpus);
    expect(score("")).toEqual([]);
    expect(score("   ")).toEqual([]);
  });

  it("prefix-matches vocab so 'term' hits 'terminal'", () => {
    const { score } = buildIndex(corpus);
    const results = score("term");
    const ids = results.map((r) => r.item);
    expect(ids).toContain("new-terminal");
    expect(ids).toContain("terminal-settings");
    expect(ids).not.toContain("close-tab");
  });

  it("ranks multi-term matches above single-term matches", () => {
    // "new term" has two query tokens: "new" matches only "New Terminal",
    // "term" matches both. Only "New Terminal" satisfies ALL terms, so it
    // must be the top (and only) result.
    const { score } = buildIndex(corpus);
    const results = score("new term");
    expect(results.length).toBeGreaterThan(0);
    expect(results[0].item).toBe("new-terminal");
    expect(results.map((r) => r.item)).not.toContain("terminal-settings");
  });

  it("requires every query term to match", () => {
    const { score } = buildIndex(corpus);
    // "open zzz" — "zzz" has no prefix match, so no results.
    expect(score("open zzz")).toEqual([]);
  });

  it("applies length normalization: shorter doc with same tf ranks higher", () => {
    const { score } = buildIndex([
      { item: "short", text: "Open File" },
      { item: "long", text: "Open File In New Window From Recent History" },
    ]);
    const results = score("open");
    expect(results[0].item).toBe("short");
  });

  it("returns results sorted by score descending", () => {
    const { score } = buildIndex(corpus);
    const results = score("open file");
    for (let i = 1; i < results.length; i++) {
      expect(results[i - 1].score).toBeGreaterThanOrEqual(results[i].score);
    }
  });

  it("handles exact token match", () => {
    const { score } = buildIndex(corpus);
    const results = score("terminal");
    const ids = results.map((r) => r.item);
    expect(ids).toContain("new-terminal");
    expect(ids).toContain("terminal-settings");
  });

  it("is case-insensitive", () => {
    const { score } = buildIndex(corpus);
    expect(score("TERMINAL").map((r) => r.item)).toEqual(
      score("terminal").map((r) => r.item),
    );
  });
});
