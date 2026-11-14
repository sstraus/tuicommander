import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

const { invoke, sink } = vi.hoisted(() => ({
	invoke: vi.fn(),
	sink: { current: null as ((data: ArrayBuffer) => void) | null },
}));
vi.mock("../../components/Terminal/canvasTerminalTransport", async (original) => ({
	...(await original<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: async (callback: (data: ArrayBuffer) => void) => {
			sink.current = callback;
		},
		resubscribe: vi.fn(),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";
import { isPerfDebug, setPerfDebug } from "../../utils/perfDebug";

const ROWS = 8;
const COLS = 40;
// Deep enough that the entry gesture plus four steps up never pin the view at the
// history top, where a step moves nothing and correctly paints nothing.
const HISTORY = 2000;
// Every column holds a glyph, so a repainted row costs COLS fillText calls.
const textAt = (abs: number) => `${abs}:`.padEnd(COLS, String.fromCharCode(65 + (abs % 26)));

// Owned grid wire protocol (live frame and styled history range), not an external fixture.
function wire(start: number, count: number, styled = false) {
	const header = styled ? 12 : 26;
	const prefix = styled ? 6 : 4;
	const buffer = new ArrayBuffer(header + count * (prefix + COLS * 11));
	const view = new DataView(buffer);
	if (styled) {
		view.setUint32(0, start, true);
		view.setUint32(4, HISTORY, true);
		view.setUint16(8, COLS, true);
		view.setUint16(10, count, true);
	} else {
		view.setUint16(0, count, true);
		view.setUint8(6, 1);
		view.setUint32(7, HISTORY - start, true);
		view.setUint32(11, HISTORY, true);
		view.setUint16(18, ROWS, true);
		view.setUint16(20, COLS, true);
	}
	let at = header;
	for (let i = 0; i < count; i++) {
		if (styled) {
			view.setUint32(at, start + i, true);
			view.setUint16(at + 4, COLS, true);
		} else {
			view.setUint16(at, i, true);
			view.setUint16(at + 2, COLS, true);
		}
		at += prefix;
		for (const char of textAt(start + i)) {
			view.setUint32(at, char.codePointAt(0) ?? 32, true);
			at += 11;
		}
	}
	return buffer;
}

function context(canvas: HTMLCanvasElement) {
	const glyphs: string[] = [];
	const state = {
		canvas,
		measureText: () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }),
		fillText: (text: string) => {
			glyphs.push(text);
		},
	};
	return {
		glyphs,
		ctx: new Proxy(state, {
			get: (target, key) => Reflect.get(target, key) ?? (() => {}),
		}) as unknown as CanvasRenderingContext2D,
	};
}
let contexts: Map<HTMLCanvasElement, ReturnType<typeof context>>;
let wasPerfDebug = false;
beforeEach(() => {
	contexts = new Map();
	sink.current = null;
	wasPerfDebug = isPerfDebug();
	setPerfDebug(false);
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	invoke.mockReset().mockImplementation(async (cmd: string, args: { start: number; count: number }) => {
		if (cmd === "terminal_styled_rows")
			return wire(args.start, Math.min(args.count, HISTORY + ROWS - args.start), true);
		return null;
	});
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: async () => [], ready: Promise.resolve() },
	});
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
		width: 326,
		height: 128,
		top: 0,
		left: 0,
		right: 326,
		bottom: 128,
		x: 0,
		y: 0,
		toJSON: () => ({}),
	});
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
		let output = contexts.get(this);
		if (!output) {
			output = context(this);
			contexts.set(this, output);
		}
		return output.ctx;
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
	setPerfDebug(wasPerfDebug);
	vi.useRealTimers();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

// Catches: every frame of a wheel gesture repaints the whole viewport glyph by glyph, so the
// main-thread cost of scrolling scales with rows x columns and falls behind under CPU load (2778).
it.each([-1, 1])("a wheel gesture repaints only the rows it exposes, direction %s", async (direction) => {
	const terminalId = terminalsStore.add({
		name: "smooth paint",
		sessionId: "smooth-paint",
		fontSize: 14,
		cwd: "/workspace",
		awaitingInput: null,
	});
	const view = render(() => <CanvasTerminal sessionId="smooth-paint" terminalId={terminalId} />);
	try {
		await waitFor(() => expect(sink.current).not.toBeNull());
		vi.useFakeTimers();
		sink.current?.(wire(HISTORY, ROWS));
		await vi.advanceTimersByTimeAsync(64);
		const canvas = view.container.querySelector('canvas[tabindex="0"]') as HTMLCanvasElement;
		const base = contexts.get(canvas);
		if (!base) throw new Error("base canvas context missing");
		const wheel = (deltaY: number) =>
			canvas.dispatchEvent(new WheelEvent("wheel", { deltaY, bubbles: true, cancelable: true }));
		// Enter the gesture deep enough into history that a scroll down has room to move,
		// and let the band fetch fill the row cache.
		for (let i = 0; i < 6; i++) {
			wheel(-200);
			await vi.advanceTimersByTimeAsync(40);
		}
		await vi.advanceTimersByTimeAsync(40);
		// End the entry gesture: its acceleration would make one step move a whole screen,
		// where a full repaint is correct. Both directions then start at the same speed.
		await vi.advanceTimersByTimeAsync(250);
		const steps: { glyphs: number; rows: Set<string> }[] = [];
		for (let i = 0; i < 4; i++) {
			base.glyphs.length = 0;
			wheel(direction * 40);
			await vi.advanceTimersByTimeAsync(40);
			steps.push({ glyphs: base.glyphs.length, rows: new Set(base.glyphs.filter((g) => /\d/.test(g))) });
		}
		// The gesture moved: each step painted the label of at least one newly exposed row.
		expect(steps.every((step) => step.rows.size > 0)).toBe(true);
		// Independent oracle: a row is COLS glyphs, the viewport is ROWS rows. Moving a few
		// lines exposes a few rows; anything near ROWS * COLS is a whole-viewport repaint.
		for (const step of steps) expect(step.glyphs).toBeLessThanOrEqual((ROWS / 2) * COLS);
	} finally {
		view.unmount();
		terminalsStore.remove(terminalId);
	}
});
