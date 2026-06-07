// --- Binary frame decoding and font measurement for CanvasTerminal ---

// Layout constants
export const GUTTER_PX = 6;
export const SCROLLBAR_PX = 14;

// Wire format constants (must match terminal_grid.rs)
const HEADER_SIZE = 22;
const CELL_SIZE = 11; // 4 (char u32) + 3 (fg) + 3 (bg) + 1 (attrs)
export const ATTR_BOLD = 0x01;
export const ATTR_ITALIC = 0x02;
export const ATTR_UNDERLINE = 0x04;
export const ATTR_STRIKEOUT = 0x08;
export const ATTR_DIM = 0x10;
export const ATTR_INVERSE = 0x20;
export const ATTR_DEFAULT_FG = 0x40;
export const ATTR_DEFAULT_BG = 0x80;

export interface DecodedRow {
	index: number;
	count: number;
	/** Unicode codepoints; 0 = empty cell */
	codepoints: Uint32Array;
	/** Packed fg color: r<<16|g<<8|b (valid when ATTR_DEFAULT_FG not set) */
	fg: Uint32Array;
	/** Packed bg color: r<<16|g<<8|b (valid when ATTR_DEFAULT_BG not set) */
	bg: Uint32Array;
	/** Per-cell ATTR_* bitmask */
	attrs: Uint8Array;
}

export interface DecodedFrame {
	cursorRow: number;
	cursorCol: number;
	cursorVisible: boolean;
	cursorShape: "block" | "underline" | "beam";
	displayOffset: number;
	historySize: number;
	hasSelection: boolean;
	keyboardFlags: number;
	bell: boolean;
	mouseMode: 0 | 1 | 2 | 3;
	sgrMouse: boolean;
	focusReporting: boolean;
	bracketedPaste: boolean;
	screenRows: number;
	screenCols: number;
	rows: DecodedRow[];
}

export interface CellMetrics {
	cellWidth: number;
	cellHeight: number;
	baseline: number;
	fontSize: number;
	dpr: number;
	scaledCellWidth: number;
	scaledCellHeight: number;
}

/** Decode a binary grid frame from the Rust backend into structured data. */
export function decodeBinaryFrame(buffer: ArrayBuffer): DecodedFrame | null {
	if (buffer.byteLength < HEADER_SIZE) return null;

	const view = new DataView(buffer);
	let offset = 0;

	const numRows = view.getUint16(offset, true);
	offset += 2;
	const cursorRow = view.getUint16(offset, true);
	offset += 2;
	const cursorCol = view.getUint16(offset, true);
	offset += 2;
	const cursorVisible = view.getUint8(offset) !== 0;
	offset += 1;
	const displayOffset = view.getUint32(offset, true);
	offset += 4;
	const historySize = view.getUint32(offset, true);
	offset += 4;
	const hasSelection = view.getUint8(offset) !== 0;
	offset += 1;
	const keyboardFlags = view.getUint8(offset);
	offset += 1;
	const frameFlags = view.getUint8(offset);
	offset += 1;
	const screenRows = view.getUint16(offset, true);
	offset += 2;
	const screenCols = view.getUint16(offset, true);
	offset += 2;
	const bell = (frameFlags & 0x01) !== 0;
	const cursorShapeRaw = (frameFlags >> 1) & 0x03;
	const cursorShape: "block" | "underline" | "beam" =
		cursorShapeRaw === 2 ? "beam" : cursorShapeRaw === 1 ? "underline" : "block";
	const mouseMode = ((frameFlags >> 3) & 0x03) as 0 | 1 | 2 | 3;
	const sgrMouse = (frameFlags & 0x20) !== 0;
	const focusReporting = (frameFlags & 0x40) !== 0;
	const bracketedPaste = (frameFlags & 0x80) !== 0;

	const rows: DecodedRow[] = [];
	for (let r = 0; r < numRows; r++) {
		if (offset + 4 > buffer.byteLength) break;
		const rowIndex = view.getUint16(offset, true);
		offset += 2;
		const colCount = view.getUint16(offset, true);
		offset += 2;

		const codepoints = new Uint32Array(colCount);
		const fg = new Uint32Array(colCount);
		const bg = new Uint32Array(colCount);
		const attrs = new Uint8Array(colCount);

		for (let c = 0; c < colCount; c++) {
			if (offset + CELL_SIZE > buffer.byteLength) break;
			codepoints[c] = view.getUint32(offset, true);
			offset += 4;
			const fgR = view.getUint8(offset++);
			const fgG = view.getUint8(offset++);
			const fgB = view.getUint8(offset++);
			const bgR = view.getUint8(offset++);
			const bgG = view.getUint8(offset++);
			const bgB = view.getUint8(offset++);
			attrs[c] = view.getUint8(offset++);
			fg[c] = (fgR << 16) | (fgG << 8) | fgB;
			bg[c] = (bgR << 16) | (bgG << 8) | bgB;
		}

		rows.push({ index: rowIndex, count: colCount, codepoints, fg, bg, attrs });
	}

	return {
		cursorRow,
		cursorCol,
		cursorVisible,
		cursorShape,
		displayOffset,
		historySize,
		hasSelection,
		keyboardFlags,
		bell,
		mouseMode,
		sgrMouse,
		focusReporting,
		bracketedPaste,
		screenRows,
		screenCols,
		rows,
	};
}

/** Measure natural character height via DOM span — matches xterm.js CharSizeService. */
export function measureCharHeightDOM(fontSize: number, fontFamily: string, fontWeight: number = 400): number {
	const span = document.createElement("span");
	span.style.font = `${fontWeight} ${fontSize}px ${fontFamily}`;
	span.style.lineHeight = "normal";
	span.style.position = "absolute";
	span.style.visibility = "hidden";
	span.textContent = "W";
	document.body.appendChild(span);
	const h = span.getBoundingClientRect().height;
	document.body.removeChild(span);
	return h;
}

/** Snap lineHeight to integer device pixels to prevent sub-pixel seams between rows. */
export function snapLineHeight(fontSize: number, target: number = 1.2): number {
	const dpr = window.devicePixelRatio || 1;
	const rawDevicePx = fontSize * target * dpr;
	const lo = Math.floor(rawDevicePx);
	const hi = Math.ceil(rawDevicePx);
	const best = Math.abs(rawDevicePx - lo) <= Math.abs(rawDevicePx - hi) ? lo : hi;
	const snapped = best / (fontSize * dpr);
	return Math.max(1.0, Math.min(snapped, 1.5));
}

export type CursorShape = "block" | "beam" | "underline";

export interface CursorRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** Compute the pixel rectangle for a cursor at the given grid position. */
export function computeCursorRect(shape: CursorShape, row: number, col: number, m: CellMetrics): CursorRect {
	const x = col * m.cellWidth;
	const y = row * m.cellHeight;
	switch (shape) {
		case "block":
			return { x, y, w: m.cellWidth, h: m.cellHeight };
		case "beam":
			return { x, y, w: 2, h: m.cellHeight };
		case "underline":
			return { x, y: y + m.cellHeight - 2, w: m.cellWidth, h: 2 };
	}
}

/**
 * Measure a monospace font and return cell metrics for grid layout.
 * Matches xterm.js WebGL renderer dimension calculation exactly:
 *   device.char.height = ceil(charHeight * dpr)
 *   device.cell.height = floor(device.char.height * lineHeight)
 *   charTop = round((cellHeight_device - charHeight_device) / 2)
 */
export function measureFont(
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
	fontSize: number,
	fontFamily: string,
	dpr: number = 1,
	lineHeight: number = 1.2,
	fontWeight: number = 400,
	charHeightOverride?: number,
): CellMetrics {
	ctx.font = `${fontWeight} ${fontSize}px ${fontFamily}`;
	const m = ctx.measureText("W");
	const cellWidth = Math.round(m.width);

	const ascent = m.fontBoundingBoxAscent ?? m.actualBoundingBoxAscent;
	const descent = m.fontBoundingBoxDescent ?? m.actualBoundingBoxDescent;
	const charHeightCSS = charHeightOverride ?? ascent + descent;

	// xterm.js WebGL formula: compute in device pixels, then convert back
	const charHeightDevice = Math.ceil(charHeightCSS * dpr);
	const cellHeightDevice = Math.floor(charHeightDevice * lineHeight);
	const charTopDevice = lineHeight === 1 ? 0 : Math.round((cellHeightDevice - charHeightDevice) / 2);

	const cellHeight = cellHeightDevice / dpr;
	const baseline = Math.ceil(ascent) + charTopDevice / dpr;

	return {
		cellWidth,
		cellHeight,
		baseline: Math.max(baseline, 0),
		fontSize,
		dpr,
		scaledCellWidth: cellWidth * dpr,
		scaledCellHeight: cellHeightDevice,
	};
}

/**
 * Resolve the terminal's default background/foreground from the theme CSS
 * custom properties that applyAppTheme (themes.ts) sets on the document root.
 *
 * Reads --bg-secondary and --fg-primary. These are the variables the theme
 * system actually defines (see applyAppTheme); --text-primary is NOT one of
 * them, so reading it would always fall back and make terminal text invisible
 * on light themes.
 */
export function resolveDefaultTerminalColors(el: HTMLElement): { bg: string; fg: string } {
	const style = getComputedStyle(el);
	const bg = style.getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
	const fg = style.getPropertyValue("--fg-primary").trim() || "#d4d4d4";
	return { bg, fg };
}
