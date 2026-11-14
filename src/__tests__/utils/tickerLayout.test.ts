import { describe, expect, it } from "vitest";
import { countSideBySide, ROTATING_SLOT_RESERVE_PX } from "../../utils/tickerLayout";

describe("countSideBySide", () => {
	const widths = [200, 180, 160];
	const gap = 10;
	const total = 200 + 180 + 160 + 2 * gap;

	it("shows everything side by side at exactly the total width", () => {
		expect(countSideBySide(widths, total, gap)).toBe(3);
	});

	it("rotates the last one when one pixel short of fitting all — catches off-by-one in the fit test", () => {
		expect(countSideBySide(widths, total - 1, gap)).toBe(2);
	});

	it("keeps the slot reserve free when pinning — catches pinning that leaves no room for the rotating slot", () => {
		// Two pinned = 200 + 180 + 2 gaps + reserve
		const needed = 200 + 180 + 2 * gap + ROTATING_SLOT_RESERVE_PX;
		expect(countSideBySide(widths, needed, gap)).toBe(2);
		expect(countSideBySide(widths, needed - 1, gap)).toBe(1);
	});

	it("rotates everything when not even one ticker leaves the reserve", () => {
		expect(countSideBySide(widths, 200 + ROTATING_SLOT_RESERVE_PX - 1, gap)).toBe(0);
	});
});
