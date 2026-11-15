// @vitest-environment node
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

const directory = process.env.TUIC_PENDING_BUBBLE_DIR;
const phase = process.env.TUIC_PENDING_BUBBLE_PHASE ?? "after";
// Layout needs the real browser; happy-dom does not implement line boxes.
describe.skipIf(!directory)("pending bubble controls in the real Transcript", () => {
	type Rect = { left: number; right: number; top: number; bottom: number };
	type Sample = { width: number; bubbles: { box: Rect; send: Rect; cancel: Rect; textRects: Rect[] }[] };
	const samples: Sample[] = [];
	beforeAll(() => {
		const result = spawnSync(process.execPath, ["src/__tests__/browser/chatWrapGeometry.mjs"], {
			encoding: "utf8",
			timeout: 90000,
			env: {
				...process.env,
				TUIC_CHAT_WRAP_DIR: directory,
				TUIC_CHAT_WRAP_PHASE: phase,
				TUIC_CHAT_WRAP_PAGE: "pendingBubbleGeometry",
				TUIC_CHAT_WRAP_WIDTHS: "390,960",
			},
		});
		if (result.status !== 0) throw new Error(result.stderr || String(result.error));
		samples.push(...JSON.parse(readFileSync(join(directory!, `${phase}.json`), "utf8")));
	}, 100000);
	it("pending_bubble_controls_stay_bottom_right — catches controls placed inline after the text", () => {
		for (const sample of samples)
			for (const b of sample.bubbles) {
				const ctx = JSON.stringify({ width: sample.width, b });
				// Right edge: cancel ends within the bubble padding of the right edge.
				expect.soft(b.box.right - b.cancel.right, ctx).toBeLessThanOrEqual(12);
				// Bottom edge: controls sit in the bottom padding band.
				expect.soft(b.box.bottom - b.cancel.bottom, ctx).toBeLessThanOrEqual(12);
				expect.soft(b.box.bottom - b.send.bottom, ctx).toBeLessThanOrEqual(12);
				expect.soft(b.send.right, ctx).toBeLessThanOrEqual(b.cancel.left);
			}
	});
	it("text wraps without running under the controls — catches text rectangles overlapping the control row", () => {
		for (const sample of samples)
			for (const b of sample.bubbles)
				for (const t of b.textRects)
					expect
						.soft(t.bottom, JSON.stringify({ width: sample.width, t, b }))
						.toBeLessThanOrEqual(Math.min(b.send.top, b.cancel.top) + 1);
	});
});
