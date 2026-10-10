const DESKTOP_UI_KEY = "tuic-desktop-ui";

/** Device-local navigation choice, shared by browser and installed PWA entry points. */
export function prefersDesktopUI(): boolean {
	try {
		return localStorage.getItem(DESKTOP_UI_KEY) === "1";
	} catch {
		// Browsers that deny storage must still be able to boot the default UI.
		return false;
	}
}

export function setDesktopUI(preferred: boolean): void {
	if (preferred) localStorage.setItem(DESKTOP_UI_KEY, "1");
	else localStorage.removeItem(DESKTOP_UI_KEY);
}

/** iPad Safari requests the desktop shell with a Mac user agent. */
export function isTabletUA(userAgent: string, maxTouchPoints: number): boolean {
	return /iPad|Macintosh/.test(userAgent) && maxTouchPoints > 1;
}

export function tabletAppDestination(
	userAgent: string,
	maxTouchPoints: number,
	pathname: string,
	search: string,
	hash: string,
	native: boolean,
	desktopPreferred = false,
): string | null {
	if (native || desktopPreferred || pathname !== "/" || /^#\/secret-form(?:\?|$)/.test(hash)) return null;
	if (!isTabletUA(userAgent, maxTouchPoints)) return null;
	return `/mobile${search}${hash}`;
}
