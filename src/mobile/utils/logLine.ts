/**
 * Types and utilities for rendering ANSI-attributed terminal log lines.
 * Matches the Rust LogLine/LogSpan/LogColor serialization format.
 */

// --- Types ---

export interface LogColor {
  idx?: number;
  rgb?: [number, number, number];
}

export interface LogSpan {
  text: string;
  fg?: LogColor;
  bg?: LogColor;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
}

export interface LogLine {
  spans: LogSpan[];
}

// --- ANSI color mapping ---

/**
 * Standard ANSI 16-color palette mapped to CSS variables.
 * These variables should be defined in the app's theme and match the terminal palette.
 */
const ANSI_16_VARS: readonly string[] = [
  "var(--ansi-black)",
  "var(--ansi-red)",
  "var(--ansi-green)",
  "var(--ansi-yellow)",
  "var(--ansi-blue)",
  "var(--ansi-magenta)",
  "var(--ansi-cyan)",
  "var(--ansi-white)",
  "var(--ansi-bright-black)",
  "var(--ansi-bright-red)",
  "var(--ansi-bright-green)",
  "var(--ansi-bright-yellow)",
  "var(--ansi-bright-blue)",
  "var(--ansi-bright-magenta)",
  "var(--ansi-bright-cyan)",
  "var(--ansi-bright-white)",
];

/** Convert ANSI 256-color index (16-231) from the 6x6x6 color cube to hex. */
function ansi256CubeToHex(idx: number): string {
  const i = idx - 16;
  const r = Math.floor(i / 36);
  const g = Math.floor((i % 36) / 6);
  const b = i % 6;
  const toVal = (c: number) => (c === 0 ? 0 : 55 + c * 40);
  return `#${toVal(r).toString(16).padStart(2, "0")}${toVal(g).toString(16).padStart(2, "0")}${toVal(b).toString(16).padStart(2, "0")}`;
}

/** Convert ANSI 256-color grayscale index (232-255) to hex. */
function ansi256GrayToHex(idx: number): string {
  const level = 8 + (idx - 232) * 10;
  const hex = level.toString(16).padStart(2, "0");
  return `#${hex}${hex}${hex}`;
}

/** Convert a LogColor to a CSS color string, or undefined for default. */
export function logColorToCss(color: LogColor | undefined): string | undefined {
  if (!color) return undefined;
  if (color.rgb) {
    const [r, g, b] = color.rgb;
    return `rgb(${r},${g},${b})`;
  }
  if (color.idx !== undefined) {
    const idx = color.idx;
    if (idx < 16) return ANSI_16_VARS[idx];
    if (idx < 232) return ansi256CubeToHex(idx);
    return ansi256GrayToHex(idx);
  }
  return undefined;
}

/** Build a CSS style object for a LogSpan. Returns undefined if no styling needed. */
export function spanStyle(
  span: LogSpan,
): Record<string, string> | undefined {
  const s: Record<string, string> = {};
  let hasStyle = false;

  const fg = logColorToCss(span.fg);
  if (fg) { s.color = fg; hasStyle = true; }

  const bg = logColorToCss(span.bg);
  if (bg) { s["background-color"] = bg; hasStyle = true; }

  if (span.bold) { s["font-weight"] = "600"; hasStyle = true; }
  if (span.italic) { s["font-style"] = "italic"; hasStyle = true; }
  if (span.underline) { s["text-decoration"] = "underline"; hasStyle = true; }

  return hasStyle ? s : undefined;
}

/** Whether a line contains Unicode box-drawing characters (U+2500–U+257F). */
const BOX_DRAWING_RE = /[\u2500-\u257F]/;

export function hasBoxDrawing(line: LogLine): boolean {
  return line.spans.some((span) => BOX_DRAWING_RE.test(span.text));
}

/** A block of lines: either a single text line or consecutive box-drawing lines. */
export type LineBlock =
  | { type: "text"; line: LogLine }
  | { type: "table"; lines: LogLine[] };

/** Group lines into blocks: consecutive box-drawing lines become one table block. */
export function groupLineBlocks(lines: LogLine[]): LineBlock[] {
  const blocks: LineBlock[] = [];
  let tableGroup: LogLine[] = [];
  for (const line of lines) {
    if (hasBoxDrawing(line)) {
      tableGroup.push(line);
    } else {
      if (tableGroup.length > 0) {
        blocks.push({ type: "table", lines: tableGroup });
        tableGroup = [];
      }
      blocks.push({ type: "text", line });
    }
  }
  if (tableGroup.length > 0) {
    blocks.push({ type: "table", lines: tableGroup });
  }
  return blocks;
}

/** Extract the plain text content of a log line (concatenated spans). */
export function lineText(line: LogLine): string {
  return line.spans.map((s) => s.text).join("");
}

/** Check if a log line's text contains the query (case-insensitive). */
export function lineMatchesQuery(line: LogLine, query: string): boolean {
  if (!query) return true;
  return lineText(line).toLowerCase().includes(query.toLowerCase());
}

/**
 * Characters that mobile browsers render as color emoji instead of monochrome
 * text glyphs.  Appending U+FE0E (VS15 — text variation selector) after each
 * forces the text presentation so they match the desktop xterm.js look.
 *
 * ● U+25CF  BLACK CIRCLE         — Claude Code / Copilot CLI status bullet
 * ○ U+25CB  WHITE CIRCLE         — Copilot CLI queued indicator
 * ⏺ U+23FA BLACK CIRCLE FOR REC — Claude Code Ink bullet variant
 * ⏵ U+23F5  PLAY BUTTON          — Claude Code subtask indicator
 * • U+2022  BULLET               — Codex CLI spinner
 * ◦ U+25E6  WHITE BULLET         — Codex CLI alternating spinner
 * ∴ U+2234  THEREFORE            — Copilot CLI thinking indicator
 * ✢ U+2722  FOUR TEARDROP STAR   — Claude Code v2.1.63+ status
 * ⚙ U+2699  GEAR                 — tool/settings indicator
 * ✻ U+273B  TEARDROP ASTERISK    — thinking indicator
 * ◉ U+25C9  FISHEYE              — occasional indicator
 */
const EMOJI_PRESENTATION_RE = /[●○⏺⏵•◦∴✢⚙✻◉]/g;

/** Append VS15 (U+FE0E) to characters that should render as text, not emoji. */
function forceTextPresentation(text: string): string {
  return text.replace(EMOJI_PRESENTATION_RE, (ch) => ch + "\uFE0E");
}

/** Type guard: checks that `value` is a LogLine (object with a `spans` array). */
export function isLogLine(value: unknown): value is LogLine {
  return (
    value !== null &&
    typeof value === "object" &&
    "spans" in value &&
    Array.isArray((value as LogLine).spans)
  );
}

/**
 * Normalize a raw log line value (from HTTP or WebSocket) to a LogLine.
 * Handles both structured LogLine objects and plain string fallback.
 * Also forces text presentation for characters that mobile browsers
 * would otherwise render as color emoji.
 */
export function normalizeLogLine(raw: unknown): LogLine {
  if (typeof raw === "string") {
    return { spans: [{ text: forceTextPresentation(raw) }] };
  }
  if (isLogLine(raw)) {
    // Apply VS15 fixup to every span's text
    for (const span of raw.spans) {
      span.text = forceTextPresentation(span.text);
    }
    return raw;
  }
  return { spans: [{ text: forceTextPresentation(String(raw)) }] };
}
