import { settingsStore } from "../../stores/settings";
import type { CanvasSearchController, CanvasSelectionController } from "./canvasTerminalSelection";
import {
	type CellMetrics,
	type CursorShape,
	cellText,
	computeCursorRect,
	type DecodedFrame,
	type DecodedRow,
	GUTTER_PX,
} from "./canvasTerminalUtils";
import type { GridRenderer } from "./gridRenderer";
import { scrollbarThumb } from "./scrollbarThumb";

export function paintSearchHighlights(
	search: CanvasSearchController,
	absRowToViewport: (absRow: number) => number | null,
	octx: CanvasRenderingContext2D,
	m: CellMetrics,
) {
	if (search.matches.length === 0) return;
	for (let i = 0; i < search.matches.length; i++) {
		const match = search.matches[i];
		const vpRow = absRowToViewport(match.row);
		if (vpRow === null) continue;
		const isActive = i === search.activeIndex;
		const x = match.col_start * m.cellWidth;
		const y = vpRow * m.cellHeight;
		const w = (match.col_end - match.col_start) * m.cellWidth;
		octx.fillStyle = "rgba(255, 180, 50, 0.2)";
		octx.fillRect(x, y, w, m.cellHeight);
		if (isActive) {
			octx.fillStyle = "#e8984c";
			octx.fillRect(x, y + m.cellHeight - 2, w, 2);
		}
	}
}

export function paintSelection(
	selection: CanvasSelectionController,
	selectionAbsRowToViewport: (absRow: number) => number | null,
	overlayScrollOffset: number | null,
	rowCache: Map<number, DecodedRow>,
	rowMap: Map<number, DecodedRow>,
	octx: CanvasRenderingContext2D,
	m: CellMetrics,
) {
	if (!selection.start || !selection.end) return;
	const absStartRow = Math.min(selection.start.row, selection.end.row);
	const absEndRow = Math.max(selection.start.row, selection.end.row);

	octx.fillStyle = "rgba(58, 130, 220, 0.35)";

	for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
		const vpRow = selectionAbsRowToViewport(absRi);
		if (vpRow === null) continue;
		// During a gesture both the selection and row cache use the same
		// eviction-stable all-time row index. At rest rowMap is viewport-relative.
		const row = overlayScrollOffset != null ? rowCache.get(absRi) : rowMap.get(vpRow);
		if (!row) continue;
		const y = vpRow * m.cellHeight;

		if (absStartRow === absEndRow) {
			const c0 = Math.min(selection.start.col, selection.end.col);
			const c1 = Math.max(selection.start.col, selection.end.col);
			octx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
		} else if (absRi === absStartRow) {
			const isStartFirst = selection.start.row <= selection.end.row;
			const startCol = isStartFirst ? selection.start.col : selection.end.col;
			octx.fillRect(startCol * m.cellWidth, y, (row.count - startCol) * m.cellWidth, m.cellHeight);
		} else if (absRi === absEndRow) {
			const isStartFirst = selection.start.row <= selection.end.row;
			const endCol = isStartFirst ? selection.end.col : selection.start.col;
			octx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
		} else {
			octx.fillRect(0, y, row.count * m.cellWidth, m.cellHeight);
		}
	}
}

export function paintCursor(
	focused: () => boolean,
	cursorBlinkOn: boolean,
	octx: CanvasRenderingContext2D,
	cachedFgDefault: string,
	rowMap: Map<number, DecodedRow>,
	gridRenderer: GridRenderer,
	cachedBgDefault: string,
	keyInputRef: HTMLInputElement,
	frame: DecodedFrame,
	m: CellMetrics,
) {
	if (frame.displayOffset > 0) return;
	if (!frame.cursorVisible) return;
	if (!focused()) return;
	if (!cursorBlinkOn) return;

	const settingShape: CursorShape =
		settingsStore.state.cursorStyle === "block"
			? "block"
			: settingsStore.state.cursorStyle === "underline"
				? "underline"
				: "beam";
	const shape: CursorShape = frame.cursorShape !== "block" ? frame.cursorShape : settingShape;
	const rect = computeCursorRect(shape, frame.cursorRow, frame.cursorCol, m);

	octx.fillStyle = cachedFgDefault;
	octx.fillRect(rect.x, rect.y, rect.w, rect.h);

	if (shape === "block") {
		const row = rowMap.get(frame.cursorRow);
		const col = frame.cursorCol;
		if (row && col < row.count) {
			const cp = row.codepoints[col];
			const glyph = cellText(row, col);
			if (glyph !== "" && (cp !== 0x20 || row.cellExtras?.has(col))) {
				const fontFamily = settingsStore.getFontFamily();
				octx.font = gridRenderer.buildFontStyle(row.attrs[col], m.fontSize, fontFamily);
				octx.fillStyle = cachedBgDefault;
				octx.fillText(glyph, rect.x, frame.cursorRow * m.cellHeight + m.baseline);
			}
		}
	}

	syncImePosition(keyInputRef, frame.cursorRow, frame.cursorCol, m);
}

export function syncImePosition(keyInputRef: HTMLInputElement, row: number, col: number, m: CellMetrics) {
	const x = GUTTER_PX + col * m.cellWidth;
	const y = row * m.cellHeight;
	keyInputRef.style.left = `${x}px`;
	keyInputRef.style.top = `${y}px`;
	keyInputRef.style.height = `${m.cellHeight}px`;
	keyInputRef.style.fontSize = `${m.fontSize}px`;
}

export function thumbFor(scrollbarTrackHeight: number, lastResizeRows: number, frame: DecodedFrame) {
	return scrollbarThumb({
		// Track height comes from the resize-time cache, not scrollbarRef.clientHeight;
		// visible rows = the authoritative resize row count — no per-frame
		// canvasRef.clientHeight read (layout-forcing).
		trackH: scrollbarTrackHeight,
		visibleRows: lastResizeRows || 24,
		historySize: frame.historySize,
		displayOffset: frame.displayOffset,
	});
}
