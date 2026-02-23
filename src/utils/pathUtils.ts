/**
 * Cross-platform path utilities for UI display.
 *
 * Handles both `/` (Unix) and `\` (Windows) separators so that
 * paths received from the Rust backend display correctly on all platforms.
 */

const SEP_RE = /[/\\]/;

/** Split a path into segments by either separator. */
export function pathParts(p: string): string[] {
  return p.split(SEP_RE).filter(Boolean);
}

/** Get the last segment of a path (filename or directory name). */
export function pathBasename(p: string): string {
  const parts = pathParts(p);
  return parts.length > 0 ? parts[parts.length - 1] : "";
}

/** Get the directory portion of a path, preserving the original separator. */
export function pathDirname(p: string): string {
  const lastSep = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return lastSep < 0 ? "" : p.slice(0, lastSep);
}

/** Replace the last segment (basename) of a path, preserving the original separator. */
export function replaceBasename(p: string, newName: string): string {
  const lastSep = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (lastSep < 0) return newName;
  return p.slice(0, lastSep + 1) + newName;
}
