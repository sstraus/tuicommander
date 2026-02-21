import { describe, it, expect } from "vitest";
import { stripAnsi } from "../../utils/stripAnsi";

describe("stripAnsi", () => {
  it("passes through plain text unchanged", () => {
    expect(stripAnsi("hello world")).toBe("hello world");
  });

  it("removes SGR color reset (ESC[0m)", () => {
    expect(stripAnsi("\x1b[0mhello\x1b[0m")).toBe("hello");
  });

  it("removes SGR foreground color (ESC[32m)", () => {
    expect(stripAnsi("\x1b[32mgreen text\x1b[0m")).toBe("green text");
  });

  it("removes SGR bold (ESC[1m)", () => {
    expect(stripAnsi("\x1b[1mbold\x1b[0m")).toBe("bold");
  });

  it("removes combined SGR attributes (ESC[1;32m)", () => {
    expect(stripAnsi("\x1b[1;32mbold green\x1b[0m")).toBe("bold green");
  });

  it("removes 256-color foreground (ESC[38;5;196m)", () => {
    expect(stripAnsi("\x1b[38;5;196mred256\x1b[0m")).toBe("red256");
  });

  it("removes true-color foreground (ESC[38;2;255;0;0m)", () => {
    expect(stripAnsi("\x1b[38;2;255;0;0mrgb red\x1b[0m")).toBe("rgb red");
  });

  it("removes cursor movement up (ESC[1A)", () => {
    expect(stripAnsi("before\x1b[1Aafter")).toBe("beforeafter");
  });

  it("removes cursor to column (ESC[5G)", () => {
    expect(stripAnsi("abc\x1b[5Gxyz")).toBe("abcxyz");
  });

  it("removes erase in line (ESC[2K)", () => {
    expect(stripAnsi("line\x1b[2K")).toBe("line");
  });

  it("removes OSC sequence with BEL terminator (ESC]0;title BEL)", () => {
    expect(stripAnsi("\x1b]0;My Title\x07rest")).toBe("rest");
  });

  it("removes OSC sequence with ST terminator (ESC]0;title ESC\\)", () => {
    expect(stripAnsi("\x1b]0;My Title\x1b\\rest")).toBe("rest");
  });

  it("removes hyperlink OSC sequence", () => {
    expect(stripAnsi("\x1b]8;;https://example.com\x07link text\x1b]8;;\x07")).toBe("link text");
  });

  it("handles multiple sequences in one string", () => {
    expect(stripAnsi("\x1b[32m✓\x1b[0m success \x1b[31m✗\x1b[0m failure")).toBe(
      "✓ success ✗ failure",
    );
  });

  it("handles empty string", () => {
    expect(stripAnsi("")).toBe("");
  });

  it("handles string with only ANSI codes", () => {
    expect(stripAnsi("\x1b[0m\x1b[1m\x1b[32m")).toBe("");
  });

  it("preserves multi-byte UTF-8 characters", () => {
    expect(stripAnsi("\x1b[32m€\x1b[0m")).toBe("€");
  });

  it("removes DEC private mode sequences (ESC[?25l, ESC[?25h)", () => {
    expect(stripAnsi("\x1b[?25lhidden cursor\x1b[?25h")).toBe("hidden cursor");
  });

  it("removes scroll region sequences (ESC[H ESC[2J)", () => {
    expect(stripAnsi("\x1b[H\x1b[2Jclear")).toBe("clear");
  });
});
