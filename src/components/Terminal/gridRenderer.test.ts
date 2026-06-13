import { describe, expect, it, vi } from "vitest";
import {
	ATTR_DEFAULT_BG,
	ATTR_DEFAULT_FG,
	ATTR_INVERSE,
	type CellMetrics,
	type DecodedRow,
} from "./canvasTerminalUtils";
import { createGridRenderer, type GridContext2D } from "./gridRenderer";

function mockCtx() {
	const fillRect = vi.fn();
	const fillText = vi.fn();
	const ctx = {
		fillStyle: "",
		strokeStyle: "",
		font: "",
		lineWidth: 1,
		globalAlpha: 1,
		canvas: { width: 1400, height: 800 },
		fillRect,
		fillText,
		strokeRect: vi.fn(),
		beginPath: vi.fn(),
		moveTo: vi.fn(),
		lineTo: vi.fn(),
		stroke: vi.fn(),
		measureText: () => ({ width: 10 }),
	} as unknown as GridContext2D;
	return { ctx, fillRect, fillText };
}

const METRICS: CellMetrics = {
	cellWidth: 10,
	cellHeight: 20,
	baseline: 15,
	fontSize: 14,
	dpr: 1,
	scaledCellWidth: 10,
	scaledCellHeight: 20,
};

/** Build a row: "ABC" glyphs (default bg) followed by `bgSpaces` trailing space
 *  cells carrying an explicit background — grok's dark tool-block band shape. */
function rowWithTrailingBg(bgSpaces: number): DecodedRow {
	const count = 3 + bgSpaces;
	const codepoints = new Uint32Array(count);
	const fg = new Uint32Array(count);
	const bg = new Uint32Array(count);
	const attrs = new Uint8Array(count);
	codepoints[0] = 0x41; // A
	codepoints[1] = 0x42; // B
	codepoints[2] = 0x43; // C
	for (let c = 0; c < 3; c++) attrs[c] = ATTR_DEFAULT_FG | ATTR_DEFAULT_BG; // glyphs on default bg
	for (let c = 3; c < count; c++) {
		codepoints[c] = 0x20; // space
		bg[c] = 0x141414; // explicit dark bg (48;2;20;20;20)
		attrs[c] = ATTR_DEFAULT_FG; // default fg, EXPLICIT bg (ATTR_DEFAULT_BG cleared)
	}
	return { index: 0, count, codepoints, fg, bg, attrs };
}

describe("gridRenderer paintRow — background of trailing cells (story 036)", () => {
	it("fills trailing space cells that carry an explicit background", () => {
		const { ctx, fillRect } = mockCtx();
		const r = createGridRenderer(ctx, { fontWeight: () => "normal", getFontFamily: () => "monospace" });
		r.paintRow(rowWithTrailingBg(7), 0, METRICS);

		// Each trailing bg space (cols 3..9) must be filled at x = c*cellWidth.
		const filledXs = fillRect.mock.calls.map((c) => c[0] as number);
		for (let c = 3; c < 10; c++) {
			expect(filledXs).toContain(c * METRICS.cellWidth);
		}
	});

	it("does NOT fill trailing cells on the default background", () => {
		const { ctx, fillRect } = mockCtx();
		const r = createGridRenderer(ctx, { fontWeight: () => "normal", getFontFamily: () => "monospace" });
		// "ABC" then 7 trailing spaces, all on the DEFAULT background.
		const count = 10;
		const codepoints = new Uint32Array(count);
		const fg = new Uint32Array(count);
		const bg = new Uint32Array(count);
		const attrs = new Uint8Array(count);
		codepoints[0] = 0x41;
		codepoints[1] = 0x42;
		codepoints[2] = 0x43;
		for (let c = 0; c < count; c++) attrs[c] = ATTR_DEFAULT_FG | ATTR_DEFAULT_BG;
		for (let c = 3; c < count; c++) codepoints[c] = 0x20;
		r.paintRow({ index: 0, count, codepoints, fg, bg, attrs }, 0, METRICS);

		// No background fill should happen anywhere — every cell is default bg.
		expect(fillRect).not.toHaveBeenCalled();
	});

	it("fills trailing cells that have ATTR_DEFAULT_BG set but ATTR_INVERSE active", () => {
		const { ctx, fillRect } = mockCtx();
		const r = createGridRenderer(ctx, { fontWeight: () => "normal", getFontFamily: () => "monospace" });
		// "ABC" then 7 trailing spaces whose bg is *default* but rendered inverse —
		// the swap makes the (explicit) fg the visible background, so the cell must
		// fill despite ATTR_DEFAULT_BG being set. Guards the `|| ATTR_INVERSE` term.
		const count = 10;
		const codepoints = new Uint32Array(count);
		const fg = new Uint32Array(count);
		const bg = new Uint32Array(count);
		const attrs = new Uint8Array(count);
		codepoints[0] = 0x41;
		codepoints[1] = 0x42;
		codepoints[2] = 0x43;
		for (let c = 0; c < 3; c++) attrs[c] = ATTR_DEFAULT_FG | ATTR_DEFAULT_BG;
		for (let c = 3; c < count; c++) {
			codepoints[c] = 0x20;
			fg[c] = 0xff0000; // explicit fg — becomes the visible bg under inverse
			attrs[c] = ATTR_DEFAULT_BG | ATTR_INVERSE; // default-bg flag SET, inverse active
		}
		r.paintRow({ index: 0, count, codepoints, fg, bg, attrs }, 0, METRICS);

		const filledXs = fillRect.mock.calls.map((c) => c[0] as number);
		for (let c = 3; c < count; c++) {
			expect(filledXs).toContain(c * METRICS.cellWidth);
		}
	});
});
