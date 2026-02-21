/**
 * Strips ANSI escape sequences from a string.
 *
 * Covers:
 * - SGR (Select Graphic Rendition): colors, bold, underline, etc.  ESC[ ... m
 * - Cursor movement: ESC[ ... A/B/C/D/E/F/G/H
 * - Erase sequences: ESC[ ... J/K
 * - Scroll sequences: ESC[ ... S/T
 * - DEC private mode: ESC[? ... l/h
 * - OSC sequences with BEL (ESC]...BEL) or ST (ESC]...ESC\) terminators
 *
 * Used before regex pattern matching on PTY output so that plugin watchers
 * can write simple patterns without accounting for color codes.
 */

// Matches CSI sequences per ECMA-48:
//   ESC [  param-bytes(0x30-0x3F)*  intermediate-bytes(0x20-0x2F)*  final-byte(0x40-0x7E)
// and OSC sequences: ESC ] ... (BEL | ESC \)
const ANSI_RE =
  /\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]|\x1b\][\s\S]*?(?:\x07|\x1b\\)/g;

export function stripAnsi(str: string): string {
  return str.replace(ANSI_RE, "");
}
