import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import { readStylesheet } from "../../__tests__/fixtures/readStylesheet";

// Catches: the mobile entry point not loading a token the badge uses (--unseen was desktop-only,
// so the unseen Finished badge rendered with no colour on /mobile).
it("defines every colour token the mobile StatusBadge uses in the mobile stylesheet", () => {
	const badge = readFileSync("src/mobile/components/StatusBadge.module.css", "utf8");
	const used = new Set([...badge.matchAll(/var\((--[\w-]+)\s*[,)]/g)].map((m) => m[1]));
	const mobile = readStylesheet("src/mobile/mobile.css");
	const missing = [...used].filter((token) => !new RegExp(`${token}\\s*:`).test(mobile));
	expect(missing).toEqual([]);
});

// Catches: the mobile --unseen drifting from the desktop purple the status contract expects.
it("uses the same --unseen colour as the desktop stylesheet", () => {
	const value = (css: string) => css.match(/--unseen:\s*([^;]+);/)?.[1];
	expect(value(readStylesheet("src/mobile/mobile.css"))).toBe(value(readStylesheet("src/global.css")));
});
