import { describe, expect, it, vi } from "vitest";
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

describe("freeze detector lifecycle", () => {
	// Catches: setPerfDebug(false) leaving the detector running (it never stopped),
	// and a paint-jank rAF loop re-arming every frame while perfDebug is on.
	it("starts one timer and no animation-frame loop, and stopFreezeDetector clears the timer", async () => {
		vi.resetModules();
		vi.useFakeTimers();
		vi.doMock("../../utils/perfDebug", () => ({ isPerfDebug: () => true }));
		vi.doMock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), diag: { warn: vi.fn() } } }));
		const raf = vi.fn();
		vi.stubGlobal("requestAnimationFrame", raf);
		const { startFreezeDetector, stopFreezeDetector } = await import("../../utils/freezeDetector");
		startFreezeDetector();
		expect(vi.getTimerCount()).toBe(1);
		expect(raf).not.toHaveBeenCalled();
		stopFreezeDetector();
		expect(vi.getTimerCount()).toBe(0);
		startFreezeDetector();
		expect(vi.getTimerCount()).toBe(1);
		vi.useRealTimers();
		vi.unstubAllGlobals();
		vi.doUnmock("../../utils/perfDebug");
		vi.doUnmock("../../stores/appLogger");
	});
});
