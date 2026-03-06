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

/** Extract the plain text content of a log line (concatenated spans). */
export function lineText(line: LogLine): string {
  return line.spans.map((s) => s.text).join("");
}

/** Check if a log line's text contains the query (case-insensitive). */
export function lineMatchesQuery(line: LogLine, query: string): boolean {
  if (!query) return true;
  return lineText(line).toLowerCase().includes(query.toLowerCase());
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
 */
export function normalizeLogLine(raw: unknown): LogLine {
  if (typeof raw === "string") {
    return { spans: [{ text: raw }] };
  }
  if (isLogLine(raw)) {
    return raw;
  }
  return { spans: [{ text: String(raw) }] };
}
