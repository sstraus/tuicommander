import { beforeAll, describe, expect, it } from "vitest";
import { type ChatAppearance, captureChatReadability } from "./captureChatReadability";

// Required local landing gate for Chat CSS: make check-chat-readability.
// It starts fixture Vite on a free port, supplies these variables, and traps cleanup.
// Ubuntu CI cannot use the approved macOS-only stealth wrapper; ordinary Vitest keeps this opt-in.
// Direct use with an existing fixture Vite: TUIC_CHAT_BROWSER_URL=... TUIC_CHAT_EVIDENCE_DIR=... pnpm vitest run src/__tests__/browser/ChatReadability.test.ts
const url = process.env.TUIC_CHAT_BROWSER_URL;
describe.skipIf(!url)("browser-rendered Chat readability", () => {
	let appearances: ChatAppearance[];
	beforeAll(() => {
		if (!process.env.TUIC_CHAT_EVIDENCE_DIR) throw new Error("TUIC_CHAT_EVIDENCE_DIR is required");
		appearances = captureChatReadability(
			url!,
			process.env.TUIC_CHAT_EVIDENCE_DIR,
			process.env.TUIC_CHAT_CAPTURE_PHASE ?? "current",
		);
	}, 240000);

	it("keeps intermediate prose on the normal foreground — catches prose kept on the muted colour", () => {
		expect(appearances).toHaveLength(4);
		for (const appearance of appearances) {
			for (const sample of [appearance.terminal, appearance.acp]) {
				expect.soft(sample.color, JSON.stringify(appearance)).toBe(sample.foreground);
			}
		}
	});

	it("keeps answer list markers inside the text inset — catches list markers drawn over the rule", () => {
		expect(appearances).toHaveLength(4);
		for (const appearance of appearances) {
			expect(appearance.lists.map((list) => list.tag)).toEqual(["UL", "OL"]);
			expect(parseFloat(appearance.unmarkedBorder)).toBe(0);
			for (const list of appearance.lists) {
				expect(list.border).toBe(3);
				// The outside marker needs the normal Markdown 2em gutter in addition
				// to the answer's 13px text inset; measured item/text bounds must agree.
				expect.soft(list.padding, JSON.stringify(appearance)).toBeGreaterThanOrEqual(13 + 2 * list.font);
				expect.soft(list.itemLeft - list.ruleRight).toBeGreaterThanOrEqual(13 + 2 * list.font);
				expect(list.textLeft).toBeGreaterThanOrEqual(list.itemLeft);
			}
		}
	});
});
