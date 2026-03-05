import { describe, it, expect } from "vitest";
import { lineMatchesQuery, lineText } from "../utils/logLine";
import type { LogLine } from "../utils/logLine";

describe("lineText", () => {
  it("concatenates span texts", () => {
    const line: LogLine = {
      spans: [
        { text: "Hello " },
        { text: "World" },
      ],
    };
    expect(lineText(line)).toBe("Hello World");
  });

  it("handles single span", () => {
    const line: LogLine = { spans: [{ text: "foo" }] };
    expect(lineText(line)).toBe("foo");
  });

  it("handles empty spans", () => {
    const line: LogLine = { spans: [] };
    expect(lineText(line)).toBe("");
  });
});

describe("lineMatchesQuery", () => {
  it("returns true for case-insensitive match", () => {
    const line: LogLine = { spans: [{ text: "Error: something failed" }] };
    expect(lineMatchesQuery(line, "error")).toBe(true);
    expect(lineMatchesQuery(line, "ERROR")).toBe(true);
    expect(lineMatchesQuery(line, "something")).toBe(true);
  });

  it("returns false when no match", () => {
    const line: LogLine = { spans: [{ text: "All good" }] };
    expect(lineMatchesQuery(line, "error")).toBe(false);
  });

  it("matches across spans", () => {
    const line: LogLine = {
      spans: [
        { text: "err" },
        { text: "or found" },
      ],
    };
    expect(lineMatchesQuery(line, "error")).toBe(true);
  });

  it("returns true for empty query", () => {
    const line: LogLine = { spans: [{ text: "anything" }] };
    expect(lineMatchesQuery(line, "")).toBe(true);
  });
});
