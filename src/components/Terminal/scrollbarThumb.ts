/**
 * Geometry of the terminal scrollbar thumb, shared by the paint path and the
 * drag handler so both agree on the same height and travel range.
 *
 * The thumb is proportional to the visible share of the scrollback, but never
 * smaller than `MIN_THUMB_PX`: a long history otherwise shrinks it to a sliver
 * that is hard to grab. It is also never taller than the track itself, so the
 * travel range can not go negative on a very short pane.
 */

/** Smallest thumb height in CSS pixels, so a long history stays grabbable. */
export const MIN_THUMB_PX = 48;

export interface ScrollbarThumbInput {
	/** Track height in CSS pixels. */
	trackH: number;
	/** Rows on screen. */
	visibleRows: number;
	/** Rows in the scrollback above the screen. */
	historySize: number;
	/** Rows scrolled back from the live bottom (0 = following output). */
	displayOffset: number;
}

export interface ScrollbarThumb {
	/** Thumb height in CSS pixels. */
	height: number;
	/** Pixels the thumb can travel: track height minus thumb height. */
	range: number;
	/** Thumb top offset inside the track in CSS pixels. */
	top: number;
}

export function scrollbarThumb({
	trackH,
	visibleRows,
	historySize,
	displayOffset,
}: ScrollbarThumbInput): ScrollbarThumb {
	const ratio = Math.min(1, visibleRows / (historySize + visibleRows));
	const height = Math.min(trackH, Math.max(MIN_THUMB_PX, trackH * ratio));
	const range = trackH - height;
	const top = historySize > 0 ? (1 - displayOffset / historySize) * range : range;
	return { height, range, top };
}

/**
 * Display offset that puts the thumb centre under a click at `clickY` (CSS
 * pixels from the track top), clamped to the track ends. The inverse of the
 * `top` formula above, so the click and the paint use the same travel range:
 * mapping the click over the whole track instead put the thumb up to its own
 * height away from the pointer (#2776-d2f2).
 */
export function trackClickOffset(thumb: ScrollbarThumb, clickY: number, historySize: number): number {
	if (thumb.range <= 0) return clickY < thumb.height / 2 ? historySize : 0;
	const top = Math.max(0, Math.min(thumb.range, clickY - thumb.height / 2));
	return Math.round((1 - top / thumb.range) * historySize);
}
