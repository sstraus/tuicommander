import { describe, it, expect } from "vitest";
import type { LogLine } from "../../../mobile/utils/logLine";
import { searchLogLines, highlightSpans, type SearchMatch } from "../../../components/Terminal/scrollbackSearch";

function mkLine(...texts: string[]): LogLine {
  return { spans: texts.map((t) => ({ text: t })) };
}

function mkColorLine(segments: Array<{ text: string; fg?: { idx: number } }>): LogLine {
  return { spans: segments.map((s) => ({ text: s.text, fg: s.fg })) };
}

const defaultOpts = { caseSensitive: false, regex: false, wholeWord: false };

describe("searchLogLines", () => {
  it("returns empty for empty query", () => {
    const lines = [mkLine("hello world")];
    expect(searchLogLines(lines, "", defaultOpts)).toEqual([]);
  });

  it("returns empty for no lines", () => {
    expect(searchLogLines([], "foo", defaultOpts)).toEqual([]);
  });

  it("finds a single match", () => {
    const lines = [mkLine("hello world")];
    const matches = searchLogLines(lines, "world", defaultOpts);
    expect(matches).toEqual([{ lineIndex: 0, colStart: 6, colEnd: 11 }]);
  });

  it("finds multiple matches across lines", () => {
    const lines = [mkLine("foo bar"), mkLine("baz"), mkLine("foo baz foo")];
    const matches = searchLogLines(lines, "foo", defaultOpts);
    expect(matches).toEqual([
      { lineIndex: 0, colStart: 0, colEnd: 3 },
      { lineIndex: 2, colStart: 0, colEnd: 3 },
      { lineIndex: 2, colStart: 8, colEnd: 11 },
    ]);
  });

  it("is case-insensitive by default", () => {
    const lines = [mkLine("Hello WORLD")];
    const matches = searchLogLines(lines, "hello", defaultOpts);
    expect(matches).toHaveLength(1);
  });

  it("respects caseSensitive option", () => {
    const lines = [mkLine("Hello WORLD")];
    const matches = searchLogLines(lines, "hello", { ...defaultOpts, caseSensitive: true });
    expect(matches).toHaveLength(0);
    const matches2 = searchLogLines(lines, "Hello", { ...defaultOpts, caseSensitive: true });
    expect(matches2).toHaveLength(1);
  });

  it("supports regex search", () => {
    const lines = [mkLine("error: file not found"), mkLine("warning: deprecated")];
    const matches = searchLogLines(lines, "error|warning", { ...defaultOpts, regex: true });
    expect(matches).toHaveLength(2);
    expect(matches[0].lineIndex).toBe(0);
    expect(matches[1].lineIndex).toBe(1);
  });

  it("returns empty on invalid regex", () => {
    const lines = [mkLine("hello")];
    const matches = searchLogLines(lines, "[invalid", { ...defaultOpts, regex: true });
    expect(matches).toEqual([]);
  });

  it("supports wholeWord option", () => {
    const lines = [mkLine("foobar foo barfoo")];
    const matches = searchLogLines(lines, "foo", { ...defaultOpts, wholeWord: true });
    expect(matches).toEqual([{ lineIndex: 0, colStart: 7, colEnd: 10 }]);
  });

  it("caps matches at 1000", () => {
    const line = mkLine("a".repeat(2000));
    const matches = searchLogLines([line], "a", defaultOpts);
    expect(matches).toHaveLength(1000);
  });

  it("searches across multi-span lines", () => {
    const lines = [mkColorLine([{ text: "hel" }, { text: "lo world" }])];
    const matches = searchLogLines(lines, "hello", defaultOpts);
    expect(matches).toEqual([{ lineIndex: 0, colStart: 0, colEnd: 5 }]);
  });
});

describe("highlightSpans", () => {
  it("returns original spans when no matches", () => {
    const line = mkLine("hello world");
    const segs = highlightSpans(line, [], -1, 0);
    expect(segs).toHaveLength(1);
    expect(segs[0].text).toBe("hello world");
    expect(segs[0].highlight).toBe(false);
  });

  it("splits a single span around a match", () => {
    const line = mkLine("hello world");
    const matches: SearchMatch[] = [{ lineIndex: 0, colStart: 6, colEnd: 11 }];
    const segs = highlightSpans(line, matches, 0, 0);
    expect(segs).toHaveLength(2);
    expect(segs[0]).toMatchObject({ text: "hello ", highlight: false });
    expect(segs[1]).toMatchObject({ text: "world", highlight: true, active: true });
  });

  it("marks active match distinctly", () => {
    const line = mkLine("foo foo foo");
    const matches: SearchMatch[] = [
      { lineIndex: 0, colStart: 0, colEnd: 3 },
      { lineIndex: 0, colStart: 4, colEnd: 7 },
      { lineIndex: 0, colStart: 8, colEnd: 11 },
    ];
    const segs = highlightSpans(line, matches, 1, 0);
    const highlighted = segs.filter((s) => s.highlight);
    expect(highlighted).toHaveLength(3);
    expect(highlighted[0].active).toBe(false);
    expect(highlighted[1].active).toBe(true);
    expect(highlighted[2].active).toBe(false);
  });

  it("handles match spanning multiple spans", () => {
    const line = mkColorLine([
      { text: "hel", fg: { idx: 1 } },
      { text: "lo world", fg: { idx: 2 } },
    ]);
    const matches: SearchMatch[] = [{ lineIndex: 0, colStart: 0, colEnd: 5 }];
    const segs = highlightSpans(line, matches, 0, 0);
    const hlSegs = segs.filter((s) => s.highlight);
    expect(hlSegs).toHaveLength(2);
    expect(hlSegs[0].text).toBe("hel");
    expect(hlSegs[0].span.fg?.idx).toBe(1);
    expect(hlSegs[1].text).toBe("lo");
    expect(hlSegs[1].span.fg?.idx).toBe(2);
  });

  it("handles all-highlight line", () => {
    const line = mkLine("abc");
    const matches: SearchMatch[] = [{ lineIndex: 0, colStart: 0, colEnd: 3 }];
    const segs = highlightSpans(line, matches, 0, 0);
    expect(segs).toHaveLength(1);
    expect(segs[0]).toMatchObject({ text: "abc", highlight: true, active: true });
  });

  it("uses globalOffset for active match calculation", () => {
    const line = mkLine("xx");
    const matches: SearchMatch[] = [{ lineIndex: 5, colStart: 0, colEnd: 2 }];
    const segs = highlightSpans(line, matches, 10, 10);
    expect(segs[0].active).toBe(true);

    const segs2 = highlightSpans(line, matches, 9, 10);
    expect(segs2[0].active).toBe(false);
  });
});
