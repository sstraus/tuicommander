// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

// Catches: black-translucent leaves the bottom 62pt outside the drawable and
// tappable content area on the recorded iOS standalone runtime. Growing the
// CSS shell clips the composer instead; a fresh default-status-bar install
// fixes the native content origin. See fixtures/ios-standalone-2116.md.
// This protects the shipping HTML contract; jsdom does not emulate WebKit.
it("uses a non-overlay iOS status bar so standalone content reaches the screen bottom", () => {
	const html = new DOMParser().parseFromString(readFileSync("mobile.html", "utf8"), "text/html");
	expect(html.querySelector('meta[name="apple-mobile-web-app-status-bar-style"]')?.getAttribute("content")).toBe(
		"default",
	);
});
