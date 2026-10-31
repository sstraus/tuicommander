// @vitest-environment node
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

const directory = process.env.TUIC_CHAT_WRAP_DIR;
const phase = process.env.TUIC_CHAT_WRAP_PHASE ?? "after";
// Layout needs the real browser; happy-dom does not implement line boxes.
describe.skipIf(!directory)("terminal chat wrapping in the real Transcript", () => {
	type Rect = { left: number; right: number; width: number; height: number };
	type Sample = {
		width: number;
		row: Rect;
		users: { box: Rect; textRects: Rect[]; text: string }[];
		codes: { text: string; rects: number; box: Rect }[];
		sourceHasNewline: boolean;
	};
	const samples: Sample[] = [];
	beforeAll(() => {
		const result = spawnSync(process.execPath, ["src/__tests__/browser/chatWrapGeometry.mjs"], {
			encoding: "utf8",
			timeout: 90000,
			env: process.env,
		});
		if (result.status !== 0) throw new Error(result.stderr || String(result.error));
		samples.push(...JSON.parse(readFileSync(join(directory!, `${phase}.json`), "utf8")));
	}, 100000);
	it("wraps long prompts inside their bubble — catches text clipped despite a constrained bubble", () => {
		for (const sample of samples)
			for (const user of sample.users) {
				expect.soft(user.box.right).toBeLessThanOrEqual(sample.row.right);
				for (const rect of user.textRects)
					expect
						.soft(rect.right, JSON.stringify({ width: sample.width, user }))
						.toBeLessThanOrEqual(user.box.right + 1);
			}
	});
	it("keeps a short inline code token whole — catches shell-like word breaking inside --session", () => {
		for (const sample of samples)
			for (const code of sample.codes) expect.soft(code.rects, JSON.stringify(code)).toBe(1);
	});
	it("preserves deliberate user line breaks without adding PTY-width wrapping", () => {
		for (const sample of samples) {
			expect(sample.sourceHasNewline).toBe(false);
			expect(sample.users[2].text).toContain("newline.\nAnd this second line.");
		}
	});
});
