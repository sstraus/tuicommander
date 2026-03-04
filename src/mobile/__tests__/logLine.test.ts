import { describe, it, expect } from "vitest";
import {
  logColorToCss,
  spanStyle,
  normalizeLogLine,
  type LogColor,
  type LogSpan,
  type LogLine,
} from "../utils/logLine";

// --- logColorToCss ---

describe("logColorToCss", () => {
  it("returns undefined for undefined input", () => {
    expect(logColorToCss(undefined)).toBeUndefined();
  });

  it("returns undefined for empty object", () => {
    expect(logColorToCss({} as LogColor)).toBeUndefined();
  });

  it("maps rgb color to CSS rgb()", () => {
    expect(logColorToCss({ rgb: [255, 128, 0] })).toBe("rgb(255,128,0)");
  });

  it("maps ANSI 16-color index 0 (black) to CSS variable", () => {
    expect(logColorToCss({ idx: 0 })).toBe("var(--ansi-black)");
  });

  it("maps ANSI 16-color index 1 (red) to CSS variable", () => {
    expect(logColorToCss({ idx: 1 })).toBe("var(--ansi-red)");
  });

  it("maps ANSI 16-color index 15 (bright white) to CSS variable", () => {
    expect(logColorToCss({ idx: 15 })).toBe("var(--ansi-bright-white)");
  });

  it("maps ANSI 256-color cube index 16 to hex", () => {
    // idx 16 = cube(0,0,0) = #000000
    expect(logColorToCss({ idx: 16 })).toBe("#000000");
  });

  it("maps ANSI 256-color cube index 196 to red hex", () => {
    // idx 196 = cube(5,0,0) => r=255, g=0, b=0
    expect(logColorToCss({ idx: 196 })).toBe("#ff0000");
  });

  it("maps ANSI 256-color grayscale index 232 to dark gray", () => {
    // idx 232 = gray(0) = 8 => #080808
    expect(logColorToCss({ idx: 232 })).toBe("#080808");
  });

  it("maps ANSI 256-color grayscale index 255 to near-white", () => {
    // idx 255 = gray(23) = 8 + 23*10 = 238 => #eeeeee
    expect(logColorToCss({ idx: 255 })).toBe("#eeeeee");
  });

  it("prefers rgb over idx when both present", () => {
    expect(logColorToCss({ rgb: [1, 2, 3], idx: 5 })).toBe("rgb(1,2,3)");
  });
});

// --- spanStyle ---

describe("spanStyle", () => {
  it("returns undefined for plain text span", () => {
    expect(spanStyle({ text: "hello" })).toBeUndefined();
  });

  it("returns color for fg", () => {
    const result = spanStyle({ text: "x", fg: { idx: 1 } });
    expect(result).toEqual({ color: "var(--ansi-red)" });
  });

  it("returns background-color for bg", () => {
    const result = spanStyle({ text: "x", bg: { idx: 2 } });
    expect(result).toEqual({ "background-color": "var(--ansi-green)" });
  });

  it("returns font-weight for bold", () => {
    const result = spanStyle({ text: "x", bold: true });
    expect(result).toEqual({ "font-weight": "600" });
  });

  it("returns font-style for italic", () => {
    const result = spanStyle({ text: "x", italic: true });
    expect(result).toEqual({ "font-style": "italic" });
  });

  it("returns text-decoration for underline", () => {
    const result = spanStyle({ text: "x", underline: true });
    expect(result).toEqual({ "text-decoration": "underline" });
  });

  it("combines multiple attributes", () => {
    const result = spanStyle({
      text: "x",
      fg: { rgb: [255, 0, 0] },
      bold: true,
      underline: true,
    });
    expect(result).toEqual({
      color: "rgb(255,0,0)",
      "font-weight": "600",
      "text-decoration": "underline",
    });
  });
});

// --- normalizeLogLine ---

describe("normalizeLogLine", () => {
  it("wraps plain string into single span", () => {
    const result = normalizeLogLine("hello world");
    expect(result).toEqual({ spans: [{ text: "hello world" }] });
  });

  it("passes through valid LogLine object", () => {
    const logLine: LogLine = { spans: [{ text: "a", fg: { idx: 1 } }, { text: "b" }] };
    expect(normalizeLogLine(logLine)).toBe(logLine);
  });

  it("wraps number as string span", () => {
    expect(normalizeLogLine(42)).toEqual({ spans: [{ text: "42" }] });
  });

  it("wraps null as string span", () => {
    expect(normalizeLogLine(null)).toEqual({ spans: [{ text: "null" }] });
  });

  it("wraps undefined as string span", () => {
    expect(normalizeLogLine(undefined)).toEqual({ spans: [{ text: "undefined" }] });
  });
});
