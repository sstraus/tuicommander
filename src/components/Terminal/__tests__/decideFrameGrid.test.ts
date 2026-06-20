import { describe, expect, it } from "vitest";

import { type DecodedFrame, type DecodedRow, decideFrameGrid, type FrameGridPrev } from "../canvasTerminalUtils";

function makeRow(index: number, count = 8): DecodedRow {
	return {
		index,
		count,
		codepoints: new Uint32Array(count),
		fg: new Uint32Array(count),
		bg: new Uint32Array(count),
		attrs: new Uint8Array(count),
	};
}

function makeFrame(opts: {
	screenRows: number;
	screenCols?: number;
	displayOffset?: number;
	historySize?: number;
	rows: DecodedRow[];
}): DecodedFrame {
	return {
		cursorRow: 0,
		cursorCol: 0,
		cursorVisible: true,
		cursorShape: "block",
		displayOffset: opts.displayOffset ?? 0,
		historySize: opts.historySize ?? 0,
		historyBase: 0,
		hasSelection: false,
		keyboardFlags: 0,
		bell: false,
		mouseMode: 0,
		sgrMouse: false,
		focusReporting: false,
		bracketedPaste: false,
		screenRows: opts.screenRows,
		screenCols: opts.screenCols ?? 80,
		rows: opts.rows,
	};
}

describe("decideFrameGrid", () => {
	const prev: FrameGridPrev = { lastScreenRows: 24, lastScreenCols: 80, lastDisplayOffset: 0, lastHistorySize: 100 };

	it("flags geomChanged when screen rows or cols differ", () => {
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 30, rows: [] }), 24).geomChanged).toBe(true);
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, screenCols: 100, rows: [] }), 24).geomChanged).toBe(true);
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, screenCols: 80, rows: [] }), 24).geomChanged).toBe(false);
	});

	it("flags scrollChanged when displayOffset or historySize differ", () => {
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 5, historySize: 100, rows: [] }), 24)
				.scrollChanged,
		).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 0, historySize: 200, rows: [] }), 24)
				.scrollChanged,
		).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, displayOffset: 0, historySize: 100, rows: [] }), 24)
				.scrollChanged,
		).toBe(false);
	});

	it("flags fullReplace when the frame carries >= screenRows rows", () => {
		const rows = Array.from({ length: 24 }, (_, i) => makeRow(i));
		expect(decideFrameGrid(prev, makeFrame({ screenRows: 24, historySize: 100, rows }), 24).fullReplace).toBe(true);
		expect(
			decideFrameGrid(prev, makeFrame({ screenRows: 24, historySize: 100, rows: [makeRow(0)] }), 24).fullReplace,
		).toBe(false);
	});

	it("uses fallbackRows for the full-replace threshold when frame.screenRows is 0", () => {
		const rows = Array.from({ length: 3 }, (_, i) => makeRow(i));
		// frame.screenRows 0 → threshold = fallbackRows (3) → 3 rows is a full replace.
		// geomChanged because 0 !== prev.lastScreenRows (24).
		const d = decideFrameGrid(prev, makeFrame({ screenRows: 0, historySize: 100, rows }), 3);
		expect(d.fullReplace).toBe(true);
	});

	it("flags scrollWait only for a partial frame after a pure scroll (no geom change)", () => {
		const partialScroll = decideFrameGrid(
			prev,
			makeFrame({ screenRows: 24, displayOffset: 5, historySize: 100, rows: [makeRow(0)] }),
			24,
		);
		expect(partialScroll.scrollWait).toBe(true);

		// A geometry change is not a scrollWait even if scroll also changed.
		const geomAndScroll = decideFrameGrid(
			prev,
			makeFrame({ screenRows: 30, displayOffset: 5, historySize: 100, rows: [makeRow(0)] }),
			24,
		);
		expect(geomAndScroll.scrollWait).toBe(false);

		// A full frame after a scroll is a fullReplace, not a scrollWait.
		const fullScroll = decideFrameGrid(
			prev,
			makeFrame({
				screenRows: 24,
				displayOffset: 5,
				historySize: 100,
				rows: Array.from({ length: 24 }, (_, i) => makeRow(i)),
			}),
			24,
		);
		expect(fullScroll.scrollWait).toBe(false);
		expect(fullScroll.fullReplace).toBe(true);
	});
});
