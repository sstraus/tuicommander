import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

const { invoke, sink, paints } = vi.hoisted(() => ({
	invoke: vi.fn(),
	sink: { current: null as ((data: ArrayBuffer) => void) | null },
	paints: vi.fn(),
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
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: paints, paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import type { CellMetrics, DecodedRow } from "../../components/Terminal/canvasTerminalUtils";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";
import { isPerfDebug, setPerfDebug } from "../../utils/perfDebug";

const ROWS = 8;
const COLS = 40;
const URL = "https://example.com";
const FILE = "file:///workspace/very-long-directory/example-report.md";
let linkAbs = 102;
let wrapped = false;
function textAt(abs: number): string {
	if (wrapped && abs === linkAbs) return FILE.slice(0, COLS);
	if (wrapped && abs === linkAbs + 1) return FILE.slice(COLS).padEnd(COLS);
	return (abs === linkAbs ? `link ${URL}` : `line ${abs}`).padEnd(COLS);
}
// Owned grid wire protocol, not a fixture that imitates an external CLI.
function wire(start: number, count: number, styled = false) {
	const header = styled ? 12 : 26;
	const prefix = styled ? 6 : 4;
	const buffer = new ArrayBuffer(header + count * (prefix + COLS * 11));
	const view = new DataView(buffer);
	if (styled) {
		view.setUint32(0, start, true);
		view.setUint32(4, 100, true);
		view.setUint16(8, COLS, true);
		view.setUint16(10, count, true);
	} else {
		view.setUint16(0, count, true);
		view.setUint8(6, 1);
		view.setUint32(7, 100 - start, true);
		view.setUint32(11, 100, true);
		view.setUint16(18, ROWS, true);
		view.setUint16(20, COLS, true);
	}
	let at = header;
	for (let i = 0; i < count; i++) {
		if (styled) {
			view.setUint32(at, start + i, true);
			view.setUint16(at + 4, COLS | (wrapped && start + i === linkAbs ? 0x8000 : 0), true);
		} else {
			view.setUint16(at, i, true);
			view.setUint16(at + 2, COLS | (wrapped && start + i === linkAbs ? 0x8000 : 0), true);
		}
		at += prefix;
		const text = textAt(start + i);
		for (const char of text) {
			view.setUint32(at, char.codePointAt(0) ?? 32, true);
			at += 11;
		}
	}
	return buffer;
}

function context() {
	const lines: { x: number; y: number; dashed: boolean }[] = [];
	let dashed = false;
	let pending: typeof lines = [];
	const state = {
		measureText: () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }),
		clearRect: () => {
			lines.length = 0;
		},
		setLineDash: (dash: number[]) => {
			dashed = dash.length > 0;
		},
		beginPath: () => {
			pending = [];
		},
		moveTo: (x: number, y: number) => {
			pending.push({ x, y, dashed });
		},
		stroke: () => {
			lines.push(...pending);
		},
	};
	return {
		lines,
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
	paints.mockClear();
	linkAbs = 102;
	wrapped = false;
	wasPerfDebug = isPerfDebug();
	setPerfDebug(false);
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	invoke
		.mockReset()
		.mockImplementation(
			async (
				cmd: string,
				args: { start: number; count: number; row: number; candidates: string[]; candidate: string },
			) => {
				if (cmd === "terminal_styled_rows") return wire(args.start, args.count, true);
				if (cmd === "terminal_get_row_text") return textAt(100 + args.row);
				if (cmd === "terminal_get_logical_line") return [linkAbs - 100, FILE];
				if (cmd === "resolve_terminal_paths")
					return args.candidates.map((candidate) =>
						candidate === FILE.slice(7) ? { absolute_path: FILE.slice(7), is_directory: false } : null,
					);
				if (cmd === "resolve_terminal_path")
					return args.candidate === FILE.slice(7) ? { absolute_path: FILE.slice(7), is_directory: false } : null;
				return null;
			},
		);
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
			output = context();
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

describe("link overlay during cached smooth scroll", () => {
	// Catches: the link overlay keeps backend viewport rows while cached text moves.
	it.each([-40, -240, 40])("projects or clips the underline with its text for wheel delta %s", async (delta) => {
		const top = delta > 0 ? 80 : 100;
		linkAbs = top + 2;
		const terminalId = terminalsStore.add({
			name: "link scroll",
			sessionId: "link-scroll",
			fontSize: 14,
			cwd: "/workspace",
			awaitingInput: null,
		});
		const view = render(() => <CanvasTerminal sessionId="link-scroll" terminalId={terminalId} />);
		try {
			await waitFor(() => expect(sink.current).not.toBeNull());
			vi.useFakeTimers();
			sink.current?.(wire(top, ROWS));
			await vi.advanceTimersByTimeAsync(32);
			const canvas = view.container.querySelector('canvas[tabindex="0"]') as HTMLCanvasElement;
			const overlay = contexts.get(view.container.querySelectorAll("canvas").item(2));
			expect(overlay?.lines.filter((line) => line.dashed)).toHaveLength(1);
			canvas.dispatchEvent(new WheelEvent("wheel", { deltaY: delta, bubbles: true, cancelable: true }));
			await vi.advanceTimersByTimeAsync(96);
			const [rows, metrics] = paints.mock.calls.at(-1) as [Map<number, DecodedRow>, CellMetrics];
			const linkRow = [...rows].find(([, row]) => String.fromCodePoint(...row.codepoints).includes(URL))?.[0];
			// The text renderer's output is the independent oracle for the overlay's position.
			const visible = linkRow !== undefined && linkRow >= 0 && linkRow < ROWS;
			expect(overlay?.lines.filter((line) => line.dashed)).toEqual(
				visible ? [{ x: 5 * metrics.cellWidth, y: (linkRow + 1) * metrics.cellHeight - 0.5, dashed: true }] : [],
			);
		} finally {
			view.unmount();
			terminalsStore.remove(terminalId);
		}
	});
	// Catches: a solid hover or one half of a verified wrapped file URL skips the scroll projection.
	it("moves both wrapped file underlines and Cmd-hover with their cached text", async () => {
		wrapped = true;
		const terminalId = terminalsStore.add({
			name: "wrapped link",
			sessionId: "link-scroll",
			fontSize: 14,
			cwd: "/workspace",
			awaitingInput: null,
		});
		const view = render(() => <CanvasTerminal sessionId="link-scroll" terminalId={terminalId} />);
		try {
			await waitFor(() => expect(sink.current).not.toBeNull());
			vi.useFakeTimers();
			sink.current?.(wire(100, ROWS));
			await vi.advanceTimersByTimeAsync(192);
			const canvas = view.container.querySelector('canvas[tabindex="0"]') as HTMLCanvasElement;
			const overlay = contexts.get(view.container.querySelectorAll("canvas").item(2));
			document.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 54, clientY: 40, metaKey: true }));
			await vi.advanceTimersByTimeAsync(112);
			expect(canvas.style.cursor).toBe("pointer");
			expect(overlay?.lines.filter((line) => !line.dashed)).toHaveLength(2);
			canvas.dispatchEvent(new WheelEvent("wheel", { deltaY: -40, bubbles: true, cancelable: true }));
			await vi.advanceTimersByTimeAsync(96);
			const [rows, metrics] = paints.mock.calls.at(-1) as [Map<number, DecodedRow>, CellMetrics];
			const firstRow = [...rows].find(([, row]) => String.fromCodePoint(...row.codepoints).startsWith("file://"))?.[0];
			expect(firstRow).toBeGreaterThan(2);
			if (firstRow === undefined) throw new Error("wrapped link missing from cached text");
			const ys = [(firstRow + 1) * metrics.cellHeight - 0.5, (firstRow + 2) * metrics.cellHeight - 0.5];
			for (const dashed of [true, false]) {
				expect([...new Set(overlay?.lines.filter((line) => line.dashed === dashed).map((line) => line.y))]).toEqual(ys);
			}
		} finally {
			view.unmount();
			terminalsStore.remove(terminalId);
		}
	});
});
