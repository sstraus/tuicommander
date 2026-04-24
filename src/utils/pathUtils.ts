/**
 * Cross-platform path utilities.
 *
 * Handles both `/` (Unix) and `\` (Windows) separators so that
 * paths received from the Rust backend work correctly on all platforms.
 *
 * POLICY: All path comparison and construction in the frontend MUST use these
 * helpers instead of raw string operations. Forbidden patterns:
 *   - `startsWith("/")` to detect absolute paths → use `isAbsolutePath()`
 *   - `path + "/"` or template literals for path joins → use `joinPath()`
 *   - `.split("/").pop()` for basenames → use `pathBasename()`
 *   - `path.startsWith(prefix + "/")` for containment → use `pathStartsWith()`
 */

const SEP_RE = /[/\\]/;

/** Normalize all backslashes to forward slashes for comparison. */
export function normalizeSep(p: string): string {
  return p.replace(/\\/g, "/");
}

/** True when `p` is an absolute path on any OS. */
export function isAbsolutePath(p: string): boolean {
  if (p.startsWith("/")) return true;
  if (/^[A-Za-z]:[\\/]/.test(p)) return true;
  if (p.startsWith("\\\\")) return true;
  return false;
}

/** True when `path` starts with `prefix` at a directory boundary, separator-agnostic. */
export function pathStartsWith(path: string, prefix: string): boolean {
  const np = normalizeSep(path);
  const npfx = normalizeSep(prefix).replace(/\/+$/, "");
  if (!npfx) return true;
  return np === npfx || np.startsWith(npfx + "/");
}

/** Strip `prefix` from `path` at a directory boundary. Returns the relative portion, or `null` if `path` does not start with `prefix`. Separators are normalized to `/` in the result. */
export function pathStripPrefix(path: string, prefix: string): string | null {
  const np = normalizeSep(path);
  const npfx = normalizeSep(prefix).replace(/\/+$/, "");
  if (np === npfx) return "";
  if (np.startsWith(npfx + "/")) return np.slice(npfx.length + 1);
  return null;
}

/** Join path segments, stripping trailing/leading separators between parts. */
export function joinPath(base: string, ...parts: string[]): string {
  let result = base.replace(/[\\/]+$/, "");
  for (const part of parts) {
    if (!part) continue;
    result += "/" + part.replace(/^[\\/]+/, "");
  }
  return result;
}

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
