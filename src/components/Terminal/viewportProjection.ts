import { type DecodedRow, rowText, utf16SpanToCellRange } from "./canvasTerminalUtils";
import { answerMarkerOffset } from "./suggestOverlay";

/** A frontend-only view of backend cells. Stored rows and PTY coordinates never change. */
export class ViewportProjection {
	readonly key: string;
	private readonly removed: number[];
	private readonly markers = new Map<number, [number, number]>();
	constructor(
		rows: ReadonlyMap<number, DecodedRow>,
		readonly hidden: ReadonlySet<number> = new Set(),
		answerMarkers: ReadonlySet<number> = new Set(),
	) {
		this.removed = Array.from(hidden).sort((a, b) => a - b);
		for (const index of answerMarkers) {
			const row = rows.get(index);
			const span = row && answerMarkerSpan(row);
			if (span) this.markers.set(index, span);
		}
		this.key = `${this.removed.join(",")};${[...this.markers].map(([row, span]) => `${row}:${span}`).join(";")}`;
	}
	/** Layout identity for one backend row, independent of its text generation. */
	rowLayoutKey(row: number): string {
		return `${this.displayRow(row)};${this.markers.get(row)?.join(":") ?? ""}`;
	}
	displayRow(row: number): number | null {
		if (this.hidden.has(row)) return null;
		return row >= 0
			? row - this.removed.filter((i) => i >= 0 && i < row).length
			: row + this.removed.filter((i) => i >= row && i < 0).length;
	}
	backendRow(row: number): number {
		let backend = row;
		if (row >= 0) {
			for (const index of this.removed) if (index >= 0 && index <= backend) backend++;
		} else {
			for (const index of this.removed.toReversed()) if (index < 0 && index >= backend) backend--;
		}
		return backend;
	}
	/** Boundaries inside an omitted marker collapse to its left edge. */
	displayCol(row: number, col: number): number {
		const span = this.markers.get(row);
		return span ? col - Math.max(0, Math.min(col, span[1]) - span[0]) : col;
	}
	backendCol(row: number, col: number): number {
		const span = this.markers.get(row);
		return span && col >= span[0] ? col + span[1] - span[0] : col;
	}
	toBackend(row: number, col: number): { row: number; col: number } {
		const backend = this.backendRow(row);
		return { row: backend, col: this.backendCol(backend, col) };
	}
	toDisplay(row: number, col: number): { row: number; col: number } | null {
		const display = this.displayRow(row);
		const span = this.markers.get(row);
		if (display === null || (span && col >= span[0] && col < span[1])) return null;
		return { row: display, col: this.displayCol(row, col) };
	}
	projectRow(row: DecodedRow): DecodedRow | null {
		const index = this.displayRow(row.index);
		if (index === null) return null;
		const span = this.markers.get(row.index);
		const content = span ? omitCells(row, span) : row;
		return index === content.index ? content : { ...content, index };
	}
}

function answerMarkerSpan(row: DecodedRow): [number, number] | null {
	const offset = answerMarkerOffset(rowText(row));
	// Soft-wrapped marker rows have no intent anchor in this physical fragment.
	const start =
		offset === undefined ? row.codepoints.indexOf(0x1f4ac) : utf16SpanToCellRange(row, offset, offset + 2)[0];
	if (start < 0) return null;
	let end = start + 1;
	if (row.codepoints[end] === 0) end++; // wide-cell spacer
	if (row.codepoints[end] === 32) end++; // protocol's trailing space
	return [start, end];
}

function omitCells(row: DecodedRow, [start, end]: [number, number]): DecodedRow {
	const count = row.count - (end - start);
	const codepoints = new Uint32Array(count);
	const fg = new Uint32Array(count);
	const bg = new Uint32Array(count);
	const attrs = new Uint8Array(count);
	for (const [from, to] of [
		[row.codepoints, codepoints],
		[row.fg, fg],
		[row.bg, bg],
		[row.attrs, attrs],
	] as const) {
		to.set(from.subarray(0, start));
		to.set(from.subarray(end), start);
	}
	const cellExtras = new Map<number, string>();
	for (const [col, text] of row.cellExtras ?? []) {
		if (col < start) cellExtras.set(col, text);
		else if (col >= end) cellExtras.set(col - end + start, text);
	}
	return { ...row, count, codepoints, fg, bg, attrs, cellExtras };
}

/** Standalone row paint uses the same column projection as the viewport. */
export function omitAnswerMarker(row: DecodedRow): DecodedRow {
	const span = answerMarkerSpan(row);
	return span ? omitCells(row, span) : row;
}
