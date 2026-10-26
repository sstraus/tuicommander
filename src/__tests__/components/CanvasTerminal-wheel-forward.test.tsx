import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

/**
 * The wheel forwarded to an app that reports the mouse (Claude Code, vim, lazygit).
 *
 * The oracle is what a native terminal does: one SGR notch per line of travel,
 * never one per DOM event, and Shift+wheel is the escape hatch that keeps the
 * wheel for TUIC's own scrollback (macOS reports that gesture on deltaX).
 */

const { invoke, frameSink } = vi.hoisted(() => ({
	invoke: vi.fn(),
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

const ROWS = 24;
const COLS = 10;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;

/** A blank full screen with mouse reporting (SGR, button-event tracking) on unless `mouseMode` is 0. */
function frame(mouseMode: number, history = 0): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	if (history > 0) {
		view.setUint32(7, Math.floor(history / 2), true);
		view.setUint32(11, history, true);
	}
	view.setUint8(17, mouseMode === 0 ? 0 : (mouseMode << 3) | 0x20);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS, true);
		offset += 4 + COLS * CELL_SIZE;
	}
	return buffer;
}

const SGR_NOTCH = /\x1b\[<(64|65);[^;]+;[^M]+M/g;

const ptyWrites = () =>
	invoke.mock.calls.filter(([cmd]) => cmd === "write_pty").map(([, a]) => (a as { data: string }).data);

/** Notches forwarded to the app as `[up, down]` counts. */
function forwardedNotches(): [number, number] {
	const all = ptyWrites().join("");
	const hits = all.match(SGR_NOTCH) ?? [];
	return [hits.filter((h) => h.includes("<64;")).length, hits.filter((h) => h.includes("<65;")).length];
}

describe("CanvasTerminal wheel forwarded to a mouse-reporting app", () => {
	let canvas: Element;
	let unmount: () => void;

	function wheel(init: WheelEventInit) {
		const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, clientX: 20, clientY: 20, ...init });
		// jsdom's WheelEvent drops the modifier flags from its init dict.
		if (init.shiftKey) Object.defineProperty(e, "shiftKey", { value: true });
		canvas.dispatchEvent(e);
	}

	async function mount(mouseMode: number, history = 0) {
		const view = render(() => <CanvasTerminal sessionId="wheel-1" terminalId="wheel-1" />);
		unmount = view.unmount;
		await waitFor(() => expect(frameSink.current).not.toBeNull());
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame(mouseMode, history));
		const found = view.container.querySelector('canvas[tabindex="0"]');
		if (!found) throw new Error("terminal canvas not mounted");
		canvas = found;
		await new Promise((r) => setTimeout(r, 20));
		invoke.mockClear();
	}

	beforeEach(() => {
		invoke.mockReset();
		invoke.mockResolvedValue(null);
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
			height: 1000,
			top: 0,
			left: 0,
			right: 800,
			bottom: 1000,
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
		unmount();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// Catches: one SGR notch per DOM event, so a macOS momentum tail floods the app.
	it("turns a burst of tiny trackpad deltas into far fewer notches than events", async () => {
		await mount(2);
		for (let i = 0; i < 20; i++) wheel({ deltaY: 2 });
		const [up, down] = forwardedNotches();
		expect(up).toBe(0);
		expect(down).toBeGreaterThanOrEqual(1);
		expect(down).toBeLessThanOrEqual(4);
	});

	// Catches: a sub-line delta being forwarded (or rounded up to a notch) on its own.
	it("forwards nothing for a single sub-line delta", async () => {
		await mount(2);
		wheel({ deltaY: 1 });
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: the per-event cap being lost, so one discrete-mouse click scrolls the app a page.
	it("caps a discrete mouse click at three notches in one write", async () => {
		await mount(2);
		wheel({ deltaY: -1000 });
		expect(forwardedNotches()).toEqual([3, 0]);
		expect(ptyWrites()).toHaveLength(1);
	});

	// Catches: line-mode devices being run through the pixel accumulator.
	it("forwards DOM_DELTA_LINE as whole notches", async () => {
		await mount(2);
		wheel({ deltaY: 2, deltaMode: 1 });
		expect(forwardedNotches()).toEqual([0, 2]);
	});

	// Catches: a reversal inheriting the outbound gesture's banked pixels.
	it("scrolls the other way at once after a reversal", async () => {
		await mount(2);
		wheel({ deltaY: 1 });
		wheel({ deltaY: -1000 });
		expect(forwardedNotches()).toEqual([3, 0]);
	});

	// Catches: Shift+wheel (the scrollback escape hatch) reaching the app.
	it("never forwards Shift+wheel, on either axis", async () => {
		await mount(2);
		wheel({ deltaY: 100, shiftKey: true });
		wheel({ deltaX: 100, deltaY: 0, shiftKey: true });
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: a handler that merely drops Shift+wheel instead of handing it to TUIC's scrollback.
	it("scrolls TUIC's scrollback on Shift+wheel instead of forwarding it", async () => {
		await mount(2, 1000);
		wheel({ deltaY: -100, shiftKey: true });
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith(
				"terminal_scroll_to_anchor",
				expect.objectContaining({ sessionId: "wheel-1" }),
			),
		);
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: a Shift+wheel event (scrollback branch) replacing the forwarded gesture's idle
	// timer, so the sub-notch residual survives the 200 ms idle gap and fires a notch early.
	it("drops the forwarded residual after idle even when a Shift+wheel arrived in between", async () => {
		await mount(2, 1000);
		wheel({ deltaY: 14 });
		wheel({ deltaY: 14, shiftKey: true });
		await new Promise((r) => setTimeout(r, 300));
		wheel({ deltaY: 14 });
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: resetSmoothScroll (typing, resize, scrollbar drag) cancelling the gesture-end
	// timer without clearing the forwarded residual, so the next gesture inherits it.
	it("drops the forwarded residual when typing cancels the gesture timer", async () => {
		await mount(2);
		wheel({ deltaY: 14 });
		// Keyboard input is bound to the hidden <input> next to the canvas.
		const keyInput = document.querySelector('input[aria-hidden="true"]');
		if (!keyInput) throw new Error("hidden key input not mounted");
		keyInput.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true, cancelable: true }));
		await waitFor(() => expect(ptyWrites().join("")).toContain("a"));
		invoke.mockClear();
		wheel({ deltaY: 14 });
		expect(forwardedNotches()).toEqual([0, 0]);
	});

	// Catches: wheel being swallowed or forwarded when the app does not report the mouse.
	it("forwards nothing when mouse reporting is off", async () => {
		await mount(0);
		wheel({ deltaY: 1000 });
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: a horizontal-only swipe (no Shift) turning into vertical notches.
	it("ignores a horizontal-only swipe without Shift", async () => {
		await mount(2);
		wheel({ deltaX: 1000, deltaY: 0 });
		expect(ptyWrites()).toEqual([]);
	});
});
