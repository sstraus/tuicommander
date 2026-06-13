import { beforeEach, describe, expect, it } from "vitest";
import type { SearchOptions } from "../DomSearchEngine";
import { buildSearchPattern, DomSearchEngine } from "../DomSearchEngine";

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

	describe("matchFractions()", () => {
		const asRect = (top: number, height: number): DOMRect =>
			({ top, height, left: 0, right: 0, bottom: top + height, width: 0, x: 0, y: top, toJSON() {} }) as DOMRect;

		/** Build a scroll container with a given scrollHeight and top offset 0. */
		const makeScrollEl = (scrollHeight: number, scrollTop = 0): HTMLElement => {
			const el = document.createElement("div");
			Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true });
			Object.defineProperty(el, "scrollTop", { value: scrollTop, configurable: true });
			el.getBoundingClientRect = () => asRect(0, 0);
			return el;
		};

		/** Override each rendered <mark>'s rect (in DOM order). */
		const stubMarkRects = (...rects: DOMRect[]) => {
			const marks = container.querySelectorAll<HTMLElement>("mark.search-match");
			marks.forEach((m, i) => {
				m.getBoundingClientRect = () => rects[i];
			});
		};

		it("returns [] when the scroll container has no height", () => {
			engine.search("hello", DEFAULT_OPTS);
			expect(engine.matchFractions(makeScrollEl(0))).toEqual([]);
		});

		it("maps each match to the fraction of its center down the scroll height", () => {
			engine.search("hello", DEFAULT_OPTS); // two marks
			stubMarkRects(asRect(100, 10), asRect(600, 10));
			const fr = engine.matchFractions(makeScrollEl(1000));
			expect(fr).toHaveLength(2);
			expect(fr[0]).toBeCloseTo(0.105); // (100 + 5) / 1000
			expect(fr[1]).toBeCloseTo(0.605); // (600 + 5) / 1000
		});

		it("deduplicates matches that land in the same permille bucket", () => {
			engine.search("hello", DEFAULT_OPTS);
			stubMarkRects(asRect(300, 10), asRect(300, 10));
			expect(engine.matchFractions(makeScrollEl(1000))).toHaveLength(1);
		});

		it("clamps fractions to [0, 1]", () => {
			engine.search("hello", DEFAULT_OPTS);
			stubMarkRects(asRect(-50, 10), asRect(5000, 10));
			const fr = engine.matchFractions(makeScrollEl(1000));
			expect(fr[0]).toBe(0);
			expect(fr[1]).toBe(1);
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

describe("buildSearchPattern", () => {
	it("returns null for an empty term", () => {
		expect(buildSearchPattern("", DEFAULT_OPTS)).toBeNull();
	});

	it("escapes regex metacharacters in literal mode", () => {
		const re = buildSearchPattern("a.b(c)", DEFAULT_OPTS);
		expect(re).not.toBeNull();
		expect("a.b(c)".match(re as RegExp)).toHaveLength(1);
		expect("aXbYc".match(re as RegExp)).toBeNull();
	});

	it("honors caseSensitive flag", () => {
		expect(buildSearchPattern("Hi", { ...DEFAULT_OPTS, caseSensitive: true })?.flags).toBe("g");
		expect(buildSearchPattern("Hi", DEFAULT_OPTS)?.flags).toBe("gi");
	});

	it("wraps with word boundaries for wholeWord", () => {
		const re = buildSearchPattern("cat", { ...DEFAULT_OPTS, wholeWord: true });
		expect("a cat sat".match(re as RegExp)).toHaveLength(1);
		expect("category".match(re as RegExp)).toBeNull();
	});

	it("treats the term as a pattern in regex mode", () => {
		const re = buildSearchPattern("ab+", { ...DEFAULT_OPTS, regex: true });
		expect("abbb".match(re as RegExp)).toHaveLength(1);
	});

	it("returns null for an invalid regex", () => {
		expect(buildSearchPattern("a(", { ...DEFAULT_OPTS, regex: true })).toBeNull();
	});
});
