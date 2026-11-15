// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockOpenUrl } = vi.hoisted(() => ({ mockOpenUrl: vi.fn() }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: mockOpenUrl }));

import { handleExternalLinkClick } from "../../utils/externalLinkClick";

function click(href: string): Pick<MouseEvent, "defaultPrevented"> {
	document.body.innerHTML = "";
	const a = document.createElement("a");
	a.setAttribute("href", href);
	a.textContent = "x";
	document.body.appendChild(a);
	const event = new MouseEvent("click", { bubbles: true, cancelable: true });
	let defaultPrevented = false;
	a.addEventListener("click", (e) => {
		handleExternalLinkClick(e);
		defaultPrevented = e.defaultPrevented;
		// Observe the handler first, then stop jsdom's unrelated navigation default action.
		e.preventDefault();
	});
	a.dispatchEvent(event);
	return { defaultPrevented };
}

describe("handleExternalLinkClick round 4", () => {
	beforeEach(() => mockOpenUrl.mockClear());
	afterEach(() => {
		document.body.innerHTML = "";
	});

	it("does not queue jsdom navigation timers for unhandled links", () => {
		// Catches: anchor default navigation leaves a Timeout after the helper returns.
		vi.useFakeTimers();
		try {
			expect(click("docs/local").defaultPrevented).toBe(false);
			expect(vi.getTimerCount()).toBe(0);
		} finally {
			vi.clearAllTimers();
			vi.useRealTimers();
		}
	});

	it("leaves hrefs the URL parser keeps relative", () => {
		// Catches: normalising with /\s/ or trim() (strips U+00A0, U+FEFF, U+2028, which the URL
		// parser does NOT strip, so the browser resolves these as relative paths) or deleting
		// every C0 control / space anywhere instead of only tab/CR/LF.
		for (const href of [
			"\u00a0https://example.com/a",
			"\ufeffhttps://example.com/a",
			"\u2028https://example.com/a",
			"ht tp://example.com/a",
			"ht\u0001tp://example.com/a",
			"https\u00a0://example.com/a",
			"https ://example.com/a",
			"\u0000mailto:a@b.c",
			" \tfile:///etc/hosts",
			"\n javascript:alert(1)",
		]) {
			const event = click(href);
			expect(event.defaultPrevented, JSON.stringify(href)).toBe(false);
		}
		expect(mockOpenUrl).not.toHaveBeenCalled();
	});

	it("intercepts every leading C0/space and embedded tab/CR/LF combination", () => {
		// Catches: a leading-trim class that stops short of U+0000 or U+001F, or tab/CR/LF removal
		// that is applied only before the trim (leaving "\t http:" blocked) or only for \n.
		for (const href of [
			"\u0000https://example.com/a",
			"\u001fhttps://example.com/a",
			"\u000bhttp://example.com/a",
			"\f http://example.com/a",
			"\t \n https://example.com/a",
			"h\rt\nt\tp://example.com/a",
			"https\n://example.com/a",
			"HtTpS:\n//example.com/a",
			"https://example.com/a \n",
		]) {
			mockOpenUrl.mockClear();
			const event = click(href);
			expect(event.defaultPrevented, JSON.stringify(href)).toBe(true);
			expect(mockOpenUrl, JSON.stringify(href)).toHaveBeenCalledTimes(1);
		}
	});

	it("hands the normalised URL, not the raw attribute, to the opener", () => {
		// Catches: passing getAttribute() text with embedded tab/newline to tauriOpenUrl /
		// window.open, so the opener plugin scope check or the OS receives "ht\ntp://…".
		click("\u0001 ht\ntp\ts://example.com/a\r\n");
		expect(mockOpenUrl).toHaveBeenCalledWith("https://example.com/a");
	});

	it("does not intercept a relative link that merely contains http: later in the text", () => {
		// Catches: unanchored regex after normalisation (e.g. removing leading text before 'http').
		for (const href of ["docs/http://x", "\t./https://x", "?u=https://x", "x\nhttps://x"]) {
			expect(click(href).defaultPrevented, JSON.stringify(href)).toBe(false);
		}
		expect(mockOpenUrl).not.toHaveBeenCalled();
	});
});
