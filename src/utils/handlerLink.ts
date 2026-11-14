/** Schemes that name WebView or local content, never an OS handler document.
 *  `file:` has its own trusted route (`openLocalPath`). The backend refuses the same set. */
const NON_HANDLER_SCHEMES = new Set(["javascript", "vbscript", "data", "blob", "about", "file"]);

/** `\\host\share…` or `//host/share…` with a host; not the `\\?\` / `\\.\` device namespaces. */
export function isUncPath(target: string): boolean {
	return /^[\\/]{2}[^\\/?.][^\\/]*(?:[\\/]|$)/.test(target);
}

/**
 * True for a link only an OS handler can open: a network (UNC) path or a URL with
 * a custom scheme such as `editor://` or `smb://`. A one-letter scheme is a drive
 * path, and `name:42` is a file-line reference, not a scheme.
 */
export function isHandlerLink(target: string): boolean {
	if (isUncPath(target)) return true;
	const match = /^([a-z][a-z\d+.-]+):(.+)$/is.exec(target);
	if (!match || /^\d+(:\d+)?$/.test(match[2])) return false;
	return !NON_HANDLER_SCHEMES.has(match[1].toLowerCase()) && !/[\s\u0000-\u001f]/.test(target);
}
