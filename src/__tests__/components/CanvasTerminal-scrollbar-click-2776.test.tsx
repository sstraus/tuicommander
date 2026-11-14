import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

/**
 * A click on the scrollbar track moves the view to the spot under the pointer
 * (#2776-d2f2).
 *
 * The oracle is what the user sees: after the click, the backend scrolls, the
 * next frame repaints the thumb, and that thumb is centred under the pointer
 * (clamped to the track ends). The thumb is the viewport's picture, so with a
 * proportional thumb this is also "the viewport centre sits at the clicked
 * fraction of the content". The fake backend answers the scroll request the
 * way the grid does: anchor = all-time top row, offset = origin - anchor.
 */

const { invoke, frameSink } = vi.hoisted(() => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	frameSink: { current: null as ((data: ArrayBuffer) => void) | null },
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: vi.fn(async (onFrame: (data: ArrayBuffer) => void) => {
			frameSink.current = onFrame;
		}),
		resubscribe: vi.fn().mockResolvedValue(undefined),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 4;
const COLS = 8;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;
const PANE_H = 1000;

function fullFrame(historySize: number, displayOffset: number): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint32(7, displayOffset, true);
	view.setUint32(11, historySize, true);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	view.setUint32(22, 0, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS, true);
		offset += 4 + COLS * CELL_SIZE;
	}
	return buffer;
}

function thumbGeometry(thumb: HTMLElement) {
	const top = Number(/translateY\(([-\d.]+)px\)/.exec(thumb.style.transform)?.[1]);
	return { top, height: Number.parseFloat(thumb.style.height) };
}

async function mountWithHistory(sessionId: string, history: number) {
	invoke.mockImplementation(async (cmd: string, args: { anchor: number | null }) => {
		// historyBase is 0, so the all-time origin is the history size.
		if (cmd === "terminal_scroll_to_anchor")
			frameSink.current?.(fullFrame(history, args.anchor == null ? 0 : history - args.anchor));
	});
	const view = render(() => <CanvasTerminal sessionId={sessionId} terminalId={sessionId} />);
	await waitFor(() => expect(frameSink.current).not.toBeNull());
	frameSink.current?.(fullFrame(history, 0));
	const thumb = view.container.querySelector<HTMLElement>("div[style*='border-radius']");
	const track = thumb?.parentElement;
	if (!thumb || !track) throw new Error("scrollbar not mounted");
	await waitFor(() => expect(thumb.style.transform).toMatch(/translateY/));
	// Following output: the thumb rests at the bottom, so it spans to the track end.
	const live = thumbGeometry(thumb);
	return { view, thumb, track, trackH: live.top + live.height };
}

async function clickTrackAt(track: HTMLElement, thumb: HTMLElement, y: number) {
	const before = thumb.style.transform;
	invoke.mockClear();
	track.dispatchEvent(new MouseEvent("mousedown", { clientY: y, bubbles: true, cancelable: true }));
	await waitFor(() => expect(invoke.mock.calls.some(([cmd]) => cmd === "terminal_scroll_to_anchor")).toBe(true));
	await waitFor(() => expect(thumb.style.transform).not.toBe(before));
	return thumbGeometry(thumb);
}

describe("CanvasTerminal scrollbar track click (#2776-d2f2)", () => {
	beforeEach(() => {
		invoke.mockReset().mockResolvedValue(undefined);
		frameSink.current = null;
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					get: (_t, prop) =>
						prop === "measureText"
							? () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
							: vi.fn(),
				},
			) as unknown as CanvasRenderingContext2D,
		);
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			width: 800,
			height: PANE_H,
			top: 0,
			left: 0,
			right: 800,
			bottom: PANE_H,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		vi.stubGlobal(
			"ResizeObserver",
			class {
				observe() {}
				disconnect() {}
			},
		);
		vi.stubGlobal("IntersectionObserver", VisibleIntersectionObserver);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// catches: the click fraction is taken over the whole track instead of the
	// thumb's travel, so a short scrollback (big thumb) lands far from the pointer.
	it("centres the thumb under the pointer with a short scrollback", async () => {
		const { view, thumb, track, trackH } = await mountWithHistory("click-short", 60);
		try {
			const before = thumbGeometry(thumb);
			expect(before.height).toBeGreaterThan(trackH / 4); // a big, proportional thumb
			const y = trackH * 0.4;
			const after = await clickTrackAt(track, thumb, y);
			// One history row of thumb travel is the finest step the grid can take.
			const step = (trackH - after.height) / 60;
			expect(Math.abs(after.top + after.height / 2 - y)).toBeLessThanOrEqual(step);
		} finally {
			view.unmount();
		}
	});

	// catches: the same full-track mapping with a long scrollback and the 48px floor.
	it("centres the thumb under the pointer with a long scrollback", async () => {
		const { view, thumb, track, trackH } = await mountWithHistory("click-long", 100_000);
		try {
			const y = trackH * 0.1;
			const after = await clickTrackAt(track, thumb, y);
			expect(Math.abs(after.top + after.height / 2 - y)).toBeLessThanOrEqual(1);
		} finally {
			view.unmount();
		}
	});

	// catches: a click near a track end that overshoots or stops short of the
	// oldest row / the live bottom.
	it("clamps to the oldest row above half a thumb and to the live tail below it", async () => {
		const { view, thumb, track, trackH } = await mountWithHistory("click-ends", 60);
		try {
			const height = thumbGeometry(thumb).height;
			const top = await clickTrackAt(track, thumb, height / 4);
			expect(top.top).toBe(0);
			const bottom = await clickTrackAt(track, thumb, trackH - height / 4);
			expect(bottom.top + bottom.height).toBeCloseTo(trackH, 5);
			expect(invoke.mock.calls.at(-1)?.[1]).toMatchObject({ anchor: null });
		} finally {
			view.unmount();
		}
	});
});
