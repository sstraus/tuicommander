/**
 * Parse an OSC 7 URL (file://hostname/path) into a local filesystem path.
 * Returns null if the URL is invalid, non-file, or has an empty path.
 */
export function parseOsc7Url(data: string): string | null {
  let url: URL;
  try {
    url = new URL(data);
  } catch {
    return null;
  }

  if (url.protocol !== "file:") return null;

  const raw = url.pathname;
  if (!raw || raw === "") return null;

  // Decode percent-encoding (e.g. %20 → space, %23 → #)
  const decoded = decodeURIComponent(raw);

  // Strip trailing slash unless it's the root path
  const path =
    decoded.length > 1 && decoded.endsWith("/")
      ? decoded.slice(0, -1)
      : decoded;

  return path;
}
