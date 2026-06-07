import { describe, expect, it } from "vitest";

import { ATTR_BOLD, ATTR_DEFAULT_BG, ATTR_DEFAULT_FG, ATTR_INVERSE, ATTR_ITALIC } from "../canvasTerminalUtils";
import { createGridRenderer } from "../gridRenderer";

// resolveFg/resolveBg/buildFontStyle never touch the 2D context, so a stub ctx
// is fine — these tests lock the pure color/font logic that was moved out of
// CanvasTerminal (pixel paint parity is verified live, not here).
function makeRenderer(fontWeight: number | string = 400) {
	const ctx = {} as unknown as CanvasRenderingContext2D;
	return createGridRenderer(ctx, { fontWeight: () => fontWeight, getFontFamily: () => "monospace" });
}

const DEF_BG = "#101010";
const DEF_FG = "#eeeeee";
const RED = 0xff0000; // packed r<<16|g<<8|b
const BLUE = 0x0000ff;

describe("gridRenderer color resolution", () => {
	it("returns the default fg/bg when the default-attr bit is set", () => {
		const gr = makeRenderer();
		expect(gr.resolveFg(RED, BLUE, ATTR_DEFAULT_FG, DEF_FG)).toBe(DEF_FG);
		expect(gr.resolveBg(RED, BLUE, ATTR_DEFAULT_BG, DEF_BG)).toBe(DEF_BG);
	});

	it("returns explicit packed colors as rgb() strings", () => {
		const gr = makeRenderer();
		expect(gr.resolveFg(RED, BLUE, 0, DEF_FG)).toBe("rgb(255,0,0)");
		expect(gr.resolveBg(RED, BLUE, 0, DEF_BG)).toBe("rgb(0,0,255)");
	});

	it("swaps fg/bg under the inverse attribute", () => {
		const gr = makeRenderer();
		// inverse fg uses the bg color (and vice-versa)
		expect(gr.resolveFg(RED, BLUE, ATTR_INVERSE, DEF_FG)).toBe("rgb(0,0,255)");
		expect(gr.resolveBg(RED, BLUE, ATTR_INVERSE, DEF_BG)).toBe("rgb(255,0,0)");
	});

	it("inverse with a default-bg cell paints the default color as fg", () => {
		const gr = makeRenderer();
		expect(gr.resolveFg(RED, BLUE, ATTR_INVERSE | ATTR_DEFAULT_BG, DEF_FG)).toBe(DEF_FG);
	});
});

describe("gridRenderer font style", () => {
	it("builds a plain font string at the default weight", () => {
		const gr = makeRenderer(300);
		expect(gr.buildFontStyle(0, 14, "JetBrains Mono")).toBe("300 14px JetBrains Mono");
	});

	it("uses bold weight for bold cells", () => {
		const gr = makeRenderer(300);
		expect(gr.buildFontStyle(ATTR_BOLD, 14, "JetBrains Mono")).toBe("bold 14px JetBrains Mono");
	});

	it("prefixes italic for italic cells", () => {
		const gr = makeRenderer(400);
		expect(gr.buildFontStyle(ATTR_ITALIC, 16, "Hack")).toBe("italic 400 16px Hack");
	});
});
