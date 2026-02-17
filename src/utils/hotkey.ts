/**
 * Hotkey format translation between our UI format and Tauri global-shortcut format.
 *
 * UI format uses "Cmd" (e.g. "Cmd+Shift+D", "F5")
 * Tauri uses "CommandOrControl" (e.g. "CommandOrControl+Shift+D", "F5")
 */

/** Modifier mappings from UI/macOS symbols to Tauri global-shortcut format */
const MODIFIER_TO_TAURI: Record<string, string> = {
  Cmd: "CommandOrControl",
  "%": "CommandOrControl",
  "\u2318": "CommandOrControl", // ⌘
  "\u21E7": "Shift",           // ⇧
  "\u2325": "Alt",             // ⌥
  "\u2303": "Ctrl",            // ⌃
};

/** Convert our UI hotkey string to Tauri global-shortcut format */
export function hotkeyToTauriShortcut(hotkey: string): string {
  if (!hotkey) return "";
  return hotkey
    .split("+")
    .map((part) => MODIFIER_TO_TAURI[part] ?? part)
    .join("+");
}

/** All known modifier keys (UI format, macOS symbols, and Tauri format) */
const MODIFIER_KEYS = new Set([
  "Cmd", "Ctrl", "Alt", "Shift", "Super",
  "CommandOrControl",
  "%", "\u2318", "\u21E7", "\u2325", "\u2303", // macOS symbols: ⌘ ⇧ ⌥ ⌃
]);

/** Check whether a hotkey string contains at least one non-modifier key */
export function isValidHotkey(hotkey: string): boolean {
  if (!hotkey) return false;
  return hotkey.split("+").some((part) => !MODIFIER_KEYS.has(part));
}

/** Convert Tauri global-shortcut format back to our UI hotkey string */
export function tauriShortcutToHotkey(shortcut: string): string {
  if (!shortcut) return "";
  return shortcut
    .split("+")
    .map((part) => (part === "CommandOrControl" ? "Cmd" : part))
    .join("+");
}
