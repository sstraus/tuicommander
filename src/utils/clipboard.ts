import { invoke } from "../invoke";
import { shortenHomePath } from "../platform";
import { appLogger } from "../stores/appLogger";
import { isTauri } from "../transport";

/**
 * Write text to the system clipboard.
 *
 * Inside the Tauri webview we route through the native arboard command
 * instead of navigator.clipboard. WKWebView rejects navigator.clipboard.writeText
 * with NotAllowedError whenever the document isn't focused or the transient user
 * activation has already been consumed by an intervening await — exactly what the
 * terminal copy paths do (they await an IPC round-trip to fetch the selection text
 * before writing). The native command has no focus / user-gesture requirement.
 *
 * Browser mode has no such native command, so it keeps navigator.clipboard, which
 * behaves correctly in a normal (focused, secure-context) browser tab.
 *
 * Throws on failure so callers can surface a "copy failed" status.
 */
export async function writeClipboard(text: string): Promise<void> {
	if (isTauri()) {
		await invoke("write_clipboard_text", { text });
		return;
	}
	await navigator.clipboard.writeText(text);
}

/**
 * Copy a filesystem path to the clipboard, logging a failure instead of throwing.
 *
 * The path is ABSOLUTE and home-shortened to `~/…`. Both halves are load-bearing:
 * a relative path resolves against whatever cwd the consumer happens to have, and
 * `~` is what Boss wants to read back. Every "Copy Path" action routes through
 * here so the two rules hold in one place — callers pass the full path and do not
 * shorten it themselves.
 */
export function copyPathToClipboard(absolutePath: string): void {
	writeClipboard(shortenHomePath(absolutePath)).catch((err) => appLogger.error("app", "Failed to copy path", err));
}

/**
 * Read text from the system clipboard.
 *
 * Inside the Tauri webview we route through the native arboard command
 * instead of navigator.clipboard.readText(). On macOS (Sequoia+), the WKWebView
 * Web Clipboard read API surfaces a system "Paste" confirmation pill floating over
 * the page — visually colliding with our own context menu's Paste item. The native
 * command reads the pasteboard directly with no such affordance.
 *
 * Browser mode has no such native command, so it keeps navigator.clipboard.
 */
export async function readClipboard(): Promise<string> {
	if (isTauri()) {
		return await invoke<string>("read_clipboard_text");
	}
	return await navigator.clipboard.readText();
}
