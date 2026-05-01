// Terminal keyboard input → escape sequence mapping.
// Pure function: KeyboardEvent → string (to send to PTY) or null (don't handle).

const ARROW_SUFFIX: Record<string, string> = {
  ArrowUp: "A",
  ArrowDown: "B",
  ArrowRight: "C",
  ArrowLeft: "D",
};

const F_KEYS: Record<string, string> = {
  F1: "\x1bOP",
  F2: "\x1bOQ",
  F3: "\x1bOR",
  F4: "\x1bOS",
  F5: "\x1b[15~",
  F6: "\x1b[17~",
  F7: "\x1b[18~",
  F8: "\x1b[19~",
  F9: "\x1b[20~",
  F10: "\x1b[21~",
  F11: "\x1b[23~",
  F12: "\x1b[24~",
};

const NAV_KEYS: Record<string, string> = {
  Home: "\x1b[H",
  End: "\x1b[F",
  Insert: "\x1b[2~",
  Delete: "\x1b[3~",
  PageUp: "\x1b[5~",
  PageDown: "\x1b[6~",
};

const IGNORED_KEYS = new Set([
  "Shift", "Control", "Alt", "Meta", "CapsLock", "NumLock", "ScrollLock",
  "Hyper", "Super", "ContextMenu", "OS",
]);

function modifierParam(e: KeyboardEvent): number {
  return 1 + (e.shiftKey ? 1 : 0) + (e.altKey ? 2 : 0) + (e.ctrlKey ? 4 : 0);
}

/**
 * Convert a KeyboardEvent to the terminal escape sequence string to send to the PTY.
 * Returns null if the key should not be handled (modifier-only, Meta/Cmd).
 */
export function keyToSequence(e: KeyboardEvent): string | null {
  if (e.metaKey) return null;
  if (IGNORED_KEYS.has(e.key)) return null;

  // Arrow keys
  const arrowSuffix = ARROW_SUFFIX[e.key];
  if (arrowSuffix) {
    const mod = modifierParam(e);
    return mod > 1 ? `\x1b[1;${mod}${arrowSuffix}` : `\x1b[${arrowSuffix}`;
  }

  // Function keys
  const fKey = F_KEYS[e.key];
  if (fKey) return fKey;

  // Navigation keys
  const navKey = NAV_KEYS[e.key];
  if (navKey) return navKey;

  // Simple named keys
  switch (e.key) {
    case "Enter": return "\r";
    case "Tab": return "\t";
    case "Backspace": return "\x7f";
    case "Escape": return "\x1b";
  }

  // Ctrl+letter → control character (0x01-0x1a)
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    const lower = e.key.toLowerCase();
    const code = lower.charCodeAt(0);
    if (code >= 0x61 && code <= 0x7a) {
      return String.fromCharCode(code - 0x60);
    }
  }

  // Alt+letter → ESC + char
  if (e.altKey && !e.ctrlKey && e.key.length === 1) {
    return `\x1b${e.key}`;
  }

  // Printable single character
  if (e.key.length === 1) {
    return e.key;
  }

  return null;
}
