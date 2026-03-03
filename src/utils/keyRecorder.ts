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
