import { describe, it, expect, beforeEach } from "vitest";
import { LineBuffer } from "../../utils/lineBuffer";

describe("LineBuffer", () => {
  let buf: LineBuffer;

  beforeEach(() => {
    buf = new LineBuffer();
  });

  describe("push", () => {
    it("returns empty array when chunk has no newline", () => {
      expect(buf.push("hello")).toEqual([]);
    });

    it("returns a single line when chunk ends with newline", () => {
      expect(buf.push("hello\n")).toEqual(["hello"]);
    });

    it("retains partial trailing data for next push", () => {
      expect(buf.push("hel")).toEqual([]);
      expect(buf.push("lo\n")).toEqual(["hello"]);
    });

    it("splits a chunk with multiple newlines into multiple lines", () => {
      expect(buf.push("a\nb\nc\n")).toEqual(["a", "b", "c"]);
    });

    it("handles chunk with newline in middle: emits completed line, retains tail", () => {
      const lines = buf.push("foo\nbar");
      expect(lines).toEqual(["foo"]);
      // tail 'bar' emitted on next newline
      expect(buf.push("\n")).toEqual(["bar"]);
    });

    it("handles empty string input", () => {
      expect(buf.push("")).toEqual([]);
    });

    it("handles multiple consecutive pushes accumulating a line", () => {
      buf.push("a");
      buf.push("b");
      buf.push("c");
      expect(buf.push("\n")).toEqual(["abc"]);
    });

    it("handles \\r\\n line endings by stripping trailing \\r", () => {
      // Windows PTY output uses \r\n; LineBuffer strips trailing \r for cross-platform regex matching
      const lines = buf.push("hello\r\n");
      expect(lines).toEqual(["hello"]);
    });

    it("handles multi-byte UTF-8 characters split across chunks", () => {
      // '€' is 3 bytes in UTF-8 but a single JS string character
      buf.push("€ sign");
      expect(buf.push("\n")).toEqual(["€ sign"]);
    });

    it("handles a line that is just a newline (empty line)", () => {
      expect(buf.push("\n")).toEqual([""]);
    });

    it("does not emit the trailing partial line as a complete line", () => {
      buf.push("partial");
      // Nothing emitted until newline arrives
      expect(buf.push("more")).toEqual([]);
    });

    it("handles large burst with many lines at once", () => {
      const input = Array.from({ length: 100 }, (_, i) => `line${i}`).join("\n") + "\n";
      const lines = buf.push(input);
      expect(lines).toHaveLength(100);
      expect(lines[0]).toBe("line0");
      expect(lines[99]).toBe("line99");
    });
  });
});
