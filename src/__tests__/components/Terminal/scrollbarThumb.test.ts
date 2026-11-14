import { describe, expect, it } from "vitest";
import { MIN_THUMB_PX, scrollbarThumb, trackClickOffset } from "../../../components/Terminal/scrollbarThumb";

describe("scrollbarThumb", () => {
	it("sizes the thumb in proportion to the visible share of a short history", () => {
		// 50 visible of 100 total on a 400px track: half the track.
		const t = scrollbarThumb({ trackH: 400, visibleRows: 50, historySize: 50, displayOffset: 0 });
		expect(t.height).toBe(200);
		expect(t.range).toBe(200);
	});

	it("keeps the thumb large enough to grab when the history is very long", () => {
		// 40 visible of 100k rows is 0.4px proportionally: the regression was a 20px sliver.
		const t = scrollbarThumb({ trackH: 800, visibleRows: 40, historySize: 100_000, displayOffset: 0 });
		expect(t.height).toBe(MIN_THUMB_PX);
		expect(MIN_THUMB_PX).toBeGreaterThanOrEqual(40);
	});

	it("never grows past a track shorter than the minimum", () => {
		const t = scrollbarThumb({ trackH: 30, visibleRows: 2, historySize: 10_000, displayOffset: 0 });
		expect(t.height).toBe(30);
		expect(t.range).toBe(0);
		expect(t.top).toBe(0);
	});

	it("puts the thumb at the bottom when following output and at the top when fully scrolled back", () => {
		const input = { trackH: 800, visibleRows: 40, historySize: 100_000 };
		const live = scrollbarThumb({ ...input, displayOffset: 0 });
		const oldest = scrollbarThumb({ ...input, displayOffset: 100_000 });
		expect(live.top).toBe(800 - MIN_THUMB_PX);
		expect(oldest.top).toBe(0);
	});

	// catches: a pane shorter than the thumb floor has no travel range, and the
	// click mapping divides by it, sending a NaN offset to the backend.
	it("maps a track click on a pane with no thumb travel to a track end", () => {
		const thumb = scrollbarThumb({ trackH: 30, visibleRows: 2, historySize: 10_000, displayOffset: 0 });
		expect(trackClickOffset(thumb, 5, 10_000)).toBe(10_000);
		expect(trackClickOffset(thumb, 25, 10_000)).toBe(0);
	});
});
