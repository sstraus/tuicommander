import { beforeEach, describe, expect, it } from "vitest";

import {
	FRAME_TIMING_CAPACITY,
	getFrameTimingStats,
	installFrameTimingDebugHook,
	isFrameTimingEnabled,
	onFrameTimingEnabledChange,
	recordFrameTiming,
	resetFrameTiming,
	setFrameTimingEnabled,
} from "../frameTiming";

describe("frameTiming ring buffer", () => {
	beforeEach(() => {
		resetFrameTiming();
		setFrameTimingEnabled(false);
	});

	it("returns zeroed stats for an unknown session", () => {
		const stats = getFrameTimingStats("nope");
		expect(stats.decode.count).toBe(0);
		expect(stats.decode.p50).toBe(0);
		expect(stats.decode.p95).toBe(0);
		expect(stats.decode.max).toBe(0);
		expect(stats.paint.count).toBe(0);
	});

	it("computes nearest-rank percentiles and max for decode samples", () => {
		for (let v = 1; v <= 100; v++) {
			recordFrameTiming("s1", "decode", v);
		}
		const { decode } = getFrameTimingStats("s1");
		expect(decode.count).toBe(100);
		// nearest-rank: p50 -> ceil(0.50*100)=50 -> sorted[49] = 50
		expect(decode.p50).toBe(50);
		// p95 -> ceil(0.95*100)=95 -> sorted[94] = 95
		expect(decode.p95).toBe(95);
		expect(decode.max).toBe(100);
	});

	it("tracks decode, paint and sched metrics independently per session", () => {
		recordFrameTiming("a", "decode", 10);
		recordFrameTiming("a", "paint", 20);
		recordFrameTiming("a", "sched", 200);
		recordFrameTiming("b", "decode", 99);

		const a = getFrameTimingStats("a");
		expect(a.decode.count).toBe(1);
		expect(a.decode.max).toBe(10);
		expect(a.paint.count).toBe(1);
		expect(a.paint.max).toBe(20);
		// sched (worker schedule→paint delay) is a first-class, independent metric.
		expect(a.sched.count).toBe(1);
		expect(a.sched.max).toBe(200);

		const b = getFrameTimingStats("b");
		expect(b.decode.count).toBe(1);
		expect(b.decode.max).toBe(99);
		expect(b.paint.count).toBe(0);
		expect(b.sched.count).toBe(0);
	});

	it("bounds the ring buffer at FRAME_TIMING_CAPACITY, evicting oldest", () => {
		const total = FRAME_TIMING_CAPACITY + 50;
		for (let v = 1; v <= total; v++) {
			recordFrameTiming("ring", "paint", v);
		}
		const { paint } = getFrameTimingStats("ring");
		expect(paint.count).toBe(FRAME_TIMING_CAPACITY);
		// Oldest 50 evicted -> max is still the last value pushed.
		expect(paint.max).toBe(total);
	});

	it("resetFrameTiming(sessionId) clears only that session", () => {
		recordFrameTiming("keep", "decode", 5);
		recordFrameTiming("drop", "decode", 5);
		resetFrameTiming("drop");
		expect(getFrameTimingStats("keep").decode.count).toBe(1);
		expect(getFrameTimingStats("drop").decode.count).toBe(0);
	});
});

describe("frameTiming enable flag + debug hook", () => {
	beforeEach(() => {
		resetFrameTiming();
		setFrameTimingEnabled(false);
		delete (globalThis as Record<string, unknown>).__terminalFrameTiming;
	});

	it("is disabled by default and toggles", () => {
		expect(isFrameTimingEnabled()).toBe(false);
		setFrameTimingEnabled(true);
		expect(isFrameTimingEnabled()).toBe(true);
	});

	it("notifies registered listeners on enable change and stops after unregister", () => {
		const seen: boolean[] = [];
		const unregister = onFrameTimingEnabledChange((on) => seen.push(on));
		// This is what propagates the toggle into each worker-mode terminal's worker.
		setFrameTimingEnabled(true);
		setFrameTimingEnabled(false);
		expect(seen).toEqual([true, false]);

		unregister();
		setFrameTimingEnabled(true);
		expect(seen).toEqual([true, false]); // no further notifications after unregister
	});

	it("installs a debug hook exposing stats/enable/reset on globalThis", () => {
		installFrameTimingDebugHook();
		const hook = (globalThis as Record<string, unknown>).__terminalFrameTiming as {
			stats: (id: string) => unknown;
			enable: (on: boolean) => void;
			reset: (id?: string) => void;
		};
		expect(hook).toBeTruthy();

		hook.enable(true);
		expect(isFrameTimingEnabled()).toBe(true);

		recordFrameTiming("dbg", "decode", 7);
		expect(hook.stats("dbg")).toEqual(getFrameTimingStats("dbg"));

		hook.reset("dbg");
		expect(getFrameTimingStats("dbg").decode.count).toBe(0);
	});
});
