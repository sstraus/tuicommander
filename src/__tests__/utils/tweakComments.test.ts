import { describe, expect, it } from "vitest";
import {
  parseTweakComments,
  serializeTweakComment,
  insertTweakComment,
  removeTweakComment,
  updateTweakComment,
  ensureConventionHeader,
  applyTweakHighlights,
  CONVENTION_HEADER,
  type TweakComment,
} from "../../utils/tweakComments";

describe("tweakComments parser/serializer", () => {
  describe("serializeTweakComment", () => {
    it("wraps highlighted text with begin/end markers containing base64 payload", () => {
      const out = serializeTweakComment({
        id: "c_abc",
        highlighted: "evidenziato",
        comment: "il mio commento",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      expect(out).toContain("<!--tweak:begin:c_abc-->evidenziato<!--tweak:end:c_abc:");
      // payload must decode to JSON with comment + created_at
      const match = out.match(/tweak:end:c_abc:([^>]+)-->/);
      expect(match).toBeTruthy();
      const payload = JSON.parse(atob(match![1]));
      expect(payload.comment).toBe("il mio commento");
      expect(payload.created_at).toBe("2026-04-05T10:00:00.000Z");
    });

    it("handles comments with quotes, newlines and special characters", () => {
      const tricky = 'ha "virgolette", a capo\ne simboli --> <!-- <script>';
      const out = serializeTweakComment({
        id: "c_1",
        highlighted: "x",
        comment: tricky,
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      const match = out.match(/tweak:end:c_1:([^>]+)-->/);
      const payload = JSON.parse(atob(match![1]));
      expect(payload.comment).toBe(tricky);
    });
  });

  describe("parseTweakComments", () => {
    it("returns empty array when no markers present", () => {
      expect(parseTweakComments("# Title\n\nplain text")).toEqual([]);
    });

    it("extracts a single comment with its highlighted text", () => {
      const payload = btoa(
        JSON.stringify({ comment: "ciao", created_at: "2026-04-05T10:00:00.000Z" }),
      );
      const src = `Hello <!--tweak:begin:c_1-->world<!--tweak:end:c_1:${payload}-->!`;
      const comments = parseTweakComments(src);
      expect(comments).toHaveLength(1);
      expect(comments[0].id).toBe("c_1");
      expect(comments[0].highlighted).toBe("world");
      expect(comments[0].comment).toBe("ciao");
      expect(comments[0].createdAt).toBe("2026-04-05T10:00:00.000Z");
    });

    it("extracts multiple comments preserving order", () => {
      const p1 = btoa(JSON.stringify({ comment: "first", created_at: "2026-04-05T10:00:00.000Z" }));
      const p2 = btoa(JSON.stringify({ comment: "second", created_at: "2026-04-05T10:01:00.000Z" }));
      const src =
        `A <!--tweak:begin:c_1-->one<!--tweak:end:c_1:${p1}--> B ` +
        `<!--tweak:begin:c_2-->two<!--tweak:end:c_2:${p2}--> C`;
      const comments = parseTweakComments(src);
      expect(comments.map((c) => c.id)).toEqual(["c_1", "c_2"]);
      expect(comments.map((c) => c.comment)).toEqual(["first", "second"]);
    });

    it("ignores malformed markers instead of throwing", () => {
      const src = `text <!--tweak:begin:bad--> no end marker`;
      expect(parseTweakComments(src)).toEqual([]);
    });

    it("ignores markers with invalid base64 payload", () => {
      const src = `<!--tweak:begin:c_1-->x<!--tweak:end:c_1:!!!not-base64!!!-->`;
      expect(parseTweakComments(src)).toEqual([]);
    });
  });

  describe("round-trip parse(serialize(x))", () => {
    it("preserves all fields", () => {
      const original: TweakComment = {
        id: "c_xyz",
        highlighted: "la parte evidenziata",
        comment: 'commento con "quotes" e caratteri speciali <>&',
        createdAt: "2026-04-05T10:00:00.000Z",
      };
      const src = `Prefix ${serializeTweakComment(original)} suffix.`;
      const [parsed] = parseTweakComments(src);
      expect(parsed).toEqual(original);
    });
  });

  describe("insertTweakComment", () => {
    it("replaces the exact highlighted substring with the wrapped version", () => {
      const src = "Una frase con parola evidenziata dentro.";
      const out = insertTweakComment(src, {
        id: "c_1",
        highlighted: "parola evidenziata",
        comment: "nota",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      expect(out).toContain("<!--tweak:begin:c_1-->parola evidenziata<!--tweak:end:c_1:");
      // Body (after convention header) preserves original prefix/suffix around the highlight.
      const body = out.slice(CONVENTION_HEADER.length);
      expect(body.startsWith("Una frase con ")).toBe(true);
      expect(body.endsWith(" dentro.")).toBe(true);
    });

    it("adds convention header on first insertion", () => {
      const src = "Plain document.";
      const out = insertTweakComment(src, {
        id: "c_1",
        highlighted: "Plain",
        comment: "x",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      expect(out).toContain(CONVENTION_HEADER);
      expect(out.indexOf(CONVENTION_HEADER)).toBe(0);
    });

    it("does not duplicate convention header on subsequent insertions", () => {
      let src = "First word and second word.";
      src = insertTweakComment(src, {
        id: "c_1",
        highlighted: "First",
        comment: "a",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      src = insertTweakComment(src, {
        id: "c_2",
        highlighted: "second",
        comment: "b",
        createdAt: "2026-04-05T10:01:00.000Z",
      });
      const occurrences = src.split(CONVENTION_HEADER).length - 1;
      expect(occurrences).toBe(1);
    });

    it("throws when highlighted text is not found in source", () => {
      expect(() =>
        insertTweakComment("hello", {
          id: "c_1",
          highlighted: "missing",
          comment: "x",
          createdAt: "2026-04-05T10:00:00.000Z",
        }),
      ).toThrow();
    });
  });

  describe("removeTweakComment", () => {
    it("removes markers but preserves highlighted text", () => {
      const src = `A <!--tweak:begin:c_1-->kept<!--tweak:end:c_1:${btoa(
        JSON.stringify({ comment: "x", created_at: "2026-04-05T10:00:00.000Z" }),
      )}--> B`;
      const out = removeTweakComment(src, "c_1");
      expect(out).toBe("A kept B");
    });

    it("removes convention header when last comment is removed", () => {
      let src = "Word here.";
      src = insertTweakComment(src, {
        id: "c_1",
        highlighted: "Word",
        comment: "x",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      expect(src).toContain(CONVENTION_HEADER);
      src = removeTweakComment(src, "c_1");
      expect(src).not.toContain(CONVENTION_HEADER);
      expect(src).toBe("Word here.");
    });

    it("keeps convention header when other comments remain", () => {
      let src = "Word one and word two.";
      src = insertTweakComment(src, {
        id: "c_1",
        highlighted: "one",
        comment: "a",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      src = insertTweakComment(src, {
        id: "c_2",
        highlighted: "two",
        comment: "b",
        createdAt: "2026-04-05T10:01:00.000Z",
      });
      src = removeTweakComment(src, "c_1");
      expect(src).toContain(CONVENTION_HEADER);
      expect(parseTweakComments(src)).toHaveLength(1);
    });

    it("returns source unchanged when id not found", () => {
      const src = "no markers here";
      expect(removeTweakComment(src, "c_missing")).toBe(src);
    });
  });

  describe("updateTweakComment", () => {
    it("updates the comment text while preserving id and highlighted text", () => {
      let src = "Hello world.";
      src = insertTweakComment(src, {
        id: "c_1",
        highlighted: "world",
        comment: "original",
        createdAt: "2026-04-05T10:00:00.000Z",
      });
      src = updateTweakComment(src, "c_1", "updated");
      const [parsed] = parseTweakComments(src);
      expect(parsed.comment).toBe("updated");
      expect(parsed.highlighted).toBe("world");
      expect(parsed.id).toBe("c_1");
    });
  });

  describe("applyTweakHighlights", () => {
    it("converts tweak markers to highlight spans", () => {
      const p = btoa(JSON.stringify({ comment: "nota", created_at: "2026-04-05T10:00:00.000Z" }));
      const src = `Hello <!--tweak:begin:c_1-->world<!--tweak:end:c_1:${p}-->!`;
      const out = applyTweakHighlights(src);
      expect(out).toContain(`<span class="tweak-highlight" data-tweak-id="c_1" data-tweak-comment-b64="${p}">world</span>`);
      expect(out).not.toContain("<!--tweak:");
    });

    it("strips the convention header", () => {
      const src = CONVENTION_HEADER + "# Title\n\nBody text.";
      const out = applyTweakHighlights(src);
      expect(out).not.toContain(CONVENTION_HEADER);
      expect(out).toContain("# Title");
    });

    it("is a no-op on plain markdown without markers", () => {
      const src = "# Title\n\nSome **bold** text.";
      expect(applyTweakHighlights(src)).toBe(src);
    });
  });

  describe("ensureConventionHeader", () => {
    it("adds header when missing", () => {
      const out = ensureConventionHeader("body");
      expect(out.startsWith(CONVENTION_HEADER)).toBe(true);
      expect(out).toContain("body");
    });

    it("is idempotent when header already present", () => {
      const first = ensureConventionHeader("body");
      const second = ensureConventionHeader(first);
      expect(second).toBe(first);
    });
  });
});
