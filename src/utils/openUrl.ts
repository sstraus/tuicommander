import type { ConfirmOptions } from "../hooks/useConfirmDialog";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import { isTauri } from "../transport";
import { isHandlerLink, isUncPath } from "./handlerLink";

export { isHandlerLink } from "./handlerLink";

const ALLOWED_SCHEMES = new Set(["http:", "https:", "mailto:"]);

/** Open a URL in the system browser, using Tauri shell in native mode or window.open in browser mode.
 *  Only allows http/https/mailto schemes — terminal output is untrusted and arbitrary
 *  URI schemes (file://, smb://, custom protocols) could invoke OS handlers. Those
 *  go through `openConfirmedLink`, which asks the owner first. */
export function handleOpenUrl(url: string): void {
	try {
		const parsed = new URL(url);
		if (!ALLOWED_SCHEMES.has(parsed.protocol)) {
			appLogger.warn("app", `Blocked URL with disallowed scheme: ${parsed.protocol}${url.slice(0, 80)}`);
			return;
		}
	} catch {
		appLogger.warn("app", `Blocked malformed URL: ${url.slice(0, 80)}`);
		return;
	}
	if (isTauri()) {
		invoke("open_default_app", { target: url, kind: "url" }).catch((err) =>
			appLogger.error("app", "Failed to open URL", err),
		);
	} else {
		window.open(url, "_blank");
	}
}

type LinkConfirm = (options: ConfirmOptions) => Promise<boolean>;
let linkConfirm: LinkConfirm | null = null;

/** The app shell registers its confirmation dialog; without one, handler links stay closed. */
export function registerLinkConfirm(confirm: LinkConfirm | null): void {
	linkConfirm = confirm;
}

function reportOpenFailure(target: string, message: string): void {
	appLogger.error("app", "Failed to open link with the system handler", { target, error: message });
	toastsStore.add("Could not open link", message, "error");
}

/**
 * Open a link the owner clicked. Web and mail links open as before. A network
 * path or custom-scheme link opens with the system handler only after the owner
 * confirms the exact target in a dialog; callers must run this from a click, never
 * from content arriving on its own.
 */
export async function openConfirmedLink(target: string): Promise<void> {
	if (/^(https?|mailto):/i.test(target)) {
		handleOpenUrl(target);
		return;
	}
	if (!isHandlerLink(target)) {
		appLogger.warn("app", `Blocked link without a system handler route: ${target.slice(0, 80)}`);
		return;
	}
	if (!linkConfirm) {
		appLogger.warn("app", `Blocked handler link: no confirmation dialog available: ${target.slice(0, 80)}`);
		return;
	}
	const network = isUncPath(target);
	const confirmed = await linkConfirm({
		title: network ? "Open network location?" : "Open with system handler?",
		message: `${network ? "TUICommander will ask the system to open this network location:" : "TUICommander will pass this link to the application registered for it:"}\n\n${target}\n\nOpen it only if you trust where it came from.`,
		okLabel: "Open",
		cancelLabel: "Cancel",
		kind: "warning",
		defaultButton: "cancel",
	});
	if (!confirmed) return;
	if (!isTauri()) {
		if (network) {
			reportOpenFailure(target, "Network paths open only in the desktop app.");
			return;
		}
		window.open(target, "_blank");
		return;
	}
	try {
		await invoke("open_default_app", { target, kind: "handler" });
	} catch (err) {
		reportOpenFailure(target, String(err));
	}
}

/**
 * Open a local file or directory with the OS default application.
 *
 * Deliberately NOT `handleOpenUrl`. That allowlist exists because terminal
 * output is untrusted: a `file://` scraped off a PTY could name anything and
 * hand it to an OS handler. A path the app already holds is not that input — the
 * tab is rendering the file's contents on screen — and blocking it there only
 * produced a menu item that silently did nothing. Route the trusted case through
 * its own door instead of widening the guard on the untrusted one.
 */
export function openLocalPath(path: string): void {
	if (!isTauri()) {
		appLogger.warn("app", `Cannot open a local path in browser mode: ${path.slice(0, 120)}`);
		return;
	}
	invoke("open_default_app", { target: path, kind: "path" }).catch((err) =>
		appLogger.error("app", "Failed to open path externally", { path, error: String(err) }),
	);
}
