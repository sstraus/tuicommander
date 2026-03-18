/**
 * Hotkey format utilities and matching logic for tauri-plugin-user-input events.
 *
 * UI format uses "Cmd" (e.g. "Cmd+Shift+D", "F5")
 * user-input plugin uses Key enum strings (e.g. "KeyD", "F5", "Space", "MetaLeft")
 */

import { getModifierSymbol, isMacOS } from "../platform";

/** All known modifier keys (UI format and macOS symbols) */
const MODIFIER_KEYS = new Set([
  "Cmd", "Ctrl", "Alt", "Shift", "Super",
  "%", "\u2318", "\u21E7", "\u2325", "\u2303", // macOS symbols: ⌘ ⇧ ⌥ ⌃
]);

/** Check whether a hotkey string contains at least one non-modifier key */
export function isValidHotkey(hotkey: string): boolean {
  if (!hotkey) return false;
  return hotkey.split("+").some((part) => !MODIFIER_KEYS.has(part));
}

/**
 * Convert a keybinding combo string (e.g. "Cmd+Shift+D") to a display string
 * using platform-appropriate symbols (e.g. "⌘⇧D" on macOS, "Ctrl+Shift+D" on others).
 */
export function comboToDisplay(combo: string): string {
  if (!combo) return "";
  const mod = getModifierSymbol();
  const mac = isMacOS();

  const parts = combo.split("+");
  const key = parts.pop()!;
  const modifiers = parts;

  const displayParts: string[] = [];
  for (const m of modifiers) {
    switch (m) {
      case "Cmd": displayParts.push(mod); break;
      case "Shift": displayParts.push(mac ? "\u21E7" : "Shift+"); break;
      case "Alt": displayParts.push(mac ? "\u2325" : "Alt+"); break;
      case "Ctrl": displayParts.push(mac ? "^" : "Ctrl+"); break;
      default: displayParts.push(m + "+"); break;
    }
  }

  displayParts.push(key.toUpperCase());
  return displayParts.join("");
}

// --- tauri-plugin-user-input key matching ---

/** Map from UI hotkey key name to user-input plugin Key string */
const UI_KEY_TO_PLUGIN: Record<string, string> = {
  Space: "Space", Tab: "Tab", Enter: "Enter", Escape: "Escape",
  Backspace: "Backspace", Delete: "Delete", Insert: "Insert",
  Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
  ArrowUp: "ArrowUp", ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight",
  CapsLock: "CapsLock", NumLock: "NumLock", ScrollLock: "ScrollLock",
  PrintScreen: "PrintScreen", Pause: "Pause",
};
// F1–F24
for (let i = 1; i <= 24; i++) UI_KEY_TO_PLUGIN[`F${i}`] = `F${i}`;

/** Convert a single non-modifier key from UI format to plugin Key string */
function uiKeyToPluginKey(uiKey: string): string {
  if (UI_KEY_TO_PLUGIN[uiKey]) return UI_KEY_TO_PLUGIN[uiKey];
  // Single letter → "KeyA"
  if (uiKey.length === 1 && /^[A-Z]$/i.test(uiKey)) return `Key${uiKey.toUpperCase()}`;
  // Single digit → "Num0"
  if (uiKey.length === 1 && /^[0-9]$/.test(uiKey)) return `Num${uiKey}`;
  // Punctuation keys
  const PUNCT: Record<string, string> = {
    "`": "Grave", "-": "Minus", "=": "Equal",
    "[": "BracketLeft", "]": "BracketRight", "\\": "Backslash",
    ";": "Semicolon", "'": "Quote", ",": "Comma", ".": "Period", "/": "Slash",
  };
  if (PUNCT[uiKey]) return PUNCT[uiKey];
  return uiKey; // fallback: pass through as-is
}

/** Modifier key names from the user-input plugin */
const PLUGIN_MODIFIER_KEYS = new Set([
  "MetaLeft", "MetaRight", "ShiftLeft", "ShiftRight",
  "ControlLeft", "ControlRight", "AltLeft", "AltRight",
]);

/** Check whether a user-input plugin key is a modifier */
export function isPluginModifierKey(key: string): boolean {
  return PLUGIN_MODIFIER_KEYS.has(key);
}

/** Parsed hotkey: the primary key + required modifier flags */
export interface ParsedHotkey {
  /** Plugin Key string for the primary (non-modifier) key */
  key: string;
  needCmd: boolean;
  needShift: boolean;
  needAlt: boolean;
  needCtrl: boolean;
}

/** Parse a UI hotkey string into a structured form for matching */
export function parseHotkey(hotkey: string): ParsedHotkey | null {
  if (!hotkey || !isValidHotkey(hotkey)) return null;
  const parts = hotkey.split("+");
  let key = "";
  let needCmd = false, needShift = false, needAlt = false, needCtrl = false;
  for (const p of parts) {
    if (p === "Cmd" || p === "%" || p === "\u2318") needCmd = true;
    else if (p === "Shift" || p === "\u21E7") needShift = true;
    else if (p === "Alt" || p === "\u2325") needAlt = true;
    else if (p === "Ctrl" || p === "\u2303") needCtrl = true;
    else key = p;
  }
  if (!key) return null;
  return { key: uiKeyToPluginKey(key), needCmd, needShift, needAlt, needCtrl };
}

/** Tracked modifier state from user-input plugin events */
export interface ModifierState {
  cmd: boolean;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
}

/** Update modifier state from a plugin key event */
export function updateModifierState(state: ModifierState, key: string, pressed: boolean): void {
  switch (key) {
    case "MetaLeft": case "MetaRight": state.cmd = pressed; break;
    case "ShiftLeft": case "ShiftRight": state.shift = pressed; break;
    case "AltLeft": case "AltRight": state.alt = pressed; break;
    case "ControlLeft": case "ControlRight": state.ctrl = pressed; break;
  }
}

/** Check if the current modifier state matches the hotkey requirements */
export function modifiersMatch(parsed: ParsedHotkey, mods: ModifierState): boolean {
  return parsed.needCmd === mods.cmd
    && parsed.needShift === mods.shift
    && parsed.needAlt === mods.alt
    && parsed.needCtrl === mods.ctrl;
}
