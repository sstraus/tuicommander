// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockOpenUrl } = vi.hoisted(() => ({ mockOpenUrl: vi.fn() }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: mockOpenUrl }));

import { handleExternalLinkClick } from "../../utils/externalLinkClick";

function click(href: string): { defaultPrevented: boolean } {
	document.body.innerHTML = "";
	const a = document.createElement("a");
	a.setAttribute("href", href);
	a.textContent = "x";
	document.body.appendChild(a);
	const event = new MouseEvent("click", { bubbles: true, cancelable: true });
	let defaultPrevented = false;
	a.addEventListener("click", handleExternalLinkClick);
	a.addEventListener("click", (e) => {
		defaultPrevented = e.defaultPrevented;
		e.preventDefault(); // Do not navigate after testing the handler.
	});
	a.dispatchEvent(event);
	return { defaultPrevented };
}

describe("handleExternalLinkClick round 3", () => {
	beforeEach(() => mockOpenUrl.mockClear());
	afterEach(() => {
		document.body.innerHTML = "";
	});

	it("intercepts an http href that the URL parser normalises (embedded tab/newline, leading control char)", () => {
		// Catches: deciding on the raw attribute text, so `ht\ntp://x` (which the browser
		// resolves to http://x) skips the handler and reaches the WebView's
		// "link could be dangerous" navigation, a regression from the old `anchor.href`.
		for (const href of ["ht\ntp://example.com/a", "htt\tps://example.com/a", "\u0001https://example.com/a"]) {
			mockOpenUrl.mockClear();
			const event = click(href);
			expect(event.defaultPrevented, JSON.stringify(href)).toBe(true);
			expect(mockOpenUrl, JSON.stringify(href)).toHaveBeenCalledTimes(1);
		}
	});

	it("leaves relative links whose name merely starts with http alone", () => {
		// Catches: a regex loosened to /^https?/ (no colon), hijacking `https.md` and `http-notes/a.md`.
		for (const href of ["https.md", "http-notes/a.md", "httpx://a", "https+x:a", "#http:x", "./http:x"]) {
			const event = click(href);
			expect(event.defaultPrevented, href).toBe(false);
		}
		expect(mockOpenUrl).not.toHaveBeenCalled();
	});

	it("does not intercept mailto or other schemes (document handler is http(s) only)", () => {
		// Catches: widening the document handler to every allowed scheme, double-opening mailto.
		for (const href of ["mailto:a@b.c", "javascript:alert(1)", "file:///etc/hosts"]) {
			expect(click(href).defaultPrevented, href).toBe(false);
		}
		expect(mockOpenUrl).not.toHaveBeenCalled();
	});
});
