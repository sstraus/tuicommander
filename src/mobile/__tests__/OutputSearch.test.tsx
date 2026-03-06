import { describe, it, expect } from "vitest";
import { lineMatchesQuery, type LogLine } from "../utils/logLine";

// --- Search toggle logic ---
// Mirrors the searchOpen/searchQuery state machine in SessionDetailScreen.

function makeSearchState() {
  let open = false;
  let query = "";

  return {
    get open() { return open; },
    get query() { return query; },
    toggle() {
      if (open) {
        open = false;
        query = "";
      } else {
        open = true;
      }
    },
    setQuery(q: string) { query = q; },
    dismissOnEscape() {
      if (open) {
        open = false;
        query = "";
      }
    },
  };
}

describe("search toggle state machine", () => {
  it("starts closed with empty query", () => {
    const s = makeSearchState();
    expect(s.open).toBe(false);
    expect(s.query).toBe("");
  });

  it("opens on first toggle", () => {
    const s = makeSearchState();
    s.toggle();
    expect(s.open).toBe(true);
  });

  it("closes and clears query on second toggle", () => {
    const s = makeSearchState();
    s.toggle();
    s.setQuery("error");
    s.toggle();
    expect(s.open).toBe(false);
    expect(s.query).toBe("");
  });

  it("re-opens with empty query after close", () => {
    const s = makeSearchState();
    s.toggle();
    s.setQuery("warn");
    s.toggle();
    s.toggle();
    expect(s.open).toBe(true);
    expect(s.query).toBe("");
  });

  it("dismisses on Escape when open", () => {
    const s = makeSearchState();
    s.toggle();
    s.setQuery("fatal");
    s.dismissOnEscape();
    expect(s.open).toBe(false);
    expect(s.query).toBe("");
  });

  it("Escape is a no-op when already closed", () => {
    const s = makeSearchState();
    s.dismissOnEscape();
    expect(s.open).toBe(false);
    expect(s.query).toBe("");
  });
});

// --- Output line filtering ---
// Mirrors the displayedLines memo in OutputView that filters allLines
// through lineMatchesQuery when a searchQuery is present.

function filterLines(lines: LogLine[], query: string | undefined): LogLine[] {
  if (!query) return lines;
  return lines.filter((line) => lineMatchesQuery(line, query));
}

const SAMPLE_LINES: LogLine[] = [
  { spans: [{ text: "INFO: server started on port 3000" }] },
  { spans: [{ text: "WARN: deprecated API called" }] },
  { spans: [{ text: "ERROR: connection refused" }] },
  { spans: [{ text: "INFO: request handled in 12ms" }] },
  { spans: [{ text: "ERROR: timeout waiting for response" }] },
];

describe("output line filtering", () => {
  it("returns all lines when query is empty", () => {
    expect(filterLines(SAMPLE_LINES, "")).toEqual(SAMPLE_LINES);
  });

  it("returns all lines when query is undefined", () => {
    expect(filterLines(SAMPLE_LINES, undefined)).toEqual(SAMPLE_LINES);
  });

  it("filters lines matching query case-insensitively", () => {
    const result = filterLines(SAMPLE_LINES, "error");
    expect(result).toHaveLength(2);
    expect(result[0].spans[0].text).toContain("ERROR: connection refused");
    expect(result[1].spans[0].text).toContain("ERROR: timeout");
  });

  it("filters by partial match", () => {
    const result = filterLines(SAMPLE_LINES, "port");
    expect(result).toHaveLength(1);
    expect(result[0].spans[0].text).toContain("port 3000");
  });

  it("returns empty array when nothing matches", () => {
    expect(filterLines(SAMPLE_LINES, "xyz-no-match")).toEqual([]);
  });

  it("matches across multiple spans in a line", () => {
    const multiSpanLines: LogLine[] = [
      { spans: [{ text: "conn" }, { text: "ection" }, { text: " lost" }] },
      { spans: [{ text: "all good" }] },
    ];
    const result = filterLines(multiSpanLines, "connection");
    expect(result).toHaveLength(1);
    expect(result[0].spans[0].text).toBe("conn");
  });

  it("handles empty line array", () => {
    expect(filterLines([], "error")).toEqual([]);
  });
});
