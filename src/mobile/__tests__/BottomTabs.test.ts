import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/BottomTabs.module.css"), "utf-8");

describe("BottomTabs CSS", () => {
	it("defines a real (non-transparent) background", () => {
		// Must be a genuine fill, not `none`/`transparent` — without the frosted
		// blur the bar relies on its background for legibility over content.
		const match = css.match(/background:\s*(?!none|transparent\b)[^;]+/);
		expect(match, "no opaque background found in BottomTabs.module.css").toBeTruthy();
	});

	it("does not use backdrop-filter (glassmorphism ban)", () => {
		const match = css.match(/backdrop-filter/);
		expect(match).toBeNull();
	});
});
