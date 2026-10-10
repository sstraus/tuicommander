import { describe, expect, it } from "vitest";
import { type StreamSplit, splitStream } from "../../utils/incrementalMarkdown";

/**
 * `splitStream` decides where a growing markdown answer may be cut into a
 * prefix that is rendered once and kept, and a tail that is re-parsed on every
 * tick. Every test here asks one question: is the cut SAFE — would the two
 * halves, parsed alone, render what the whole document renders?
 *
 * The bias is deliberate and one-directional. Refusing a safe cut costs speed;
 * taking an unsafe one corrupts the answer on screen. So the uncertain cases
 * below assert "stays in the tail" or "stays in one segment", never "must be
 * committed here".
 */
describe("splitStream", () => {
	/** Source text of every committed segment, in order. */
	const committedText = (s: StreamSplit) => s.committed.map((seg) => seg.text);

	it("commits nothing until a boundary is proven", () => {
		// No blank line yet: the paragraph may still grow.
		const split = splitStream("Just one paragraph still being written");
		expect(split.committed).toEqual([]);
		expect(split.tail.text).toBe("Just one paragraph still being written");
	});

	it("commits a finished paragraph once a new top-level block starts", () => {
		const split = splitStream("First para.\n\nSecond para.\n");
		expect(committedText(split)).toEqual(["First para.\n\n"]);
		expect(split.tail.text).toBe("Second para.\n");
	});

	it("does not commit on a trailing blank line alone", () => {
		// The next block has not started, so we cannot yet know the cut is safe:
		// what follows may be an indented continuation of the previous list.
		expect(splitStream("First para.\n\n").committed).toEqual([]);
	});

	/**
	 * A boundary is decided from the first line of the FOLLOWING block, and
	 * while the stream is open that line may still be growing: a lone "-" is a
	 * paragraph until the next byte makes it "- item", a list. Deciding on an
	 * incomplete line is how a warm scan drifts away from a cold one.
	 */
	it("never decides a boundary on an incomplete final line", () => {
		expect(splitStream("Para.\n\n-").committed).toEqual([]);
		expect(committedText(splitStream("Para.\n\n- item\n"))).toEqual([]);
	});

	/**
	 * The blank line inside a fence is not a block boundary. Cutting there would
	 * split the fence and each half would parse as an unterminated code block.
	 */
	it("never cuts inside a fenced code block", () => {
		const src = "Intro.\n\n```js\nconst a = 1;\n\nconst b = 2;\n```\n\nAfter.\n";
		const split = splitStream(src);
		expect(committedText(split)).toEqual(["Intro.\n\n", "```js\nconst a = 1;\n\nconst b = 2;\n```\n\n"]);
		expect(split.tail.text).toBe("After.\n");
	});

	it("never cuts a fence that is still open", () => {
		const split = splitStream("Intro.\n\n```js\nconst a = 1;\n\nmore coming");
		expect(committedText(split)).toEqual(["Intro.\n\n"]);
		expect(split.tail.text).toBe("```js\nconst a = 1;\n\nmore coming");
	});

	/**
	 * A loose list keeps its blank lines. Cutting between two items would render
	 * two adjacent <ul>s instead of one, and would restart an ordered list at 1.
	 */
	it("never cuts between the items of a loose bullet list", () => {
		const split = splitStream("- item one\n\n- item two\n\nAfter the list.\n");
		expect(committedText(split)).toEqual(["- item one\n\n- item two\n\n"]);
		expect(split.tail.text).toBe("After the list.\n");
	});

	it("never cuts between the items of a loose ordered list", () => {
		const split = splitStream("1. first\n\n2. second\n\nAfter.\n");
		expect(committedText(split)).toEqual(["1. first\n\n2. second\n\n"]);
	});

	it("never cuts before an indented continuation of a list item", () => {
		const split = splitStream("- item one\n\n  still item one\n\nAfter.\n");
		expect(committedText(split)).toEqual(["- item one\n\n  still item one\n\n"]);
		expect(split.tail.text).toBe("After.\n");
	});

	it("never cuts between blockquote paragraphs", () => {
		const split = splitStream("> quoted one\n\n> quoted two\n\nAfter.\n");
		expect(committedText(split)).toEqual(["> quoted one\n\n> quoted two\n\n"]);
	});

	it("never cuts before a table continuation row", () => {
		const split = splitStream("| a | b |\n| - | - |\n\n| 1 | 2 |\n\nAfter.\n");
		expect(committedText(split)).toEqual(["| a | b |\n| - | - |\n\n| 1 | 2 |\n\n"]);
	});

	/**
	 * A tag opened in one segment and closed in another would be balanced
	 * independently by DOMPurify, so the second segment escapes the wrapper.
	 * The invariant is what matters, not where the cut lands: an opening tag
	 * and its close must never end up in different segments.
	 */
	it("never separates a raw HTML block's open tag from its close", () => {
		const split = splitStream("Intro.\n\n<div class='x'>\n\ninside\n\n</div>\n\nAfter.\n");
		const all = [...split.committed, split.tail];
		const withOpen = all.filter((s) => s.text.includes("<div"));
		expect(withOpen).toHaveLength(1);
		expect(withOpen[0].text).toContain("</div>");
		expect(withOpen[0].text).toContain("inside");
	});

	/**
	 * Tweak highlights are a matched pair of private-use codepoints injected
	 * later from these HTML comments. A cut between begin and end orphans one
	 * sentinel, which then survives into the DOM as a stray glyph.
	 */
	it("never cuts between a tweak begin and its end marker", () => {
		const src =
			"Intro.\n\n<!--tweak:begin:t1-->\nHighlighted para.\n\nStill highlighted.\n<!--tweak:end:t1 @0 body-->\n\nAfter.\n";
		const split = splitStream(src);
		expect(committedText(split)).toEqual([
			"Intro.\n\n",
			"<!--tweak:begin:t1-->\nHighlighted para.\n\nStill highlighted.\n<!--tweak:end:t1 @0 body-->\n\n",
		]);
	});

	/**
	 * A reference link resolves against definitions anywhere in the document.
	 * Parsed alone, a segment holding only the usage renders literal brackets
	 * instead of an anchor; a segment holding only the definition strands a
	 * later usage.
	 */
	it("refuses to commit a segment whose reference link is defined elsewhere", () => {
		const src = "See [the docs][d] for more.\n\nNext para.\n\n[d]: https://example.com\n";
		const split = splitStream(src);
		expect(split.committed).toEqual([]);
		expect(split.tail.text).toBe(src);
	});

	/**
	 * Pairing a usage with its definition inside one segment is not enough: the
	 * shortcut `[label]` form means a usage need not look like one, and a usage
	 * that arrives LATER in the stream would resolve against this definition in
	 * the whole document but find nothing in its own segment. So a definition
	 * anywhere disables splitting for the whole document.
	 */
	it("commits nothing even when a definition sits beside its usage", () => {
		const split = splitStream("See [the docs][d].\n[d]: https://example.com\n\nNext para.\n");
		expect(split.committed).toEqual([]);
	});

	it("refuses to commit a segment holding a definition a later segment may use", () => {
		expect(splitStream("[d]: https://example.com\n\nSee [the docs][d].\n").committed).toEqual([]);
	});

	it("does not mistake a task-list checkbox for a reference link", () => {
		const split = splitStream("- [x] done\n- [ ] todo\n\nAfter.\n");
		expect(committedText(split)).toEqual(["- [x] done\n- [ ] todo\n\n"]);
	});

	describe("carried offsets", () => {
		it("gives every segment the absolute source line its first line sits on", () => {
			const split = splitStream("one\n\ntwo\n\nthree\n\ntail\n");
			expect(split.committed.map((s) => s.lineOffset)).toEqual([0, 2, 4]);
			expect(split.tail.lineOffset).toBe(6);
		});

		it("counts blank lines inside a segment toward the next offset", () => {
			const split = splitStream("```\na\n\nb\n```\n\nafter\n\ntail\n");
			expect(split.committed.map((s) => s.lineOffset)).toEqual([0, 6]);
			expect(split.tail.lineOffset).toBe(8);
		});
	});

	describe("resuming from a previous split", () => {
		/**
		 * The whole point of the split is that a tick does not re-scan what is
		 * already committed. Resuming must therefore land on exactly the answer
		 * a cold scan of the full source gives — byte for byte, offset for
		 * offset. Feeding one character at a time is the harshest form of that.
		 */
		it("matches a cold scan when fed one character at a time", () => {
			const full = "Alpha para.\n\n```js\nlet x = 1;\n\nlet y = 2;\n```\n\n- a\n\n- b\n\nOmega para.\n";
			let warm: StreamSplit | undefined;
			for (let i = 1; i <= full.length; i++) warm = splitStream(full.slice(0, i), warm);
			const cold = splitStream(full);
			const w = warm as StreamSplit;
			expect(committedText(w)).toEqual(committedText(cold));
			expect(w.tail.text).toBe(cold.tail.text);
			expect(w.committed.map((s) => s.lineOffset)).toEqual(cold.committed.map((s) => s.lineOffset));
			expect(w.tail.lineOffset).toBe(cold.tail.lineOffset);
		});

		it("keeps already-committed segments byte-identical as the source grows", () => {
			let split = splitStream("First para.\n\nSecond para.\n");
			const first = split.committed[0];
			expect(first).toBeDefined();
			split = splitStream("First para.\n\nSecond para.\n\nThird para.\n", split);
			expect(split.committed[0]).toEqual(first);
		});

		it("starts over when the source is not an extension of the previous one", () => {
			// A conversation switch replaces the text instead of appending.
			const split = splitStream("Totally different.\n\nText.\n", splitStream("First para.\n\nSecond para.\n"));
			expect(committedText(split)).toEqual(["Totally different.\n\n"]);
		});
	});

	/**
	 * The story's fifth criterion: the per-render cost must stop growing with
	 * the answer's length. The tail is the only thing re-parsed each tick, so
	 * the measurement is the total characters re-parsed across the whole stream.
	 * Today's renderer re-parses the entire document on every tick.
	 */
	describe("cost", () => {
		const paragraphs = (n: number) =>
			`${Array.from({ length: n }, (_, i) => `Paragraph number ${i} with some filler words in it.`).join("\n\n")}\n`;

		it("re-parses a bounded tail instead of the whole document", () => {
			const ticks = Array.from({ length: 200 }, (_, i) => paragraphs(i + 1));
			let split: StreamSplit | undefined;
			let reparsed = 0;
			let longestTail = 0;
			for (const tick of ticks) {
				split = splitStream(tick, split);
				reparsed += split.tail.text.length;
				longestTail = Math.max(longestTail, split.tail.text.length);
			}
			const full = ticks.at(-1)!;
			// The tail stays the size of one block, whatever the answer's length.
			expect(longestTail).toBeLessThan(200);
			// Each character is re-parsed a bounded number of times in total…
			expect(reparsed).toBeLessThan(full.length * 3);
			// …versus re-parsing the whole document on every tick, today's cost.
			const wholeDocumentEachTick = ticks.reduce((n, t) => n + t.length, 0);
			expect(reparsed).toBeLessThan(wholeDocumentEachTick / 10);
		});

		it("loses no source text across the split", () => {
			let split: StreamSplit | undefined;
			for (let i = 1; i <= 50; i++) split = splitStream(paragraphs(i), split);
			const s = split as StreamSplit;
			expect(s.committed.map((seg) => seg.text).join("") + s.tail.text).toBe(paragraphs(50));
		});
	});
});

/**
 * Findings from an independent adversarial review (codex) of the first cut.
 * Each case below is a markdown construct where a committed boundary rendered
 * something the whole document does not — the one failure mode this module
 * exists to prevent. They are grouped separately so the origin stays visible.
 */
describe("splitStream divergences found by adversarial review", () => {
	const count = (haystack: string, needle: string) => haystack.split(needle).length - 1;

	describe("multi-line HTML comments", () => {
		/** Everything up to `-->` is hidden in the whole document. Cutting on the
		 *  blank line inside the comment makes the hidden half visible forever. */
		it("never cuts inside an open comment", () => {
			const src = "<!--\nhidden\n\nstill hidden\n-->\n\nVisible.\n\nTail.\n";
			const split = splitStream(src);

			for (const seg of split.committed) {
				expect(count(seg.text, "<!--")).toBe(count(seg.text, "-->"));
			}
			// Committing the comment whole is fine — it hides its content either
			// way. What must never happen is a segment that opens it and stops.
			const opener = split.committed.find((s) => s.text.includes("<!--"));
			if (opener) expect(opener.text).toContain("-->");
		});

		it("still commits past a comment that closes on its own line", () => {
			const src = "<!-- a note -->\n\nFirst.\n\nSecond.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});
	});

	describe("fence delimiters", () => {
		/** CommonMark closes a fence only with the same character, at least as
		 *  long. A blind toggle thinks this code block ended and commits prose
		 *  that the whole document renders as code. */
		it("does not treat a shorter fence as a close", () => {
			const src = "````js\nconst x = 1;\n```\n\nAfter the fence.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed).toEqual([]);
		});

		it("does not treat a different fence character as a close", () => {
			const src = "```js\nconst x = 1;\n~~~\n\nAfter the fence.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed).toEqual([]);
		});

		it("still closes on a longer fence of the same character", () => {
			const src = "```js\nconst x = 1;\n`````\n\nAfter the fence.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});
	});

	describe("reference-link definitions", () => {
		/** A definition resolves usages ANYWHERE in the document, including text
		 *  committed long before it arrived. A document that carries one cannot be
		 *  split at all — the shortcut `[docs]` form makes any bracket suspect. */
		it("commits nothing once a definition exists", () => {
			const src = "See [docs].\n\nMore text.\n\n[docs]: https://example.com\n";
			const split = splitStream(src);

			expect(split.committed).toEqual([]);
		});

		it("ignores a definition that is only inside a fence", () => {
			const src = "First.\n\n~~~text\n[d]: /fake\n~~~\n\nSecond.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});

		it("commits normally when no definition exists anywhere", () => {
			// With no definition in the document these render as literal brackets
			// whether parsed alone or together, so the cut is safe.
			const src = "See [text][label].\n\nSecond.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});

		it("does not mistake a task list for a reference", () => {
			const src = "- [x] done\n- [ ] todo\n\nSecond.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});
	});

	describe("resume against a replaced source", () => {
		/** A boundary is decided by looking at the first line of the NEXT block.
		 *  A replacement that shares the committed prefix but changes that line
		 *  invalidates the decision, so the prefix alone cannot license a resume. */
		it("re-scans when the line that decided the last cut changed", () => {
			const first = splitStream("- one\n\nAfter.\n");
			expect(first.committed.length).toBeGreaterThan(0);

			// Same prefix, but the next block is now another item of the same list:
			// the whole document renders ONE loose list, not a list plus a list.
			const second = splitStream("- one\n\n- two\n", first);

			expect(second.committed).toEqual([]);
		});

		it("still resumes a true append", () => {
			const first = splitStream("- one\n\nAfter.\n");
			const second = splitStream("- one\n\nAfter.\n\nMore.\n", first);

			expect(second.committed[0]).toBe(first.committed[0]);
		});
	});

	describe("void HTML tags", () => {
		/** `<br>` has no closing tag, so counting it as an unclosed open makes
		 *  every later candidate fail — the tail grows without bound and the
		 *  quadratic cost this module removes comes straight back. */
		it("does not treat a void element as an unclosed tag", () => {
			const src = "First.\n\n<br>\n\nSecond.\n\nThird.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});

		it("does not treat a self-closing tag as an unclosed tag", () => {
			const src = "First.\n\n<img src='a.png' />\n\nSecond.\n\nThird.\n\nTail.\n";
			const split = splitStream(src);

			expect(split.committed.length).toBeGreaterThan(0);
		});

		it("still refuses a genuinely unclosed container", () => {
			const src = "First.\n\n<div>\n\nSecond.\n\nTail.\n";
			const split = splitStream(src);

			for (const seg of split.committed) {
				expect(seg.text.includes("<div")).toBe(false);
			}
		});
	});

	describe("stray tweak sentinels", () => {
		/** An unmatched end drove the counter negative, so the next real begin
		 *  brought it back to zero and a cut landed between a begin and its end. */
		it("keeps a begin/end pair in one segment after a stray end", () => {
			const src =
				"<!--tweak:end:stray-->\n\n<!--tweak:begin:a-->\nfirst paragraph\n\nsecond paragraph\n<!--tweak:end:a @0 note-->\n\nTail.\n";
			const split = splitStream(src);

			for (const seg of split.committed) {
				expect(count(seg.text, "<!--tweak:begin:")).toBe(count(seg.text, "<!--tweak:end:a"));
			}
		});
	});
});
