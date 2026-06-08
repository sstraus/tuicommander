import { describe, expect, it } from "vitest";

import { nextScrollOffset } from "../scrollCoalesce";

describe("nextScrollOffset", () => {
	it("scrolls up into history (negative lines raise the offset)", () => {
		expect(nextScrollOffset(0, -3, 100)).toBe(3);
	});

	it("scrolls down toward the bottom (positive lines lower the offset)", () => {
		expect(nextScrollOffset(10, 4, 100)).toBe(6);
	});

	it("clamps at the bottom (offset never goes below 0)", () => {
		expect(nextScrollOffset(2, 5, 100)).toBe(0);
	});

	it("clamps at the top (offset never exceeds historySize)", () => {
		expect(nextScrollOffset(98, -10, 100)).toBe(100);
	});

	it("treats historySize 0 as pinned to the bottom", () => {
		expect(nextScrollOffset(0, -5, 0)).toBe(0);
	});
});
