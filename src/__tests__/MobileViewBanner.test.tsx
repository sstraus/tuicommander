import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MobileViewBanner } from "../components/MobileViewBanner";
import { prefersDesktopUI, setDesktopUI } from "../utils/tabletRouting";

describe("mobile view return link", () => {
	beforeEach(() => {
		localStorage.clear();
		vi.stubGlobal("__TAURI_SHIM__", true);
		vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Macintosh Safari");
		vi.spyOn(navigator, "maxTouchPoints", "get").mockReturnValue(5);
	});
	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});
	// Catches: a dismissed phone banner hides the tablet's only way back.
	it("always offers tablets a return link and clears the desktop choice", () => {
		localStorage.setItem("tuic-mobile-banner-dismissed", "1");
		setDesktopUI(true);
		const view = render(() => <MobileViewBanner />);
		const link = view.getByRole("link", { name: "Switch" });
		expect(link.getAttribute("href")).toBe("/mobile");
		expect(view.queryByRole("button", { name: "Dismiss mobile banner" })).toBeNull();
		fireEvent.click(link);
		expect(prefersDesktopUI()).toBe(false);
	});
	// Catches: tablets inside native Tauri gain an irrelevant web navigation banner.
	it("never renders in native Tauri", () => {
		vi.stubGlobal("__TAURI_SHIM__", false);
		const view = render(() => <MobileViewBanner />);
		expect(view.queryByRole("link")).toBeNull();
	});
	// Catches: ordinary desktop browsers gain the tablet banner.
	it("keeps non-touch Macs on desktop without a banner", () => {
		vi.spyOn(navigator, "maxTouchPoints", "get").mockReturnValue(0);
		const view = render(() => <MobileViewBanner />);
		expect(view.queryByRole("link")).toBeNull();
	});
});
