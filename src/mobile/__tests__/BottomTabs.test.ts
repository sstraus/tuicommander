import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/BottomTabs.module.css"), "utf-8");

describe("BottomTabs CSS", () => {
	it("has a background defined", () => {
		const match = css.match(/background:\s*[^;]+/);
		expect(match, "background not found in BottomTabs.module.css").toBeTruthy();
	});

	it("does not use backdrop-filter (glassmorphism ban)", () => {
		const match = css.match(/backdrop-filter/);
		expect(match).toBeNull();
	});
});
