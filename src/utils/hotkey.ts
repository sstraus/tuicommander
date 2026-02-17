/**
 * Hotkey format translation between our UI format and Tauri global-shortcut format.
 *
 * UI format uses "Cmd" (e.g. "Cmd+Shift+D", "F5")
 * Tauri uses "CommandOrControl" (e.g. "CommandOrControl+Shift+D", "F5")
 */

/** Convert our UI hotkey string to Tauri global-shortcut format */
export function hotkeyToTauriShortcut(hotkey: string): string {
  if (!hotkey) return "";
  return hotkey
    .split("+")
    .map((part) => (part === "Cmd" ? "CommandOrControl" : part))
    .join("+");
}

/** Convert Tauri global-shortcut format back to our UI hotkey string */
export function tauriShortcutToHotkey(shortcut: string): string {
  if (!shortcut) return "";
  return shortcut
    .split("+")
    .map((part) => (part === "CommandOrControl" ? "Cmd" : part))
    .join("+");
}
