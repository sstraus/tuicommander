/**
 * Format a Unix millisecond timestamp as a short relative string.
 *
 * Options:
 *   showDateFallback — for timestamps older than 24h, return a locale date
 *   string instead of "Nd ago". Useful for note/item timestamps.
 */
export function formatRelativeTime(
  timestamp: number | null | undefined,
  options?: { showDateFallback?: boolean },
): string {
  if (!timestamp) return "never";
  const diff = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (diff < 5) return "just now";
  if (diff < 60) return `${diff}s ago`;
  const diffMin = Math.floor(diff / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  if (options?.showDateFallback) {
    return new Date(timestamp).toLocaleDateString();
  }
  const diffDay = Math.floor(diffHr / 24);
  return `${diffDay}d ago`;
}

/** Format a duration in milliseconds as a human-readable string (e.g. "2m 5s"). */
export function formatDuration(ms: number): string {
  const totalSec = Math.round(ms / 1000);
  if (totalSec < 60) return `${totalSec}s`;
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;
  return `${mins}m ${secs}s`;
}

/** Format an ISO timestamp as a relative time string (e.g., "3h ago", "2d ago") */
export function relativeTime(isoString: string): string {
  if (!isoString) return "";

  const now = Date.now();
  const then = new Date(isoString).getTime();
  const diffMs = now - then;
  const diffSec = Math.floor(diffMs / 1000);

  if (diffSec < 60) return "just now";

  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;

  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour}h ago`;

  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return `${diffDay}d ago`;

  if (diffDay < 30) {
    const weeks = Math.floor(diffDay / 7);
    return `${weeks}w ago`;
  }

  const months = Math.floor(diffDay / 30);
  return `${months}mo ago`;
}
