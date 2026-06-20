import { describe, expect, it } from "vitest";
import { isBackgroundTimerGap } from "../../utils/freezeDetector";

describe("isBackgroundTimerGap", () => {
	it("reports a genuine main-thread block when focused, visible, sub-sleep", () => {
		// 1000ms gap while focused+visible IS a real freeze — must NOT be skipped.
		expect(isBackgroundTimerGap(1000, false, true)).toBe(false);
	});

	it("skips the App Nap clamp: visible but unfocused window (the phantom-freeze flood)", () => {
		// document.hidden stays false, but the window is not the key window → 1Hz clamp.
		expect(isBackgroundTimerGap(1000, false, false)).toBe(true);
	});

	it("skips when the page is hidden (timers clamped to ~1Hz)", () => {
		expect(isBackgroundTimerGap(1000, true, false)).toBe(true);
	});

	it("skips sleep/suspend gaps even when focused and visible", () => {
		expect(isBackgroundTimerGap(60_000, false, true)).toBe(true);
	});

	it("does not skip a normal short tick while focused", () => {
		expect(isBackgroundTimerGap(55, false, true)).toBe(false);
	});
});
