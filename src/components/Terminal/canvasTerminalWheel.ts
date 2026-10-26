/**
 * Pixel-to-notch quantization for the terminal wheel handler.
 *
 * The problem this fixes: a native terminal accumulates wheel pixels and emits one
 * scroll "notch" per line height. TUIC's forward-to-app branch (see CanvasTerminal.tsx
 * handleWheel) used to emit exactly one SGR notch per DOM wheel event, ignoring
 * deltaY's magnitude entirely. macOS delivers a wheel event for the whole inertial
 * momentum tail after a flick — 60-120 events/s of exponentially decaying deltas — so
 * one physical gesture turned into dozens or hundreds of notches sent to the app. This
 * module accumulates pixels per gesture and only emits whole notches, the same way a
 * native terminal does.
 */

export interface WheelDeltaLike {
	readonly deltaX: number;
	readonly deltaY: number;
	/** 0 = DOM_DELTA_PIXEL, 1 = DOM_DELTA_LINE, 2 = DOM_DELTA_PAGE */
	readonly deltaMode: number;
	readonly shiftKey: boolean;
}

/** Per-gesture accumulator. One instance lives for the lifetime of a terminal pane. */
export interface WheelNotchState {
	/** Signed sub-notch pixels carried across events within one gesture. */
	residualPx: number;
	/** Sign of the last non-zero delta (+1 down, -1 up, 0 = idle). */
	lastSign: number;
}

/**
 * Max notches a single DOM wheel event may emit in pixel mode. 3 is the near-universal
 * native "lines per wheel click" (VTE, iTerm2, Ghostty, Windows default) — it is also
 * what makes a discrete-mouse browser's ~100px-per-click land on 3 notches instead of
 * ceil(100/17)=6.
 */
export const WHEEL_MAX_NOTCHES_PER_EVENT = 3;

/** Gesture-idle window after which the residual is discarded (matches the existing scroll-gesture timer). */
export const WHEEL_GESTURE_END_MS = 200;

export function createWheelNotchState(): WheelNotchState {
	return { residualPx: 0, lastSign: 0 };
}

export function resetWheelNotch(state: WheelNotchState): void {
	state.residualPx = 0;
	state.lastSign = 0;
}

/**
 * The vertical scroll intent of a wheel event, correcting for macOS/WebKit swapping
 * Shift+wheel onto the horizontal axis. A discrete mouse's Shift+wheel reports its
 * delta on deltaX with deltaY == 0 — exactly the gesture the docs call the scrollback
 * escape hatch — so without this it silently does nothing until Shift is released.
 * Only consulted when Shift is held, so a genuine horizontal trackpad swipe (Shift not
 * held) never scrolls the terminal vertically.
 */
export function wheelScrollDelta(e: WheelDeltaLike): number {
	return e.shiftKey && Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
}

/**
 * Normalize a wheel event's vertical delta to pixels. DOM_DELTA_LINE is scaled by
 * cellHeightPx (one "line" per Chrome/Firefox is one terminal row's worth of pixels
 * for our purposes); DOM_DELTA_PAGE by one viewport. Pixel mode passes through.
 */
export function wheelDeltaToPixels(e: WheelDeltaLike, cellHeightPx: number, viewportRows: number): number {
	const dy = wheelScrollDelta(e);
	if (e.deltaMode === 1) return dy * cellHeightPx;
	if (e.deltaMode === 2) return dy * cellHeightPx * viewportRows;
	return dy;
}

/**
 * Quantize one wheel event into whole notches for SGR mouse forwarding.
 *
 * Returns a signed notch count: negative = scroll up (SGR button 64), positive =
 * scroll down (button 65), 0 = emit nothing for this event. Mutates `state` in place.
 */
export function quantizeWheelNotches(
	state: WheelNotchState,
	e: WheelDeltaLike,
	cellHeightPx: number,
	viewportRows: number,
): number {
	const dy = wheelScrollDelta(e);

	// Line/page deltas are already quantized by the OS or browser — running them
	// through the pixel accumulator would be a lossy round-trip. Reset first so a
	// device switch mid-gesture doesn't inherit stale pixel residual.
	if (e.deltaMode === 1) {
		resetWheelNotch(state);
		const rows = Math.max(1, viewportRows);
		return Math.max(-rows, Math.min(rows, Math.round(dy)));
	}
	if (e.deltaMode === 2) {
		resetWheelNotch(state);
		if (dy === 0) return 0;
		return Math.sign(dy) * Math.max(1, viewportRows - 1);
	}

	if (dy === 0) return 0;
	const sign = dy > 0 ? 1 : -1;
	// A direction reversal restarts the ramp — otherwise a scroll-back gesture has to
	// pay back the outbound gesture's banked pixels before it visibly moves.
	if (state.lastSign !== 0 && sign !== state.lastSign) state.residualPx = 0;
	state.lastSign = sign;
	state.residualPx += dy;

	const notchPx = Math.max(1, cellHeightPx);
	let notches = Math.trunc(state.residualPx / notchPx);
	state.residualPx -= notches * notchPx;

	if (Math.abs(notches) > WHEEL_MAX_NOTCHES_PER_EVENT) {
		notches = Math.sign(notches) * WHEEL_MAX_NOTCHES_PER_EVENT;
		// Never bank the clamped excess — banking would turn a steady stream of
		// discrete-mouse events into a runaway (each event queues more than it emits).
		state.residualPx = 0;
	}

	return notches;
}
