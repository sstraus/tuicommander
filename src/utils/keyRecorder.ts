/**
 * Convert a KeyboardEvent into a combo string matching keybindingDefaults format.
 * Returns null if the event is modifier-only (no actual key pressed).
 *
 * Format: "Cmd+Shift+D", "Cmd+\\", "F5", etc.
 * - "Cmd" = Meta on macOS, Ctrl on Win/Linux (platform-agnostic primary modifier)
 * - "Ctrl" = literal Control key (distinct from Cmd on macOS)
 * - "Alt", "Shift" = standard modifiers
 */

import { isMacOS } from "../platform";

/** Keys that are modifiers and should not be treated as the main key */
const MODIFIER_CODES = new Set([
  "ShiftLeft", "ShiftRight",
  "ControlLeft", "ControlRight",
  "AltLeft", "AltRight",
  "MetaLeft", "MetaRight",
]);

/** Map KeyboardEvent.key values to our combo key names */
const KEY_MAP: Record<string, string> = {
  " ": "Space",
  "ArrowUp": "Up",
  "ArrowDown": "Down",
  "ArrowLeft": "Left",
  "ArrowRight": "Right",
  "Backspace": "Backspace",
  "Delete": "Delete",
  "Enter": "Enter",
  "Tab": "Tab",
  "Escape": "Escape",
};

/**
 * Keys recognized by the `global-hotkey` crate's `Shortcut::from_str` parser.
 * Single-char punctuation maps to the name the parser expects.
 * Letters (A-Z), digits (0-9), and function keys (F1-F24) are always valid.
 */
const GLOBAL_HOTKEY_KEYS = new Set([
  // Named keys
  "Backspace", "Tab", "Enter", "Space", "Escape",
  "Delete", "End", "Home", "Insert", "PageDown", "PageUp",
  "PrintScreen", "ScrollLock", "CapsLock", "Pause",
  "Up", "Down", "Left", "Right",
  "NumLock",
  // Named numpad keys
  "NumpadAdd", "NumpadDecimal", "NumpadDivide", "NumpadEnter",
  "NumpadEqual", "NumpadMultiply", "NumpadSubtract",
  // Fn (captured separately via native event, not KeyboardEvent)
  "Fn",
]);

/** Punctuation characters → global-hotkey parser name */
const CHAR_TO_GLOBAL_HOTKEY: Record<string, string> = {
  "`": "Backquote", "\\": "Backslash",
  "[": "BracketLeft", "]": "BracketRight",
  ",": "Comma", "=": "Equal", "-": "Minus",
  ".": "Period", "'": "Quote", ";": "Semicolon", "/": "Slash",
};

/**
 * Validate and normalize a key combo for `global-hotkey` `Shortcut::from_str`.
 * Returns the normalized combo string if valid, or throws with a user-friendly
 * message listing the unsupported key.
 */
export function validateGlobalHotkeyCombo(combo: string): string {
  const parts = combo.split("+");
  const modifiers: string[] = [];
  let key = "";

  for (const p of parts) {
    if (["Cmd", "Ctrl", "Alt", "Shift"].includes(p)) {
      modifiers.push(p);
    } else {
      key = p;
    }
  }

  if (!key) throw new Error("No key specified (modifiers only)");

  // Letters and digits are always valid
  if (/^[A-Z]$/i.test(key) || /^[0-9]$/.test(key)) {
    return combo;
  }
  // Function keys F1-F24
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) {
    return combo;
  }
  // Numpad digits Numpad0-Numpad9
  if (/^Numpad[0-9]$/.test(key)) {
    return combo;
  }
  // Named keys
  if (GLOBAL_HOTKEY_KEYS.has(key)) {
    return combo;
  }
  // Punctuation character → named key
  const mapped = CHAR_TO_GLOBAL_HOTKEY[key];
  if (mapped) {
    return [...modifiers, mapped].join("+");
  }

  throw new Error(
    `The key "${key}" is not supported for global hotkeys. ` +
    `Supported: letters, digits, F1–F24, and common punctuation (, . / ; ' [ ] \\ \` - =).`
  );
}

export function keyEventToCombo(e: KeyboardEvent): string | null {
  // Ignore modifier-only presses
  if (MODIFIER_CODES.has(e.code)) return null;

  const mac = isMacOS();
  const modifiers: string[] = [];

  // On macOS: Meta = Cmd, Ctrl = Ctrl
  // On Win/Linux: Ctrl = Cmd (primary modifier), Meta = Super (ignored)
  if (mac) {
    if (e.metaKey) modifiers.push("Cmd");
    if (e.ctrlKey) modifiers.push("Ctrl");
  } else {
    if (e.ctrlKey) modifiers.push("Cmd");
  }
  if (e.altKey) modifiers.push("Alt");
  if (e.shiftKey) modifiers.push("Shift");

  // Determine the main key
  let key = KEY_MAP[e.key] ?? e.key;

  // Function keys: keep as-is (F1-F12)
  if (/^F\d{1,2}$/.test(key)) {
    return [...modifiers, key].join("+");
  }

  // Single character keys: uppercase
  if (key.length === 1) {
    key = key.toUpperCase();
  }

  return [...modifiers, key].join("+");
}
