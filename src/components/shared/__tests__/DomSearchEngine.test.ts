import { describe, it, expect, beforeEach } from "vitest";
import { DomSearchEngine } from "../DomSearchEngine";
import type { SearchOptions } from "../DomSearchEngine";

const DEFAULT_OPTS: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };

function makeContainer(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}

describe("DomSearchEngine", () => {
  let container: HTMLElement;
  let engine: DomSearchEngine;

  beforeEach(() => {
    container = makeContainer("<p>Hello world, hello universe</p>");
    engine = new DomSearchEngine(container);
  });

  describe("search()", () => {
    it("returns correct match count", () => {
      expect(engine.search("hello", DEFAULT_OPTS)).toBe(2);
      expect(engine.matchCount()).toBe(2);
    });

    it("returns 0 for no matches", () => {
      expect(engine.search("notfound", DEFAULT_OPTS)).toBe(0);
      expect(engine.matchCount()).toBe(0);
    });

    it("returns 0 for empty term", () => {
      expect(engine.search("", DEFAULT_OPTS)).toBe(0);
    });

    it("injects <mark> elements with search-match class", () => {
      engine.search("hello", DEFAULT_OPTS);
      const marks = container.querySelectorAll("mark.search-match");
      expect(marks.length).toBe(2);
    });

    it("sets first match as active", () => {
      engine.search("hello", DEFAULT_OPTS);
      expect(engine.activeIndex()).toBe(0);
      const active = container.querySelectorAll("mark.search-match-active");
      expect(active.length).toBe(1);
    });

    it("clears previous highlights on re-search", () => {
      engine.search("hello", DEFAULT_OPTS);
      engine.search("world", DEFAULT_OPTS);
      const marks = container.querySelectorAll("mark.search-match");
      expect(marks.length).toBe(1);
      expect(marks[0].textContent).toBe("world");
    });
  });

  describe("case sensitivity", () => {
    it("matches case-insensitively by default", () => {
      expect(engine.search("HELLO", DEFAULT_OPTS)).toBe(2);
    });

    it("respects caseSensitive option", () => {
      expect(engine.search("Hello", { ...DEFAULT_OPTS, caseSensitive: true })).toBe(1);
    });
  });

  describe("regex mode", () => {
    it("supports regex patterns", () => {
      expect(engine.search("hel+o", { ...DEFAULT_OPTS, regex: true })).toBe(2);
    });

    it("returns 0 for invalid regex without throwing", () => {
      expect(engine.search("[invalid", { ...DEFAULT_OPTS, regex: true })).toBe(0);
    });
  });

  describe("whole word", () => {
    it("matches whole words only", () => {
      container = makeContainer("<p>import port export</p>");
      engine = new DomSearchEngine(container);
      expect(engine.search("port", { ...DEFAULT_OPTS, wholeWord: true })).toBe(1);
    });
  });

  describe("cross-element matching", () => {
    it("matches text across adjacent inline elements", () => {
      container = makeContainer("<p><em>hello</em> world</p>");
      engine = new DomSearchEngine(container);
      expect(engine.search("hello world", DEFAULT_OPTS)).toBe(1);
    });

    it("matches across bold and plain text", () => {
      container = makeContainer("<p>say <strong>hello</strong> world</p>");
      engine = new DomSearchEngine(container);
      expect(engine.search("hello world", DEFAULT_OPTS)).toBe(1);
    });
  });

  describe("navigation", () => {
    it("next() advances active index", () => {
      engine.search("hello", DEFAULT_OPTS);
      expect(engine.activeIndex()).toBe(0);
      engine.next();
      expect(engine.activeIndex()).toBe(1);
    });

    it("next() wraps around", () => {
      engine.search("hello", DEFAULT_OPTS);
      engine.next(); // 1
      engine.next(); // wraps to 0
      expect(engine.activeIndex()).toBe(0);
    });

    it("prev() goes backward", () => {
      engine.search("hello", DEFAULT_OPTS);
      engine.prev(); // wraps to last
      expect(engine.activeIndex()).toBe(1);
    });

    it("updates active class on navigation", () => {
      engine.search("hello", DEFAULT_OPTS);
      const marks = container.querySelectorAll("mark.search-match");
      expect(marks[0].classList.contains("search-match-active")).toBe(true);
      expect(marks[1].classList.contains("search-match-active")).toBe(false);

      engine.next();
      expect(marks[0].classList.contains("search-match-active")).toBe(false);
      expect(marks[1].classList.contains("search-match-active")).toBe(true);
    });
  });

  describe("clear()", () => {
    it("removes all <mark> elements and restores text", () => {
      const original = container.innerHTML;
      engine.search("hello", DEFAULT_OPTS);
      expect(container.querySelectorAll("mark").length).toBeGreaterThan(0);
      engine.clear();
      expect(container.querySelectorAll("mark").length).toBe(0);
      expect(container.innerHTML).toBe(original);
    });

    it("resets match count and active index", () => {
      engine.search("hello", DEFAULT_OPTS);
      engine.clear();
      expect(engine.matchCount()).toBe(0);
      expect(engine.activeIndex()).toBe(-1);
    });
  });

  describe("details auto-expand", () => {
    it("opens collapsed <details> containing a match", () => {
      container = makeContainer("<details><summary>Title</summary><p>hidden hello</p></details>");
      engine = new DomSearchEngine(container);
      engine.search("hidden", DEFAULT_OPTS);
      const details = container.querySelector("details");
      expect(details?.hasAttribute("open")).toBe(true);
    });
  });

  describe("HTML entities", () => {
    it("searches visible text, not raw HTML", () => {
      container = makeContainer("<p>A &amp; B</p>");
      engine = new DomSearchEngine(container);
      // The visible text is "A & B", search for "&"
      expect(engine.search("&", DEFAULT_OPTS)).toBe(1);
    });
  });

  describe("highlight cap", () => {
    it("caps highlights at 1000 matches", () => {
      const text = "a ".repeat(1500);
      container = makeContainer(`<p>${text}</p>`);
      engine = new DomSearchEngine(container);
      const count = engine.search("a", DEFAULT_OPTS);
      expect(count).toBe(1500); // matchCount is accurate
      const marks = container.querySelectorAll("mark.search-match");
      expect(marks.length).toBeLessThanOrEqual(1000);
    });
  });
});
