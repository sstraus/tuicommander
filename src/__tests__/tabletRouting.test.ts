import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { prefersDesktopUI, setDesktopUI, tabletAppDestination } from "../utils/tabletRouting";

describe("tablet app routing", () => {
	beforeEach(() => localStorage.clear());
	// Catches: Open Desktop UI immediately redirects an iPad back to mobile.
	it("honors an explicit desktop choice on a tablet", () => {
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "", "", false, true)).toBeNull();
	});
	// Catches: the preference disappears between documents or remains after switching back.
	it("persists the desktop choice and clears it for the return to mobile", () => {
		expect(prefersDesktopUI()).toBe(false);
		setDesktopUI(true);
		expect(prefersDesktopUI()).toBe(true);
		expect(tabletAppDestination("iPad Safari", 5, "/", "", "", false, prefersDesktopUI())).toBeNull();
		setDesktopUI(false);
		expect(prefersDesktopUI()).toBe(false);
		expect(tabletAppDestination("iPad Safari", 5, "/", "", "", false, prefersDesktopUI())).toBe("/mobile");
	});
	// Catches: denying browser storage crashes the entry point before either UI mounts.
	it("defaults to mobile when browser storage is unavailable", () => {
		const read = vi.spyOn(localStorage, "getItem").mockImplementation(() => {
			throw new DOMException("Storage denied", "SecurityError");
		});
		try {
			expect(prefersDesktopUI()).toBe(false);
		} finally {
			read.mockRestore();
		}
	});
	// Catches: installed PWA navigation to desktop leaves its scope and opens another browser context.
	it("keeps both shells inside the installed PWA scope", () => {
		const manifest = JSON.parse(readFileSync("public/mobile-manifest.json", "utf8"));
		expect(manifest.scope).toBe("/");
		expect(manifest.start_url).toBe("/mobile");
	});
	// Catches: desktop-mode iPad Safari mounts the desktop UI instead of the touch UI.
	it("routes a touch-capable Mac UA and preserves navigation parameters", () => {
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "?shared=k", "#sessions", false)).toBe(
			"/mobile?shared=k#sessions",
		);
		expect(tabletAppDestination("iPad Safari", 5, "/", "", "", false)).toBe("/mobile");
	});
	// Catches: ordinary Mac browsers or native WebViews get redirected away from desktop features.
	it("keeps desktop and native clients on their current shell", () => {
		expect(tabletAppDestination("Macintosh Safari", 0, "/", "", "", false)).toBeNull();
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "", "", true)).toBeNull();
		expect(tabletAppDestination("Android", 5, "/", "", "", false)).toBeNull();
	});
	// Catches: routing loops or an iPad secret-form link loses its standalone UI.
	it("keeps mobile routes and secret forms intact", () => {
		expect(tabletAppDestination("Macintosh Safari", 5, "/mobile", "", "", false)).toBeNull();
		for (const preferred of [false, true]) {
			expect(tabletAppDestination("iPad Safari", 5, "/mobile/session/abc", "", "", false, preferred)).toBeNull();
			expect(tabletAppDestination("iPad Safari", 5, "/", "", "", true, preferred)).toBeNull();
			expect(
				tabletAppDestination("iPad Safari", 5, "/", "", "#/secret-form?request_id=a", false, preferred),
			).toBeNull();
		}
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "", "#/secret-form?request_id=a", false)).toBeNull();
	});
});
