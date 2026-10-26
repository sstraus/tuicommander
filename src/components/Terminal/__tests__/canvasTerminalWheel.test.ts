import { describe, expect, it } from "vitest";
import {
	createWheelNotchState,
	quantizeWheelNotches,
	resetWheelNotch,
	WHEEL_MAX_NOTCHES_PER_EVENT,
	type WheelDeltaLike,
	wheelDeltaToPixels,
	wheelScrollDelta,
} from "../canvasTerminalWheel";

const CELL_HEIGHT = 17;
const ROWS = 24;

function wheel(overrides: Partial<WheelDeltaLike>): WheelDeltaLike {
	return { deltaX: 0, deltaY: 0, deltaMode: 0, shiftKey: false, ...overrides };
}

describe("wheelScrollDelta", () => {
	it("uses deltaY by default", () => {
		expect(wheelScrollDelta(wheel({ deltaY: 12 }))).toBe(12);
	});

	it("falls back to deltaX only when Shift is held and it dominates", () => {
		expect(wheelScrollDelta(wheel({ shiftKey: true, deltaX: 40, deltaY: 0 }))).toBe(40);
	});

	it("ignores deltaX when Shift is not held, even if deltaY is 0", () => {
		expect(wheelScrollDelta(wheel({ shiftKey: false, deltaX: 40, deltaY: 0 }))).toBe(0);
	});

	it("prefers deltaY over deltaX under Shift when deltaY dominates", () => {
		expect(wheelScrollDelta(wheel({ shiftKey: true, deltaX: 2, deltaY: 30 }))).toBe(30);
	});
});

describe("wheelDeltaToPixels", () => {
	it("passes pixel mode through unchanged", () => {
		expect(wheelDeltaToPixels(wheel({ deltaY: 12 }), CELL_HEIGHT, ROWS)).toBe(12);
	});

	it("scales line mode by cell height", () => {
		expect(wheelDeltaToPixels(wheel({ deltaY: 3, deltaMode: 1 }), CELL_HEIGHT, ROWS)).toBe(51);
	});

	it("scales page mode by cell height times viewport rows", () => {
		expect(wheelDeltaToPixels(wheel({ deltaY: 1, deltaMode: 2 }), CELL_HEIGHT, ROWS)).toBe(408);
	});
});

describe("quantizeWheelNotches", () => {
	it("collapses a macOS momentum-tail burst into a handful of notches, not one per event", () => {
		const state = createWheelNotchState();
		let total = 0;
		for (let i = 0; i < 10; i++) total += quantizeWheelNotches(state, wheel({ deltaY: 2 }), CELL_HEIGHT, ROWS);
		expect(total).toBe(1);
	});

	it("conserves pixels across a stream within rounding", () => {
		const state = createWheelNotchState();
		const deltas = [5, 3, 8, 2, 6, 4, 9, 1, 7, 3];
		let total = 0;
		let px = 0;
		for (const d of deltas) {
			total += quantizeWheelNotches(state, wheel({ deltaY: d }), CELL_HEIGHT, ROWS);
			px += d;
		}
		expect(Math.abs(total - Math.trunc(px / CELL_HEIGHT))).toBeLessThanOrEqual(1);
	});

	it("stays quiet under one notch of travel and carries the residual", () => {
		const state = createWheelNotchState();
		expect(quantizeWheelNotches(state, wheel({ deltaY: 5 }), CELL_HEIGHT, ROWS)).toBe(0);
		expect(quantizeWheelNotches(state, wheel({ deltaY: 13 }), CELL_HEIGHT, ROWS)).toBe(1);
	});

	it("drops the residual on direction reversal instead of banking it", () => {
		const state = createWheelNotchState();
		expect(quantizeWheelNotches(state, wheel({ deltaY: 10 }), CELL_HEIGHT, ROWS)).toBe(0);
		// Banking would require -30px (10 banked + 20) to emit a notch; reversal reset
		// means -20px alone is enough.
		expect(quantizeWheelNotches(state, wheel({ deltaY: -20 }), CELL_HEIGHT, ROWS)).toBe(-1);
	});

	it("caps notches per event and discards the clamped excess", () => {
		const state = createWheelNotchState();
		expect(quantizeWheelNotches(state, wheel({ deltaY: 500 }), CELL_HEIGHT, ROWS)).toBe(WHEEL_MAX_NOTCHES_PER_EVENT);
		// If the excess had been banked, this would immediately emit more notches.
		expect(quantizeWheelNotches(state, wheel({ deltaY: 5 }), CELL_HEIGHT, ROWS)).toBe(0);
	});

	it("emits exactly the cap for a steady discrete-mouse stream, never more", () => {
		const state = createWheelNotchState();
		for (let i = 0; i < 5; i++) {
			expect(quantizeWheelNotches(state, wheel({ deltaY: 100 }), CELL_HEIGHT, ROWS)).toBe(WHEEL_MAX_NOTCHES_PER_EVENT);
		}
	});

	it("normalizes DOM_DELTA_LINE and clamps to the viewport", () => {
		const state = createWheelNotchState();
		expect(quantizeWheelNotches(state, wheel({ deltaY: 3, deltaMode: 1 }), CELL_HEIGHT, ROWS)).toBe(3);
		expect(quantizeWheelNotches(state, wheel({ deltaY: 100, deltaMode: 1 }), CELL_HEIGHT, ROWS)).toBe(ROWS);
	});

	it("normalizes DOM_DELTA_PAGE to one viewport minus a row", () => {
		const state = createWheelNotchState();
		expect(quantizeWheelNotches(state, wheel({ deltaY: -1, deltaMode: 2 }), CELL_HEIGHT, ROWS)).toBe(-23);
	});

	it("clears the pixel residual when switching delta modes mid-gesture", () => {
		const state = createWheelNotchState();
		quantizeWheelNotches(state, wheel({ deltaY: 10 }), CELL_HEIGHT, ROWS);
		quantizeWheelNotches(state, wheel({ deltaY: 3, deltaMode: 1 }), CELL_HEIGHT, ROWS);
		expect(state.residualPx).toBe(0);
	});

	it("resetWheelNotch zeroes residual and sign", () => {
		const state = createWheelNotchState();
		quantizeWheelNotches(state, wheel({ deltaY: 10 }), CELL_HEIGHT, ROWS);
		resetWheelNotch(state);
		expect(state.residualPx).toBe(0);
		expect(state.lastSign).toBe(0);
	});
});
