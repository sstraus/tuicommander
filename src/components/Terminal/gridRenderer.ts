// --- Phase 1.3 shared grid renderer (single paint implementation) ---
//
// THE one canvas2d grid paint path, used by BOTH the main thread
// (CanvasTerminal base canvas) and the render worker (OffscreenCanvas). The
// per-cell fillText/box-drawing logic was moved here verbatim from
// CanvasTerminal so there is exactly one implementation — parity is by
// construction, not by test. The 2D context is injected, so the same code
// drives CanvasRenderingContext2D and OffscreenCanvasRenderingContext2D.
//
// NOTE: this module paints ONLY the base grid (clear + rows). Cursor,
// selection, search, links, scrollbar and suggest overlays stay on the main
// thread's overlay canvas (see CanvasTerminal.repaintOverlay).

import {
	ATTR_BOLD,
	ATTR_DEFAULT_BG,
	ATTR_DEFAULT_FG,
	ATTR_DIM,
	ATTR_INVERSE,
	ATTR_ITALIC,
	ATTR_STRIKEOUT,
	ATTR_UNDERLINE,
	type CellMetrics,
	type DecodedFrame,
	type DecodedRow,
	GUTTER_PX,
} from "./canvasTerminalUtils";

/**
 * Codepoints that mobile browsers (and some WebViews) render as color emoji
 * instead of monochrome text glyphs — surfacing as a box / colored circle in
 * the canvas terminal instead of the agent's status glyph. Appending U+FE0E
 * (VS15, the text variation selector) forces text presentation so they match
 * the desktop look. Mirrors the mobile DOM fix in utils/logLine.ts
 * (forceTextPresentation). Covers Claude Code, Codex, Copilot, Gemini glyphs:
 * ● ○ ⏺ ⏵ • ◦ ∴ ✢ ⚙ ✻ ◉
 */
const EMOJI_PRESENTATION_CPS = new Set<number>([
	0x25cf, 0x25cb, 0x23fa, 0x23f5, 0x2022, 0x25e6, 0x2234, 0x2722, 0x2699, 0x273b, 0x25c9,
]);
const VS15 = "\uFE0E";

/** Live theme/font lookups supplied by the host (main reads DOM; worker reads posted state). */
export interface GridRendererDeps {
	/** Default (non-bold) weight; number or CSS keyword, matching settings. */
	fontWeight: () => number | string;
	getFontFamily: () => string;
}

export interface PaintGridOptions {
	/** Force a full clear + repaint of every row in the rowMap. */
	fullRepaint: boolean;
	/** When set (and not fullRepaint), only these row indices are repainted. */
	dirtyIndices?: Set<number>;
}

export interface GridRenderer {
	setTheme(bgDefault: string, fgDefault: string): void;
	/** Drop memoized color/font strings (call on theme/font/weight change). */
	invalidateCaches(): void;
	paintGrid(rowMap: Map<number, DecodedRow>, m: CellMetrics, opts: PaintGridOptions): void;
	paintRow(row: DecodedRow, y: number, m: CellMetrics, fontFamily?: string): void;
	resolveFg(fgP: number, bgP: number, a: number, defaultColor: string): string;
	resolveBg(fgP: number, bgP: number, a: number, defaultColor: string): string;
	buildFontStyle(a: number, fontSize: number, fontFamily: string): string;
}

export type GridContext2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

export function createGridRenderer(ctx: GridContext2D, deps: GridRendererDeps): GridRenderer {
	// Theme colors (re-read on remeasure / posted to worker on theme change).
	let cachedBgDefault = "#1e1e1e";
	let cachedFgDefault = "#d4d4d4";

	// Memo caches (per renderer instance).
	const colorStringCache = new Map<number, string>();
	const fontStyleCache = new Map<string, string>();

	const SEXTANT_MAP: number[] = (() => {
		const t: number[] = [];
		for (let bits = 1; bits < 63; bits++) {
			if (bits === 0b010101 || bits === 0b101010) continue;
			t.push(bits);
		}
		return t;
	})();

	function cachedRgbString(r: number, g: number, b: number): string {
		const key = (r << 16) | (g << 8) | b;
		let s = colorStringCache.get(key);
		if (s === undefined) {
			s = `rgb(${r},${g},${b})`;
			colorStringCache.set(key, s);
		}
		return s;
	}

	function cachedFontStyle(italic: boolean, bold: boolean, fontSize: number, fontFamily: string): string {
		const weight = bold ? "bold" : deps.fontWeight();
		const key = `${italic ? "i" : ""}${weight}${fontSize}${fontFamily}`;
		let s = fontStyleCache.get(key);
		if (s === undefined) {
			s = `${italic ? "italic " : ""}${weight} ${fontSize}px ${fontFamily}`;
			fontStyleCache.set(key, s);
		}
		return s;
	}

	function resolveFg(fgP: number, bgP: number, a: number, defaultColor: string): string {
		if (a & ATTR_INVERSE) {
			return a & ATTR_DEFAULT_BG ? defaultColor : cachedRgbString((bgP >> 16) & 0xff, (bgP >> 8) & 0xff, bgP & 0xff);
		}
		return a & ATTR_DEFAULT_FG ? defaultColor : cachedRgbString((fgP >> 16) & 0xff, (fgP >> 8) & 0xff, fgP & 0xff);
	}

	function resolveBg(fgP: number, bgP: number, a: number, defaultColor: string): string {
		if (a & ATTR_INVERSE) {
			return a & ATTR_DEFAULT_FG ? defaultColor : cachedRgbString((fgP >> 16) & 0xff, (fgP >> 8) & 0xff, fgP & 0xff);
		}
		return a & ATTR_DEFAULT_BG ? defaultColor : cachedRgbString((bgP >> 16) & 0xff, (bgP >> 8) & 0xff, bgP & 0xff);
	}

	function buildFontStyle(a: number, fontSize: number, fontFamily: string): string {
		return cachedFontStyle((a & ATTR_ITALIC) !== 0, (a & ATTR_BOLD) !== 0, fontSize, fontFamily);
	}

	function drawBoxDrawingChar(cp: number, x: number, y: number, m: CellMetrics): boolean {
		if (cp < 0x2500 || cp > 0x257f) return false;
		const w = m.cellWidth;
		const h = m.cellHeight;
		const cx = x + Math.floor(w / 2);
		const cy = y + Math.floor(h / 2);
		const lw = Math.max(1, Math.round(w / 8));
		const hlw = Math.max(1, Math.round(w / 4));

		const line = (x1: number, y1: number, x2: number, y2: number, heavy = false) => {
			ctx.lineWidth = heavy ? hlw : lw;
			ctx.beginPath();
			ctx.moveTo(x1 + 0.5, y1 + 0.5);
			ctx.lineTo(x2 + 0.5, y2 + 0.5);
			ctx.stroke();
		};

		const right = x + w;
		const bottom = y + h;

		const L = 1,
			R = 2,
			U = 4,
			D = 8;
		const HL = 16,
			HR = 32,
			HU = 64,
			HD = 128;

		let seg = 0;
		switch (cp) {
			case 0x2500:
				seg = L | R;
				break; // ─
			case 0x2501:
				seg = HL | HR;
				break; // ━
			case 0x2502:
				seg = U | D;
				break; // │
			case 0x2503:
				seg = HU | HD;
				break; // ┃
			case 0x250c:
				seg = R | D;
				break; // ┌
			case 0x250d:
				seg = HR | D;
				break; // ┍
			case 0x250e:
				seg = R | HD;
				break; // ┎
			case 0x250f:
				seg = HR | HD;
				break; // ┏
			case 0x2510:
				seg = L | D;
				break; // ┐
			case 0x2511:
				seg = HL | D;
				break; // ┑
			case 0x2512:
				seg = L | HD;
				break; // ┒
			case 0x2513:
				seg = HL | HD;
				break; // ┓
			case 0x2514:
				seg = R | U;
				break; // └
			case 0x2515:
				seg = HR | U;
				break; // ┕
			case 0x2516:
				seg = R | HU;
				break; // ┖
			case 0x2517:
				seg = HR | HU;
				break; // ┗
			case 0x2518:
				seg = L | U;
				break; // ┘
			case 0x2519:
				seg = HL | U;
				break; // ┙
			case 0x251a:
				seg = L | HU;
				break; // ┚
			case 0x251b:
				seg = HL | HU;
				break; // ┛
			case 0x251c:
				seg = U | D | R;
				break; // ├
			case 0x251d:
				seg = U | D | HR;
				break; // ┝
			case 0x251e:
				seg = HU | D | R;
				break; // ┞
			case 0x251f:
				seg = U | HD | R;
				break; // ┟
			case 0x2520:
				seg = HU | HD | R;
				break; // ┠
			case 0x2521:
				seg = HU | D | HR;
				break; // ┡
			case 0x2522:
				seg = U | HD | HR;
				break; // ┢
			case 0x2523:
				seg = HU | HD | HR;
				break; // ┣
			case 0x2524:
				seg = U | D | L;
				break; // ┤
			case 0x2525:
				seg = U | D | HL;
				break; // ┥
			case 0x2526:
				seg = HU | D | L;
				break; // ┦
			case 0x2527:
				seg = U | HD | L;
				break; // ┧
			case 0x2528:
				seg = HU | HD | L;
				break; // ┨
			case 0x2529:
				seg = HU | D | HL;
				break; // ┩
			case 0x252a:
				seg = U | HD | HL;
				break; // ┪
			case 0x252b:
				seg = HU | HD | HL;
				break; // ┫
			case 0x252c:
				seg = L | R | D;
				break; // ┬
			case 0x252d:
				seg = HL | R | D;
				break; // ┭
			case 0x252e:
				seg = L | HR | D;
				break; // ┮
			case 0x252f:
				seg = HL | HR | D;
				break; // ┯
			case 0x2530:
				seg = L | R | HD;
				break; // ┰
			case 0x2531:
				seg = HL | R | HD;
				break; // ┱
			case 0x2532:
				seg = L | HR | HD;
				break; // ┲
			case 0x2533:
				seg = HL | HR | HD;
				break; // ┳
			case 0x2534:
				seg = L | R | U;
				break; // ┴
			case 0x2535:
				seg = HL | R | U;
				break; // ┵
			case 0x2536:
				seg = L | HR | U;
				break; // ┶
			case 0x2537:
				seg = HL | HR | U;
				break; // ┷
			case 0x2538:
				seg = L | R | HU;
				break; // ┸
			case 0x2539:
				seg = HL | R | HU;
				break; // ┹
			case 0x253a:
				seg = L | HR | HU;
				break; // ┺
			case 0x253b:
				seg = HL | HR | HU;
				break; // ┻
			case 0x253c:
				seg = L | R | U | D;
				break; // ┼
			case 0x253d:
				seg = HL | R | U | D;
				break; // ┽
			case 0x253e:
				seg = L | HR | U | D;
				break; // ┾
			case 0x253f:
				seg = HL | HR | U | D;
				break; // ┿
			case 0x2540:
				seg = L | R | HU | D;
				break; // ╀
			case 0x2541:
				seg = L | R | U | HD;
				break; // ╁
			case 0x2542:
				seg = L | R | HU | HD;
				break; // ╂
			case 0x2543:
				seg = HL | R | HU | D;
				break; // ╃
			case 0x2544:
				seg = L | HR | HU | D;
				break; // ╄
			case 0x2545:
				seg = HL | R | U | HD;
				break; // ╅
			case 0x2546:
				seg = L | HR | U | HD;
				break; // ╆
			case 0x2547:
				seg = HL | HR | HU | D;
				break; // ╇
			case 0x2548:
				seg = HL | HR | U | HD;
				break; // ╈
			case 0x2549:
				seg = HL | R | HU | HD;
				break; // ╉
			case 0x254a:
				seg = L | HR | HU | HD;
				break; // ╊
			case 0x254b:
				seg = HL | HR | HU | HD;
				break; // ╋
			case 0x2574:
				seg = L;
				break; // ╴
			case 0x2575:
				seg = U;
				break; // ╵
			case 0x2576:
				seg = R;
				break; // ╶
			case 0x2577:
				seg = D;
				break; // ╷
			case 0x2578:
				seg = HL;
				break; // ╸
			case 0x2579:
				seg = HU;
				break; // ╹
			case 0x257a:
				seg = HR;
				break; // ╺
			case 0x257b:
				seg = HD;
				break; // ╻
			case 0x257c:
				seg = L | HR;
				break; // ╼
			case 0x257d:
				seg = U | HD;
				break; // ╽
			case 0x257e:
				seg = HL | R;
				break; // ╾
			case 0x257f:
				seg = HU | D;
				break; // ╿
			// Triple-dash: 3 dashes, 9 segments total, 2:1 dash:gap ratio
			case 0x2504:
			case 0x2505: {
				ctx.lineWidth = cp === 0x2505 ? hlw : lw;
				const seg = w / 9;
				for (let i = 0; i < 3; i++) {
					const sx = x + i * 3 * seg;
					ctx.fillRect(sx, cy - ctx.lineWidth / 2, seg * 2, ctx.lineWidth);
				}
				return true;
			}
			case 0x2506:
			case 0x2507: {
				ctx.lineWidth = cp === 0x2507 ? hlw : lw;
				const seg = h / 9;
				for (let i = 0; i < 3; i++) {
					const sy = y + i * 3 * seg;
					ctx.fillRect(cx - ctx.lineWidth / 2, sy, ctx.lineWidth, seg * 2);
				}
				return true;
			}
			// Quadruple-dash: 4 dashes, 12 segments total, 2:1 dash:gap ratio
			case 0x2508:
			case 0x2509: {
				ctx.lineWidth = cp === 0x2509 ? hlw : lw;
				const seg = w / 12;
				for (let i = 0; i < 4; i++) {
					const sx = x + i * 3 * seg;
					ctx.fillRect(sx, cy - ctx.lineWidth / 2, seg * 2, ctx.lineWidth);
				}
				return true;
			}
			case 0x250a:
			case 0x250b: {
				ctx.lineWidth = cp === 0x250b ? hlw : lw;
				const seg = h / 12;
				for (let i = 0; i < 4; i++) {
					const sy = y + i * 3 * seg;
					ctx.fillRect(cx - ctx.lineWidth / 2, sy, ctx.lineWidth, seg * 2);
				}
				return true;
			}
			// Rounded corners
			case 0x256d:
				seg = R | D;
				break; // ╭
			case 0x256e:
				seg = L | D;
				break; // ╮
			case 0x256f:
				seg = L | U;
				break; // ╯
			case 0x2570:
				seg = R | U;
				break; // ╰
			// Double-dash: 2 dashes, 6 segments total, 2:1 dash:gap ratio
			case 0x254c:
			case 0x254d: {
				ctx.lineWidth = cp === 0x254d ? hlw : lw;
				const seg = w / 6;
				for (let i = 0; i < 2; i++) {
					const sx = x + i * 3 * seg;
					ctx.fillRect(sx, cy - ctx.lineWidth / 2, seg * 2, ctx.lineWidth);
				}
				return true;
			}
			case 0x254e:
			case 0x254f: {
				ctx.lineWidth = cp === 0x254f ? hlw : lw;
				const seg = h / 6;
				for (let i = 0; i < 2; i++) {
					const sy = y + i * 3 * seg;
					ctx.fillRect(cx - ctx.lineWidth / 2, sy, ctx.lineWidth, seg * 2);
				}
				return true;
			}
			// Diagonals
			case 0x2571: {
				line(right, y, x, bottom);
				return true;
			}
			case 0x2572: {
				line(x, y, right, bottom);
				return true;
			}
			case 0x2573: {
				line(right, y, x, bottom);
				line(x, y, right, bottom);
				return true;
			}
			// Double-line box drawing: two parallel lines with gap
			case 0x2550:
			case 0x2551:
			case 0x2552:
			case 0x2553:
			case 0x2554:
			case 0x2555:
			case 0x2556:
			case 0x2557:
			case 0x2558:
			case 0x2559:
			case 0x255a:
			case 0x255b:
			case 0x255c:
			case 0x255d:
			case 0x255e:
			case 0x255f:
			case 0x2560:
			case 0x2561:
			case 0x2562:
			case 0x2563:
			case 0x2564:
			case 0x2565:
			case 0x2566:
			case 0x2567:
			case 0x2568:
			case 0x2569:
			case 0x256a:
			case 0x256b:
			case 0x256c: {
				const g = Math.max(1, Math.round(w / 6));
				const dbl = (x1: number, y1: number, x2: number, y2: number, horiz: boolean) => {
					const off = Math.floor(g / 2 + 0.5);
					if (horiz) {
						line(x1, y1 - off, x2, y2 - off);
						line(x1, y1 + off, x2, y2 + off);
					} else {
						line(x1 - off, y1, x2 - off, y2);
						line(x1 + off, y1, x2 + off, y2);
					}
				};
				// Encode: which directions are single (s) vs double (d)
				// Format: [sL, sR, sU, sD, dL, dR, dU, dD]
				const t: Record<number, number[]> = {
					9552: [0, 0, 0, 0, 1, 1, 0, 0], // ═
					9553: [0, 0, 0, 0, 0, 0, 1, 1], // ║
					9554: [0, 1, 0, 0, 0, 0, 0, 1], // ╒
					9555: [0, 0, 0, 1, 0, 1, 0, 0], // ╓
					9556: [0, 0, 0, 0, 0, 1, 0, 1], // ╔
					9557: [1, 0, 0, 0, 0, 0, 0, 1], // ╕
					9558: [0, 0, 0, 1, 1, 0, 0, 0], // ╖
					9559: [0, 0, 0, 0, 1, 0, 0, 1], // ╗
					9560: [0, 1, 0, 0, 0, 0, 1, 0], // ╘
					9561: [0, 0, 1, 0, 0, 1, 0, 0], // ╙
					9562: [0, 0, 0, 0, 0, 1, 1, 0], // ╚
					9563: [1, 0, 0, 0, 0, 0, 1, 0], // ╛
					9564: [0, 0, 1, 0, 1, 0, 0, 0], // ╜
					9565: [0, 0, 0, 0, 1, 0, 1, 0], // ╝
					9566: [0, 1, 0, 0, 0, 0, 1, 1], // ╞
					9567: [0, 0, 1, 1, 0, 1, 0, 0], // ╟
					9568: [0, 0, 0, 0, 0, 1, 1, 1], // ╠
					9569: [1, 0, 0, 0, 0, 0, 1, 1], // ╡
					9570: [0, 0, 1, 1, 1, 0, 0, 0], // ╢
					9571: [0, 0, 0, 0, 1, 0, 1, 1], // ╣
					9572: [0, 0, 0, 1, 1, 1, 0, 0], // ╤
					9573: [1, 1, 0, 0, 0, 0, 0, 1], // ╥
					9574: [0, 0, 0, 0, 1, 1, 0, 1], // ╦
					9575: [0, 0, 1, 0, 1, 1, 0, 0], // ╧
					9576: [1, 1, 0, 0, 0, 0, 1, 0], // ╨
					9577: [0, 0, 0, 0, 1, 1, 1, 0], // ╩
					9578: [0, 0, 1, 1, 1, 1, 0, 0], // ╪
					9579: [1, 1, 0, 0, 0, 0, 1, 1], // ╫
					9580: [0, 0, 0, 0, 1, 1, 1, 1], // ╬
				};
				const d = t[cp];
				if (!d) return false;
				if (d[0]) line(x, cy, cx, cy);
				if (d[1]) line(cx, cy, right, cy);
				if (d[2]) line(cx, y, cx, cy);
				if (d[3]) line(cx, cy, cx, bottom);
				if (d[4]) dbl(x, cy, cx, cy, true);
				if (d[5]) dbl(cx, cy, right, cy, true);
				if (d[6]) dbl(cx, y, cx, cy, false);
				if (d[7]) dbl(cx, cy, cx, bottom, false);
				return true;
			}
			default:
				return false;
		}

		if (seg & L) line(x, cy, cx, cy);
		if (seg & R) line(cx, cy, right, cy);
		if (seg & U) line(cx, y, cx, cy);
		if (seg & D) line(cx, cy, cx, bottom);
		if (seg & HL) line(x, cy, cx, cy, true);
		if (seg & HR) line(cx, cy, right, cy, true);
		if (seg & HU) line(cx, y, cx, cy, true);
		if (seg & HD) line(cx, cy, cx, bottom, true);
		return true;
	}

	function drawBlockChar(cp: number, x: number, y: number, m: CellMetrics): boolean {
		const w = m.cellWidth;
		const h = m.cellHeight;
		const hw = Math.ceil(w / 2);
		const hh = Math.ceil(h / 2);
		const hw2 = w - hw;
		const hh2 = h - hh;
		switch (cp) {
			// Half blocks
			case 0x2580:
				ctx.fillRect(x, y, w, hh);
				return true;
			case 0x2584:
				ctx.fillRect(x, y + hh, w, hh2);
				return true;
			case 0x2588:
				ctx.fillRect(x, y, w, h);
				return true;
			case 0x258c:
				ctx.fillRect(x, y, hw, h);
				return true;
			case 0x2590:
				ctx.fillRect(x + hw, y, hw2, h);
				return true;
			// Shade blocks
			case 0x2591: {
				const a = ctx.globalAlpha;
				ctx.globalAlpha = a * 0.25;
				ctx.fillRect(x, y, w, h);
				ctx.globalAlpha = a;
				return true;
			}
			case 0x2592: {
				const a = ctx.globalAlpha;
				ctx.globalAlpha = a * 0.5;
				ctx.fillRect(x, y, w, h);
				ctx.globalAlpha = a;
				return true;
			}
			case 0x2593: {
				const a = ctx.globalAlpha;
				ctx.globalAlpha = a * 0.75;
				ctx.fillRect(x, y, w, h);
				ctx.globalAlpha = a;
				return true;
			}
			// Quadrant block elements
			case 0x2596:
				ctx.fillRect(x, y + hh, hw, hh2);
				return true;
			case 0x2597:
				ctx.fillRect(x + hw, y + hh, hw2, hh2);
				return true;
			case 0x2598:
				ctx.fillRect(x, y, hw, hh);
				return true;
			case 0x2599:
				ctx.fillRect(x, y, hw, h);
				ctx.fillRect(x + hw, y + hh, hw2, hh2);
				return true;
			case 0x259a:
				ctx.fillRect(x, y, hw, hh);
				ctx.fillRect(x + hw, y + hh, hw2, hh2);
				return true;
			case 0x259b:
				ctx.fillRect(x, y, w, hh);
				ctx.fillRect(x, y + hh, hw, hh2);
				return true;
			case 0x259c:
				ctx.fillRect(x, y, w, hh);
				ctx.fillRect(x + hw, y + hh, hw2, hh2);
				return true;
			case 0x259d:
				ctx.fillRect(x + hw, y, hw2, hh);
				return true;
			case 0x259e:
				ctx.fillRect(x + hw, y, hw2, hh);
				ctx.fillRect(x, y + hh, hw, hh2);
				return true;
			case 0x259f:
				ctx.fillRect(x + hw, y, hw2, h);
				ctx.fillRect(x, y + hh, hw, hh2);
				return true;
			default:
				return false;
		}
	}

	function drawPowerlineChar(
		cp: number,
		x: number,
		y: number,
		m: CellMetrics,
		fgP: number,
		bgP: number,
		a: number,
	): boolean {
		const w = m.cellWidth;
		const h = m.cellHeight;
		const fg = resolveFg(fgP, bgP, a, cachedFgDefault);
		const bg = resolveBg(fgP, bgP, a, cachedBgDefault);

		switch (cp) {
			// Right-pointing triangle (filled)
			case 0xe0b0: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y + h / 2);
				ctx.lineTo(x, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Right-pointing triangle (line)
			case 0xe0b1: {
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y + h / 2);
				ctx.lineTo(x, y + h);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Left-pointing triangle (filled)
			case 0xe0b2: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x + w, y);
				ctx.lineTo(x, y + h / 2);
				ctx.lineTo(x + w, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Left-pointing triangle (line)
			case 0xe0b3: {
				ctx.beginPath();
				ctx.moveTo(x + w, y);
				ctx.lineTo(x, y + h / 2);
				ctx.lineTo(x + w, y + h);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Right semicircle (filled)
			case 0xe0b4: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.quadraticCurveTo(x + w * 2, y + h / 2, x, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Right semicircle (line)
			case 0xe0b5: {
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.quadraticCurveTo(x + w * 2, y + h / 2, x, y + h);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Left semicircle (filled)
			case 0xe0b6: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x + w, y);
				ctx.quadraticCurveTo(x - w, y + h / 2, x + w, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Left semicircle (line)
			case 0xe0b7: {
				ctx.beginPath();
				ctx.moveTo(x + w, y);
				ctx.quadraticCurveTo(x - w, y + h / 2, x + w, y + h);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Lower-left triangle (filled)
			case 0xe0b8: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y + h);
				ctx.lineTo(x + w, y);
				ctx.lineTo(x + w, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Lower-left triangle (line)
			case 0xe0b9: {
				ctx.beginPath();
				ctx.moveTo(x, y + h);
				ctx.lineTo(x + w, y);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Lower-right triangle (filled)
			case 0xe0ba: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y + h);
				ctx.lineTo(x, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Lower-right triangle (line)
			case 0xe0bb: {
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y + h);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Upper-left triangle (filled)
			case 0xe0bc: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y);
				ctx.lineTo(x + w, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Upper-left triangle (line)
			case 0xe0bd: {
				ctx.beginPath();
				ctx.moveTo(x, y + h);
				ctx.lineTo(x + w, y);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			// Upper-right triangle (filled)
			case 0xe0be: {
				ctx.fillStyle = bg;
				ctx.fillRect(x, y, w, h);
				ctx.beginPath();
				ctx.moveTo(x, y);
				ctx.lineTo(x + w, y);
				ctx.lineTo(x, y + h);
				ctx.closePath();
				ctx.fillStyle = fg;
				ctx.fill();
				return true;
			}
			// Upper-right triangle (line)
			case 0xe0bf: {
				ctx.beginPath();
				ctx.moveTo(x + w, y + h);
				ctx.lineTo(x, y);
				ctx.strokeStyle = fg;
				ctx.lineWidth = 1;
				ctx.stroke();
				return true;
			}
			default:
				return false;
		}
	}

	function drawBrailleChar(cp: number, x: number, y: number, m: CellMetrics): void {
		const dots = cp - 0x2800;
		if (dots === 0) return;
		const w = m.cellWidth;
		const h = m.cellHeight;
		const r = Math.max(0.5, w / 8);
		const areaW = w / 2;
		const areaH = h / 4;
		const map = [
			[0, 0],
			[0, 1],
			[0, 2],
			[1, 0],
			[1, 1],
			[1, 2],
			[0, 3],
			[1, 3],
		];
		for (let i = 0; i < 8; i++) {
			if (dots & (1 << i)) {
				const [col, row] = map[i];
				const cx = x + col * areaW + areaW / 2;
				const cy = y + row * areaH + areaH / 2;
				ctx.beginPath();
				ctx.arc(cx, cy, r, 0, Math.PI * 2);
				ctx.fill();
			}
		}
	}

	function drawLegacyComputingChar(cp: number, x: number, y: number, m: CellMetrics): boolean {
		const w = m.cellWidth;
		const h = m.cellHeight;

		// Sextant block elements (U+1FB00–U+1FB3B): 2×3 grid
		if (cp >= 0x1fb00 && cp <= 0x1fb3b) {
			const bits = SEXTANT_MAP[cp - 0x1fb00];
			if (bits === undefined) return false;
			const hw = Math.ceil(w / 2);
			const th = Math.floor(h / 3);
			const widths = [hw, w - hw];
			const heights = [th, th, h - th * 2];
			const cols = [0, hw];
			const rows = [0, th, th * 2];
			for (let bit = 0; bit < 6; bit++) {
				if (bits & (1 << bit)) {
					const col = bit & 1;
					const row = bit >> 1;
					ctx.fillRect(x + cols[col], y + rows[row], widths[col], heights[row]);
				}
			}
			return true;
		}

		// Smooth mosaic wedge/triangle characters (U+1FB3C–U+1FB6F)
		if (cp >= 0x1fb3c && cp <= 0x1fb6f) {
			return drawWedgeChar(cp, x, y, w, h);
		}

		// Vertical 1/8 strips at positions 2–7 (U+1FB70–U+1FB75)
		if (cp >= 0x1fb70 && cp <= 0x1fb75) {
			const pos = cp - 0x1fb70 + 1; // positions 1–6 → columns 2–7
			const x0 = Math.round((w * pos) / 8);
			const x1 = Math.round((w * (pos + 1)) / 8);
			ctx.fillRect(x + x0, y, x1 - x0, h);
			return true;
		}

		// Horizontal 1/8 strips at positions 2–7 (U+1FB76–U+1FB7B)
		if (cp >= 0x1fb76 && cp <= 0x1fb7b) {
			const pos = cp - 0x1fb76 + 1;
			const y0 = Math.round((h * pos) / 8);
			const y1 = Math.round((h * (pos + 1)) / 8);
			ctx.fillRect(x, y + y0, w, y1 - y0);
			return true;
		}

		// Combined corner 1/8 blocks (U+1FB7C–U+1FB81)
		if (cp >= 0x1fb7c && cp <= 0x1fb81) {
			const ew = Math.round(w / 8);
			const eh = Math.round(h / 8);
			switch (cp) {
				case 0x1fb7c: // left + lower
					ctx.fillRect(x, y, ew, h);
					ctx.fillRect(x, y + h - eh, w, eh);
					return true;
				case 0x1fb7d: // left + upper
					ctx.fillRect(x, y, ew, h);
					ctx.fillRect(x, y, w, eh);
					return true;
				case 0x1fb7e: // right + upper
					ctx.fillRect(x + w - ew, y, ew, h);
					ctx.fillRect(x, y, w, eh);
					return true;
				case 0x1fb7f: // right + lower
					ctx.fillRect(x + w - ew, y, ew, h);
					ctx.fillRect(x, y + h - eh, w, eh);
					return true;
				case 0x1fb80: // upper + lower
					ctx.fillRect(x, y, w, eh);
					ctx.fillRect(x, y + h - eh, w, eh);
					return true;
				case 0x1fb81: // rows 1,3,5,8
					ctx.fillRect(x, y, w, eh);
					ctx.fillRect(x, y + Math.round((h * 2) / 8), w, eh);
					ctx.fillRect(x, y + Math.round((h * 4) / 8), w, eh);
					ctx.fillRect(x, y + h - eh, w, eh);
					return true;
			}
			return false;
		}

		// Upper block fractions (U+1FB82–U+1FB86): 2/8, 3/8, 5/8, 6/8, 7/8
		if (cp >= 0x1fb82 && cp <= 0x1fb86) {
			const eighths = [2, 3, 5, 6, 7][cp - 0x1fb82];
			ctx.fillRect(x, y, w, Math.round((h * eighths) / 8));
			return true;
		}

		// Right block fractions (U+1FB87–U+1FB8B): 2/8, 3/8, 5/8, 6/8, 7/8
		if (cp >= 0x1fb87 && cp <= 0x1fb8b) {
			const eighths = [2, 3, 5, 6, 7][cp - 0x1fb87];
			const bw = Math.round((w * eighths) / 8);
			ctx.fillRect(x + w - bw, y, bw, h);
			return true;
		}

		return false;
	}

	function drawWedgeChar(cp: number, x: number, y: number, w: number, h: number): boolean {
		// Polygon vertices in normalized coords → absolute
		type Pt = [number, number];
		const poly = (...pts: Pt[]) => {
			ctx.beginPath();
			ctx.moveTo(x + pts[0][0] * w, y + pts[0][1] * h);
			for (let i = 1; i < pts.length; i++) ctx.lineTo(x + pts[i][0] * w, y + pts[i][1] * h);
			ctx.closePath();
			ctx.fill();
		};
		switch (cp) {
			// Lower-left family
			case 0x1fb3c:
				poly([0, 2 / 3], [0, 1], [1 / 2, 1]);
				return true;
			case 0x1fb3d:
				poly([0, 2 / 3], [0, 1], [1, 1]);
				return true;
			case 0x1fb3e:
				poly([0, 1 / 3], [0, 1], [1 / 2, 1]);
				return true;
			case 0x1fb3f:
				poly([0, 1 / 3], [0, 1], [1, 1]);
				return true;
			case 0x1fb40:
				poly([0, 0], [0, 1], [1 / 2, 1]);
				return true;
			// Lower-right/large family
			case 0x1fb41:
				poly([1 / 2, 0], [1, 0], [1, 1], [0, 1], [0, 1 / 3]);
				return true;
			case 0x1fb42:
				poly([1, 0], [1, 1], [0, 1], [0, 1 / 3]);
				return true;
			case 0x1fb43:
				poly([1 / 2, 0], [1, 0], [1, 1], [0, 1], [0, 2 / 3]);
				return true;
			case 0x1fb44:
				poly([1, 0], [1, 1], [0, 1], [0, 2 / 3]);
				return true;
			case 0x1fb45:
				poly([1 / 2, 0], [1, 0], [1, 1], [0, 1]);
				return true;
			case 0x1fb46:
				poly([0, 2 / 3], [1, 1 / 3], [1, 1], [0, 1]);
				return true;
			case 0x1fb47:
				poly([1 / 2, 1], [1, 2 / 3], [1, 1]);
				return true;
			case 0x1fb48:
				poly([0, 1], [1, 2 / 3], [1, 1]);
				return true;
			case 0x1fb49:
				poly([1 / 2, 1], [1, 1 / 3], [1, 1]);
				return true;
			case 0x1fb4a:
				poly([0, 1], [1, 1 / 3], [1, 1]);
				return true;
			case 0x1fb4b:
				poly([1 / 2, 1], [1, 0], [1, 1]);
				return true;
			// Lower-left mirror family
			case 0x1fb4c:
				poly([0, 0], [1 / 2, 0], [1, 1 / 3], [1, 1], [0, 1]);
				return true;
			case 0x1fb4d:
				poly([0, 0], [1, 1 / 3], [1, 1], [0, 1]);
				return true;
			case 0x1fb4e:
				poly([0, 0], [1 / 2, 0], [1, 2 / 3], [1, 1], [0, 1]);
				return true;
			case 0x1fb4f:
				poly([0, 0], [1, 2 / 3], [1, 1], [0, 1]);
				return true;
			case 0x1fb50:
				poly([0, 0], [1 / 2, 0], [1, 1], [0, 1]);
				return true;
			case 0x1fb51:
				poly([0, 1 / 3], [1, 2 / 3], [1, 1], [0, 1]);
				return true;
			// Upper-right diagonal family
			case 0x1fb52:
				poly([0, 0], [1, 0], [1, 1], [1 / 2, 1], [0, 2 / 3]);
				return true;
			case 0x1fb53:
				poly([0, 0], [1, 0], [1, 1], [0, 2 / 3]);
				return true;
			case 0x1fb54:
				poly([0, 0], [1, 0], [1, 1], [1 / 2, 1], [0, 1 / 3]);
				return true;
			case 0x1fb55:
				poly([0, 0], [1, 0], [1, 1], [0, 1 / 3]);
				return true;
			case 0x1fb56:
				poly([0, 0], [1, 0], [1, 1], [1 / 2, 1]);
				return true;
			// Upper-left family
			case 0x1fb57:
				poly([0, 0], [1 / 2, 0], [0, 1 / 3]);
				return true;
			case 0x1fb58:
				poly([0, 0], [1, 0], [0, 1 / 3]);
				return true;
			case 0x1fb59:
				poly([0, 0], [1 / 2, 0], [0, 2 / 3]);
				return true;
			case 0x1fb5a:
				poly([0, 0], [1, 0], [0, 2 / 3]);
				return true;
			case 0x1fb5b:
				poly([0, 0], [1 / 2, 0], [0, 1]);
				return true;
			case 0x1fb5c:
				poly([0, 0], [1, 0], [1, 1 / 3], [0, 2 / 3]);
				return true;
			case 0x1fb5d:
				poly([0, 0], [1, 0], [1, 2 / 3], [1 / 2, 1], [0, 1]);
				return true;
			case 0x1fb5e:
				poly([0, 0], [1, 0], [1, 2 / 3], [0, 1]);
				return true;
			case 0x1fb5f:
				poly([0, 0], [1, 0], [1, 1 / 3], [1 / 2, 1], [0, 1]);
				return true;
			case 0x1fb60:
				poly([0, 0], [1, 0], [1, 1 / 3], [0, 1]);
				return true;
			case 0x1fb61:
				poly([0, 0], [1, 0], [1 / 2, 1], [0, 1]);
				return true;
			// Upper-right corner family
			case 0x1fb62:
				poly([1 / 2, 0], [1, 0], [1, 1 / 3]);
				return true;
			case 0x1fb63:
				poly([0, 0], [1, 0], [1, 1 / 3]);
				return true;
			case 0x1fb64:
				poly([1 / 2, 0], [1, 0], [1, 2 / 3]);
				return true;
			case 0x1fb65:
				poly([0, 0], [1, 0], [1, 2 / 3]);
				return true;
			case 0x1fb66:
				poly([1 / 2, 0], [1, 0], [1, 1]);
				return true;
			case 0x1fb67:
				poly([0, 0], [1, 0], [1, 2 / 3], [0, 1 / 3]);
				return true;
			// Three-quarter blocks (center at 1/2, 1/2 → 4 triangles, fill 3)
			case 0x1fb68: // missing left
				poly([0, 0], [1, 0], [1 / 2, 1 / 2]);
				poly([1, 0], [1, 1], [1 / 2, 1 / 2]);
				poly([0, 1], [1, 1], [1 / 2, 1 / 2]);
				return true;
			case 0x1fb69: // missing upper
				poly([0, 0], [0, 1], [1 / 2, 1 / 2]);
				poly([0, 1], [1, 1], [1 / 2, 1 / 2]);
				poly([1, 0], [1, 1], [1 / 2, 1 / 2]);
				return true;
			case 0x1fb6a: // missing right
				poly([0, 0], [1, 0], [1 / 2, 1 / 2]);
				poly([0, 0], [0, 1], [1 / 2, 1 / 2]);
				poly([0, 1], [1, 1], [1 / 2, 1 / 2]);
				return true;
			case 0x1fb6b: // missing lower
				poly([0, 0], [1, 0], [1 / 2, 1 / 2]);
				poly([0, 0], [0, 1], [1 / 2, 1 / 2]);
				poly([1, 0], [1, 1], [1 / 2, 1 / 2]);
				return true;
			// One-quarter triangles
			case 0x1fb6c:
				poly([1 / 2, 1 / 2], [0, 0], [0, 1]);
				return true;
			case 0x1fb6d:
				poly([1 / 2, 1 / 2], [0, 0], [1, 0]);
				return true;
			case 0x1fb6e:
				poly([1 / 2, 1 / 2], [1, 0], [1, 1]);
				return true;
			case 0x1fb6f:
				poly([1 / 2, 1 / 2], [0, 1], [1, 1]);
				return true;
			default:
				return false;
		}
	}

	function paintRow(row: DecodedFrame["rows"][0], y: number, m: CellMetrics, fontFamily?: string) {
		fontFamily ??= deps.getFontFamily();

		let lastVisibleCol = -1;
		for (let c = row.count - 1; c >= 0; c--) {
			const cp = row.codepoints[c];
			if (cp !== 0 && cp !== 0x20) {
				lastVisibleCol = c;
				break;
			}
		}

		// Pass 1: backgrounds
		for (let c = 0; c < row.count; c++) {
			const cp = row.codepoints[c];
			const a = row.attrs[c];
			const hasExplicitBg = !(a & ATTR_DEFAULT_BG) || (a & ATTR_INVERSE) !== 0;
			// Skip only cells with nothing to paint: empty cells and trailing
			// spaces that ALSO carry the default background. A trailing space with
			// an explicit bg (e.g. grok's dark tool-block bands, or any full-width
			// colored bar, extending past the last glyph) MUST still be filled —
			// otherwise the colored rectangle is truncated at the last visible
			// glyph, producing dark bands misaligned to the text rows.
			if (!hasExplicitBg && (cp === 0 || (cp === 0x20 && c > lastVisibleCol))) continue;
			if (hasExplicitBg) {
				ctx.fillStyle = resolveBg(row.fg[c], row.bg[c], a, cachedBgDefault);
				ctx.fillRect(c * m.cellWidth, y, m.cellWidth, m.cellHeight);
			}
		}

		// Pass 2: text — render each glyph at its exact grid position to prevent
		// cursor drift (cellWidth is Math.round'd, so batched fillText runs
		// accumulate sub-pixel error over long lines).
		let lastFont = "";
		let lastFg = "";
		let lastDim = false;

		for (let c = 0; c < row.count; c++) {
			const cp = row.codepoints[c];
			if (cp === 0 || cp === 0x20) continue;

			const a = row.attrs[c];
			const fgP = row.fg[c];
			const bgP = row.bg[c];
			const x = c * m.cellWidth;

			if (cp >= 0x2500 && cp <= 0x257f) {
				ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
				ctx.strokeStyle = ctx.fillStyle;
				if (!drawBoxDrawingChar(cp, x, y, m)) {
					ctx.font = buildFontStyle(a, m.fontSize, fontFamily);
					ctx.fillText(String.fromCodePoint(cp), x, y + m.baseline);
				}
				lastFg = "";
				continue;
			}
			if ((cp >= 0x2580 && cp <= 0x2593) || (cp >= 0x2596 && cp <= 0x259f)) {
				ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
				drawBlockChar(cp, x, y, m);
				lastFg = "";
				continue;
			}
			if (cp >= 0xe0b0 && cp <= 0xe0bf) {
				if (drawPowerlineChar(cp, x, y, m, fgP, bgP, a)) {
					lastFg = "";
					continue;
				}
			}
			if (cp >= 0x2800 && cp <= 0x28ff) {
				ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
				drawBrailleChar(cp, x, y, m);
				lastFg = "";
				continue;
			}
			if (cp >= 0x1fb00 && cp <= 0x1fb8b) {
				ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
				if (drawLegacyComputingChar(cp, x, y, m)) {
					lastFg = "";
					continue;
				}
			}

			const font = buildFontStyle(a, m.fontSize, fontFamily);
			const fg = resolveFg(fgP, bgP, a, cachedFgDefault);
			const dim = (a & ATTR_DIM) !== 0;

			if (font !== lastFont) {
				ctx.font = font;
				lastFont = font;
			}
			if (fg !== lastFg) {
				ctx.fillStyle = fg;
				lastFg = fg;
			}
			if (dim !== lastDim) {
				ctx.globalAlpha = dim ? 0.5 : 1.0;
				lastDim = dim;
			}

			const glyph = EMOJI_PRESENTATION_CPS.has(cp)
				? String.fromCodePoint(cp) + VS15
				: String.fromCodePoint(cp);
			ctx.fillText(glyph, x, y + m.baseline);
		}
		if (lastDim) ctx.globalAlpha = 1.0;

		// Pass 3: decorations
		for (let c = 0; c < row.count; c++) {
			const a = row.attrs[c];
			if (!(a & (ATTR_UNDERLINE | ATTR_STRIKEOUT))) continue;
			const x = c * m.cellWidth;
			const fg = resolveFg(row.fg[c], row.bg[c], a, cachedFgDefault);
			if (a & ATTR_UNDERLINE) {
				ctx.fillStyle = fg;
				ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
			}
			if (a & ATTR_STRIKEOUT) {
				ctx.fillStyle = fg;
				ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
			}
		}
	}

	function paintGrid(rowMap: Map<number, DecodedRow>, m: CellMetrics, opts: PaintGridOptions): void {
		const fontFamily = deps.getFontFamily();
		const w = ctx.canvas.width / m.dpr;
		if (opts.fullRepaint || !opts.dirtyIndices) {
			// Full repaint: clear canvas + paint all rows (include gutter area)
			const h = ctx.canvas.height / m.dpr;
			ctx.fillStyle = cachedBgDefault;
			ctx.fillRect(-GUTTER_PX, 0, w, h);
			for (const [, row] of rowMap) {
				paintRow(row, row.index * m.cellHeight, m, fontFamily);
			}
		} else {
			// Incremental: repaint only dirty text rows (overlay handles cursor/selection/search)
			for (const idx of opts.dirtyIndices) {
				const y = idx * m.cellHeight;
				ctx.fillStyle = cachedBgDefault;
				ctx.fillRect(-GUTTER_PX, y, w, m.cellHeight);
				const row = rowMap.get(idx);
				if (row) paintRow(row, y, m, fontFamily);
			}
		}
	}

	function setTheme(bgDefault: string, fgDefault: string): void {
		cachedBgDefault = bgDefault;
		cachedFgDefault = fgDefault;
	}

	function invalidateCaches(): void {
		colorStringCache.clear();
		fontStyleCache.clear();
	}

	return { setTheme, invalidateCaches, paintGrid, paintRow, resolveFg, resolveBg, buildFontStyle };
}
