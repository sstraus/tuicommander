/** Detect whether we're running inside a Tauri webview */
export function isTauri(): boolean {
	return "__TAURI_INTERNALS__" in globalThis && !(globalThis as Record<string, unknown>).__TAURI_SHIM__;
}
