/**
 * Kitty keyboard protocol (flag 1: disambiguate) — key encoding for the frontend.
 *
 * When a PTY application requests enhanced keyboard mode via CSI > 1 u,
 * we intercept specific keys and encode them as CSI keycode ; modifier u
 * instead of letting xterm.js send legacy byte sequences.
 *
 * Scope: Enter, Tab, Backspace (only when modifiers held), Escape (always).
 * Regular keys and function keys are left to xterm.js default handling.
 */

/** Unicode codepoints for keys handled by kitty protocol */
const KEY_ENTER = 13;
const KEY_TAB = 9;
const KEY_BACKSPACE = 127;
const KEY_ESCAPE = 27;

/**
 * Encode a keyboard event as a kitty CSI u sequence if applicable.
 *
 * Returns the CSI sequence string to send to the PTY, or null if the key
 * should be handled by xterm.js legacy encoding.
 */
export function kittySequenceForKey(
  key: string,
  shiftKey: boolean,
  altKey: boolean,
  ctrlKey: boolean,
  metaKey: boolean,
): string | null {
  // Meta (Cmd on macOS) is never intercepted — pass through to OS
  if (metaKey) return null;

  // Map key name to codepoint
  let codepoint: number;
  switch (key) {
    case "Enter":     codepoint = KEY_ENTER; break;
    case "Tab":       codepoint = KEY_TAB; break;
    case "Backspace": codepoint = KEY_BACKSPACE; break;
    case "Escape":    codepoint = KEY_ESCAPE; break;
    default: return null; // Not a key we handle
  }

  // Per kitty spec: Enter/Tab/Backspace without modifiers use legacy encoding
  if (codepoint !== KEY_ESCAPE && !shiftKey && !altKey && !ctrlKey) {
    return null;
  }

  // Modifier encoding per kitty spec: 1 + (shift?1:0) + (alt?2:0) + (ctrl?4:0)
  const mod = 1 + (shiftKey ? 1 : 0) + (altKey ? 2 : 0) + (ctrlKey ? 4 : 0);

  // CSI keycode ; modifier u  (omit modifier parameter when mod === 1, i.e. no modifiers)
  if (mod === 1) {
    return `\x1b[${codepoint}u`;
  }
  return `\x1b[${codepoint};${mod}u`;
}
