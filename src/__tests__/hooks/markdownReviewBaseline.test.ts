import { expect, it } from "vitest";
import { createMarkdownReviewBaseline } from "../../hooks/markdownReviewBaseline";

// Catches: acknowledging an older send discards edits saved while it was pending.
it("rebases only acknowledged content while keeping later edits", () => {
	const review = createMarkdownReviewBaseline();
	review.load("before");
	review.save("before", "first save");
	const sent = review.capture();
	review.save("first save", "second save");
	review.acknowledge("first save", sent.epoch);
	expect(review.capture().baseline).toBe("first save");
	review.acknowledge("second save", review.capture().epoch);
	expect(review.capture().baseline).toBeNull();
	review.save("second save", "third save");
	expect(review.capture().baseline).toBe("second save");
});

// Catches: an external reload followed by another save narrows away earlier unsent edits.
it("keeps an externally invalidated review broad until it is acknowledged", () => {
	const review = createMarkdownReviewBaseline();
	review.load("before");
	review.save("before", "first save");
	const sent = review.capture();
	review.load("external");
	review.save("external", "later save");
	expect(review.capture().baseline).toBeNull();
	review.acknowledge("first save", sent.epoch);
	expect(review.capture().baseline).toBeNull();
});

// Catches: watcher echoes of a successful save invalidate an otherwise reliable baseline.
it("retains the first baseline across unchanged disk reloads and multiple saves", () => {
	const review = createMarkdownReviewBaseline();
	review.load("before");
	expect(review.capture().baseline).toBeNull();
	review.save("before", "first save");
	review.load("first save");
	review.save("first save", "second save");
	expect(review.capture().baseline).toBe("before");
});
