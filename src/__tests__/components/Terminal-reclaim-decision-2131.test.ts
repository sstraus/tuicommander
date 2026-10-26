import { expect, it } from "vitest";
import { decideSizeReclaim } from "../../components/Terminal/canvasTerminalUtils";

const mine = { rows: 14, cols: 37 };

// Catches: a viewer that already owns the size sends a request per focus or keystroke (SIGWINCH storm).
it("sends nothing when the child already has this viewer's size, and forgets the earlier ask", () => {
	expect(decideSizeReclaim({ screenRows: 14, screenCols: 37 }, mine, "30x100")).toEqual({ send: false, askedFor: "" });
});

// Catches: reclaim never fires, or fires again for the same foreign size before the new frame arrives.
it("asks once per foreign size", () => {
	const foreign = { screenRows: 30, screenCols: 100 };
	const first = decideSizeReclaim(foreign, mine, "");
	expect(first).toEqual({ send: true, askedFor: "30x100" });
	expect(decideSizeReclaim(foreign, mine, first.askedFor).send).toBe(false);
});

// Catches: a second viewer taking the size again after a reclaim is ignored forever.
it("asks again when the foreign size changes or returns after a reclaim", () => {
	expect(decideSizeReclaim({ screenRows: 20, screenCols: 90 }, mine, "30x100").send).toBe(true);
	const back = decideSizeReclaim({ screenRows: 14, screenCols: 37 }, mine, "30x100");
	expect(decideSizeReclaim({ screenRows: 30, screenCols: 100 }, mine, back.askedFor).send).toBe(true);
});

// Catches: a viewer with no measured box (0x0) or a partial frame (0 dims) sends a 0-size resize.
it("never sends while either size is unknown", () => {
	expect(decideSizeReclaim({ screenRows: 0, screenCols: 0 }, mine, "").send).toBe(false);
	expect(decideSizeReclaim({ screenRows: 30, screenCols: 100 }, { rows: 0, cols: 0 }, "").send).toBe(false);
});
