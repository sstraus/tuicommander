import { marked } from "marked";
import { beforeEach, describe, expect, it } from "vitest";
import { injectTweakSentinels, insertTweakComment, type TweakComment } from "../../utils/tweakComments";
import { applyTweakDomHighlights } from "../../utils/tweakDomHighlight";

/**
 * Mirror the ContentRenderer pipeline: insert a comment into raw markdown, replace
 * markers with sentinels, render with marked, then apply DOM highlights.
 */
function renderWithHighlight(source: string, comment: TweakComment): HTMLDivElement {
	const withComment = insertTweakComment(source, comment);
	const html = marked.parse(injectTweakSentinels(withComment), { async: false }) as string;
	const container = document.createElement("div");
	container.innerHTML = html;
	// parseTweakComments would re-read markers; we already have the comment, but its
	// `highlighted`/`createdAt` must match what was stored. Re-derive from the source.
	const stored = { ...comment, highlighted: "" }; // highlighted unused by the DOM applier
	applyTweakDomHighlights(container, [stored]);
	return container;
}

const COMMENT: TweakComment = {
	id: "c_1",
	highlighted: "", // set per-test via the rendered selection
	comment: "a note",
	createdAt: "2026-04-05T10:00:00.000Z",
};

describe("applyTweakDomHighlights", () => {
	let root: HTMLDivElement;
	beforeEach(() => {
		root = document.createElement("div");
	});

	it("wraps a plain-text selection in a single highlight span", () => {
		const container = renderWithHighlight("Hello world!", { ...COMMENT, highlighted: "world" });
		const spans = container.querySelectorAll(".tweak-highlight");
		expect(spans).toHaveLength(1);
		expect(spans[0].textContent).toBe("world");
		expect((spans[0] as HTMLElement).dataset["tweakId"]).toBe("c_1");
		expect((spans[0] as HTMLElement).dataset["tweakComment"]).toBe("a note");
	});

	it("leaves no sentinel characters in the rendered text", () => {
		const container = renderWithHighlight("Hello world!", { ...COMMENT, highlighted: "world" });
		expect(container.textContent?.trimEnd()).toBe("Hello world!");
		expect(container.textContent).not.toMatch(/[\uE000\uE001]/);
	});

	it("preserves bold formatting for a selection fully inside **bold**", () => {
		const container = renderWithHighlight("x **bold text** y", { ...COMMENT, highlighted: "bold text" });
		// Highlight is inside the <strong>, so formatting is preserved.
		const strong = container.querySelector("strong");
		expect(strong).not.toBeNull();
		expect(strong!.querySelector(".tweak-highlight")?.textContent).toBe("bold text");
	});

	it("renders a contiguous highlight that straddles a bold boundary (the real-world case)", () => {
		const src = "This repo is **analysis and coordination only**. It contains **no product code**. Its";
		const container = renderWithHighlight(src, { ...COMMENT, highlighted: "and coordination only. It contains no" });
		const spans = Array.from(container.querySelectorAll(".tweak-highlight"));
		// The visible highlighted text is contiguous and exactly the selection.
		const highlighted = spans.map((s) => s.textContent).join("");
		expect(highlighted).toBe("and coordination only. It contains no");
		// Bold formatting on both ends survives (the highlight did not destroy it).
		expect(container.querySelectorAll("strong").length).toBe(2);
		// No sentinels leaked, full visible text intact.
		expect(container.textContent?.trimEnd()).toBe(src.replace(/\*\*/g, ""));
	});

	it("is a no-op when the source has no comments", () => {
		root.innerHTML = marked.parse("# Title\n\nplain text", { async: false }) as string;
		applyTweakDomHighlights(root, []);
		expect(root.querySelectorAll(".tweak-highlight")).toHaveLength(0);
	});
});
