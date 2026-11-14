import { expect, it, vi } from "vitest";
import {
	ATTR_DEFAULT_BG,
	ATTR_DEFAULT_FG,
	ATTR_ITALIC,
	type CellMetrics,
	type DecodedRow,
} from "../canvasTerminalUtils";
import { createGridRenderer, type PaintGridOptions } from "../gridRenderer";

const metrics: CellMetrics = {
	cellWidth: 10,
	cellHeight: 20,
	baseline: 15,
	fontSize: 14,
	dpr: 2,
	scaledCellWidth: 20,
	scaledCellHeight: 40,
};
function row(index: number, text: string): DecodedRow {
	const count = text.length;
	return {
		index,
		count,
		wrapped: false,
		codepoints: Uint32Array.from(text, (c) => c.codePointAt(0)!),
		fg: new Uint32Array(count),
		bg: new Uint32Array(count),
		attrs: new Uint8Array(count).fill(ATTR_DEFAULT_BG | ATTR_DEFAULT_FG),
	};
}
function setup() {
	const ctx = {
		canvas: { width: 120, height: 120 },
		fillRect: vi.fn(),
		fillText: vi.fn(),
		drawImage: vi.fn(),
		save: vi.fn(),
		restore: vi.fn(),
		beginPath: vi.fn(),
		rect: vi.fn(),
		clip: vi.fn(),
	};
	const renderer = createGridRenderer(ctx as unknown as CanvasRenderingContext2D, {
		fontWeight: () => 400,
		getFontFamily: () => "monospace",
	});
	return { ctx, renderer };
}
function options(viewportTop: number, fullRepaint = false): PaintGridOptions {
	return { viewportTop, fullRepaint, dirtyIndices: new Set([0, 1, 2]) } as PaintGridOptions;
}
it("history growth copies retained pixels instead of repainting every glyph (2040)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(
		new Map([
			[0, row(0, "AAAA")],
			[1, row(1, "BBBB")],
			[2, row(2, "CCCC")],
		]),
		metrics,
		options(0, true),
	);
	ctx.fillText.mockClear();
	ctx.fillRect.mockClear();
	renderer.paintGrid(
		new Map([
			[0, row(0, "BBBB")],
			[1, row(1, "CCCC")],
			[2, row(2, "DDDD")],
		]),
		metrics,
		options(1),
	);
	expect(ctx.drawImage).toHaveBeenCalledTimes(1);
	expect(ctx.fillText.mock.calls.map((call) => call[0])).toEqual(["D", "D", "D", "D"]);
	expect(ctx.fillRect).toHaveBeenCalledWith(-6, 40, 60, 20);
});

it("pixel scrolling repaints changed retained rows rather than copying stale text (2040)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(
		new Map([
			[0, row(0, "AAAA")],
			[1, row(1, "BBBB")],
			[2, row(2, "CCCC")],
		]),
		metrics,
		options(0, true),
	);
	ctx.fillText.mockClear();
	renderer.paintGrid(
		new Map([
			[0, row(0, "BXBB")],
			[1, row(1, "CCCC")],
			[2, row(2, "DDDD")],
		]),
		metrics,
		options(1),
	);
	expect(ctx.fillText.mock.calls.map((call) => call[0]).join("")).toBe("BXBBDDDD");
});
it.each([0, 3, -3])("non-overlapping scroll %s never copies unrelated pixels (2040)", (top) => {
	const { ctx, renderer } = setup();
	const rows = new Map([
		[0, row(0, "AAAA")],
		[1, row(1, "BBBB")],
		[2, row(2, "CCCC")],
	]);
	renderer.paintGrid(rows, metrics, options(0, true));
	renderer.paintGrid(rows, metrics, options(top));
	expect(ctx.drawImage).not.toHaveBeenCalled();
});
// Catches: a wheel gesture back into history repaints every glyph of the viewport on each
// frame, so its main-thread cost scales with rows x columns instead of the rows it exposes (2778).
it("scrolling back into history copies retained pixels and paints only the exposed row (2778)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(
		new Map([
			[0, row(0, "BBBB")],
			[1, row(1, "CCCC")],
			[2, row(2, "DDDD")],
		]),
		metrics,
		options(1, true),
	);
	ctx.fillText.mockClear();
	ctx.fillRect.mockClear();
	renderer.paintGrid(
		new Map([
			[0, row(0, "AAAA")],
			[1, row(1, "BBBB")],
			[2, row(2, "CCCC")],
		]),
		metrics,
		options(0),
	);
	expect(ctx.drawImage).toHaveBeenCalledTimes(1);
	// Rows 0-1 of the old paint land on slots 1-2: source from the top, target one row down.
	expect(ctx.drawImage).toHaveBeenCalledWith(ctx.canvas, 0, 0, 120, 80, -6, 20, 60, 40);
	expect(ctx.fillText.mock.calls.map((call) => call[0])).toEqual(["A", "A", "A", "A"]);
	expect(ctx.fillRect).toHaveBeenCalledWith(-6, 0, 60, 20);
});
it("explicit invalidation prevents history pixels crossing a theme or screen boundary (2040)", () => {
	const { ctx, renderer } = setup();
	const rows = new Map([
		[0, row(0, "AAAA")],
		[1, row(1, "BBBB")],
		[2, row(2, "CCCC")],
	]);
	renderer.paintGrid(rows, metrics, options(0, true));
	ctx.fillText.mockClear();
	renderer.paintGrid(rows, metrics, options(1, true));
	expect(ctx.drawImage).not.toHaveBeenCalled();
	expect(ctx.fillText).toHaveBeenCalledTimes(12);
});

it("one changed cell repaints a clipped span instead of the whole row (2040)", () => {
	const { ctx, renderer } = setup();
	ctx.canvas.width = 240;
	const before = new Map([[0, row(0, "ABCDEFGHIJKL")]]);
	renderer.paintGrid(before, metrics, { fullRepaint: true });
	ctx.fillText.mockClear();
	ctx.fillRect.mockClear();
	renderer.paintGrid(new Map([[0, row(0, "ABCDEZ GHIJKL".replace(" ", ""))]]), metrics, {
		fullRepaint: false,
		dirtyIndices: new Set([0]),
	});
	expect(ctx.fillText.mock.calls.map((call) => call[0]).join("")).toBe("DEZGH");
	expect(ctx.fillRect).toHaveBeenCalledWith(40, 0, 30, 20);
});

it("unchanged full damage emits no base drawing rather than burning a repaint (2040)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(new Map([[0, row(0, "ABCDEF")]]), metrics, { fullRepaint: true });
	ctx.fillText.mockClear();
	ctx.fillRect.mockClear();
	renderer.paintGrid(new Map([[0, row(0, "ABCDEF")]]), metrics, { fullRepaint: false, dirtyIndices: new Set([0]) });
	expect(ctx.fillText).not.toHaveBeenCalled();
	expect(ctx.fillRect).not.toHaveBeenCalled();
});
it.each(["color", "combining", "shorter"])(
	"%s changes are not discarded by text-only damage equality (2040)",
	(kind) => {
		const { ctx, renderer } = setup();
		renderer.paintGrid(new Map([[0, row(0, "ABCDEF")]]), metrics, { fullRepaint: true });
		const next = row(0, kind === "shorter" ? "ABC" : "ABCDEF");
		if (kind === "color") {
			next.fg[3] = 0xff0000;
			next.attrs[3] = ATTR_DEFAULT_BG;
		}
		if (kind === "combining") next.cellExtras = new Map([[3, "\u0301"]]);
		ctx.fillText.mockClear();
		ctx.fillRect.mockClear();
		renderer.paintGrid(new Map([[0, next]]), metrics, { fullRepaint: false, dirtyIndices: new Set([0]) });
		expect(ctx.fillText).toHaveBeenCalled();
		expect(ctx.fillRect).toHaveBeenCalled();
		if (kind === "combining") expect(ctx.fillText.mock.calls.map((call) => call[0])).toContain("D\u0301");
		if (kind === "shorter") expect(ctx.fillRect).toHaveBeenCalledWith(-6, 0, 60, 20);
	},
);

it("same-background cells merge into one fill rather than one operation per cell (2040)", () => {
	const { ctx, renderer } = setup();
	const band = row(0, "          ");
	band.attrs.fill(ATTR_DEFAULT_FG);
	band.bg.fill(0x141414);
	renderer.paintRow(band, 0, metrics);
	expect(ctx.fillRect.mock.calls).toEqual([[0, 0, 100, 20]]);
});

it("background merging stops at default gaps and resolved color boundaries (2040)", () => {
	const { ctx, renderer } = setup();
	const band = row(0, "      ");
	band.attrs.fill(ATTR_DEFAULT_FG);
	band.bg.fill(0x141414);
	band.attrs[2] |= ATTR_DEFAULT_BG;
	band.bg[4] = 0xff0000;
	band.bg[5] = 0xff0000;
	renderer.paintRow(band, 0, metrics);
	expect(ctx.fillRect.mock.calls).toEqual([
		[0, 0, 20, 20],
		[30, 0, 10, 20],
		[40, 0, 20, 20],
	]);
});

it("first-column damage clears italic left bearings from the gutter (TERM-2)", () => {
	const { ctx, renderer } = setup();
	ctx.canvas.width = 240;
	const before = row(0, "jAAAAAAA");
	const after = row(0, " AAAAAAA");
	before.attrs.fill(ATTR_DEFAULT_BG | ATTR_DEFAULT_FG | ATTR_ITALIC);
	after.attrs.set(before.attrs);
	renderer.paintGrid(new Map([[0, before]]), metrics, { fullRepaint: true });
	ctx.fillRect.mockClear();
	renderer.paintGrid(new Map([[0, after]]), metrics, { fullRepaint: false, dirtyIndices: new Set([0]) });
	expect(ctx.rect).toHaveBeenCalledWith(-6, 0, 26, 20);
	expect(ctx.fillRect).toHaveBeenCalledWith(-6, 0, 26, 20);
});
