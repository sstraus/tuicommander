import { describe, it, expect } from "vitest";
import { SUGGEST_ANCHOR_RE, INTENT_RE } from "../../components/Terminal/renderObserver";

describe("renderObserver regexes", () => {
  describe("SUGGEST_ANCHOR_RE", () => {
    it("matches plain suggest: line", () => {
      expect(SUGGEST_ANCHOR_RE.test("suggest: 1) foo | 2) bar")).toBe(true);
    });

    it("matches suggest with leading whitespace", () => {
      expect(SUGGEST_ANCHOR_RE.test("  suggest: 1) foo")).toBe(true);
    });

    it("matches suggest with bullet prefix", () => {
      expect(SUGGEST_ANCHOR_RE.test("● suggest: something")).toBe(true);
      expect(SUGGEST_ANCHOR_RE.test("⏺ suggest: something")).toBe(true);
    });

    it("does not match suggest without content after colon", () => {
      expect(SUGGEST_ANCHOR_RE.test("suggest: ")).toBe(false);
    });

    it("does not match suggest in the middle of a line", () => {
      expect(SUGGEST_ANCHOR_RE.test("foo suggest: bar")).toBe(false);
    });
  });

  describe("INTENT_RE", () => {
    it("matches plain intent: line", () => {
      expect(INTENT_RE.test("intent: doing stuff")).toBe(true);
    });

    it("matches intent with leading whitespace", () => {
      expect(INTENT_RE.test("  intent: doing stuff")).toBe(true);
    });

    it("matches intent with bullet prefix", () => {
      expect(INTENT_RE.test("● intent: doing stuff")).toBe(true);
    });

    it("does not match intent in the middle of a line", () => {
      expect(INTENT_RE.test("foo intent: bar")).toBe(false);
    });
  });
});
