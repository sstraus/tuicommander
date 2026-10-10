import type { CanvasSelectionController } from "./canvasTerminalSelection";
import {
	cellToTextOffset,
	type DecodedRow,
	rowTextLayout,
	textSpanToCellRanges,
	utf16SpanToCellRange,
} from "./canvasTerminalUtils";

export type HoveredLink = {
	row: number;
	colStart: number;
	colEnd: number;
	path: string;
	line?: number;
	col?: number;
	spans?: { row: number; colStart: number; colEnd: number }[];
};

/** What a span underlines on screen right now; "" when there is none. */
export function underlinedText(
	rowMap: Map<number, DecodedRow>,
	row: number,
	span: { colStart: number; colEnd: number } | undefined,
): string {
	const decoded = rowMap.get(row);
	if (!decoded || !span) return "";
	const { text, utf16Starts } = rowTextLayout(decoded);
	return text.slice(
		utf16Starts[Math.min(span.colStart, decoded.count)],
		utf16Starts[Math.min(span.colEnd, decoded.count)],
	);
}

/** The text each span of the link covers on screen now. */
export function hoverSignature(rowMap: Map<number, DecodedRow>, link: HoveredLink): string {
	return (link.spans ?? [link]).map((sp) => `${sp.row}:${underlinedText(rowMap, sp.row, sp)}`).join("\n");
}

export function underlinedTextAt(
	rowMap: Map<number, DecodedRow>,
	pressSpanAt: (row: number, col: number) => { colStart: number; colEnd: number } | undefined,
	row: number,
	col: number,
): string {
	return underlinedText(rowMap, row, pressSpanAt(row, col));
}

export function getLocalSelectionText(
	selection: CanvasSelectionController,
	selectionAbsRowToViewport: (absRow: number) => number | null,
	rowMap: Map<number, DecodedRow>,
): string {
	return selection.getLocalText((absRi) => {
		const vpRow = selectionAbsRowToViewport(absRi);
		return vpRow !== null ? (rowMap.get(vpRow) ?? null) : null;
	});
}

export const rowStringSpanToCells = (
	rowMap: Map<number, DecodedRow>,
	rowIndex: number,
	start: number,
	end: number,
): [number, number] | null => {
	const row = rowMap.get(rowIndex);
	return row ? utf16SpanToCellRange(row, start, end) : null;
};

export const logicalRows = (
	rowMap: Map<number, DecodedRow>,
	startRow: number,
): { index: number; row: DecodedRow }[] => {
	const rows: { index: number; row: DecodedRow }[] = [];
	for (let rowIndex = startRow; ; rowIndex++) {
		const row = rowMap.get(rowIndex);
		if (!row) return [];
		rows.push({ index: rowIndex, row });
		if (!row.wrapped) return rows;
	}
};

export const logicalStringSpanToCells = (
	rowMap: Map<number, DecodedRow>,
	startRow: number,
	text: string,
	start: number,
	end: number,
): { row: number; colStart: number; colEnd: number }[] => {
	return textSpanToCellRanges(logicalRows(rowMap, startRow), text, start, end) ?? [];
};

export const logicalCellToStringOffset = (
	rowMap: Map<number, DecodedRow>,
	startRow: number,
	text: string,
	rowIndex: number,
	col: number,
): number | null => cellToTextOffset(logicalRows(rowMap, startRow), text, rowIndex, col);
