import { describe, expect, it } from "vitest";
import { diffLineCount, isDiffTooLarge, LARGE_DIFF_LINES } from "../../components/DiffTab/diffSize";

describe("diffSize", () => {
	it("counts 0 lines for empty input", () => {
		expect(diffLineCount("")).toBe(0);
	});

	it("counts newline-separated lines", () => {
		expect(diffLineCount("a\nb\nc")).toBe(3);
	});

	it("is not too large exactly at the threshold", () => {
		// LARGE_DIFF_LINES lines → not over the threshold (> is strict).
		const atThreshold = Array.from({ length: LARGE_DIFF_LINES }, () => "x").join("\n");
		expect(diffLineCount(atThreshold)).toBe(LARGE_DIFF_LINES);
		expect(isDiffTooLarge(atThreshold)).toBe(false);
	});

	it("is too large one line over the threshold", () => {
		const overThreshold = Array.from({ length: LARGE_DIFF_LINES + 1 }, () => "x").join("\n");
		expect(diffLineCount(overThreshold)).toBe(LARGE_DIFF_LINES + 1);
		expect(isDiffTooLarge(overThreshold)).toBe(true);
	});

	it("empty diff is never too large", () => {
		expect(isDiffTooLarge("")).toBe(false);
	});
});
