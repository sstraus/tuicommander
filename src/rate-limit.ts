import type { AgentType } from "./agents";

/** Rate limit detection result */
export interface RateLimitInfo {
  agentType: AgentType;
  sessionId: string;
  retryAfterMs: number | null;
  message: string;
  detectedAt: number;
}

/**
 * Check if a session is still rate-limited
 * @param info The rate limit info
 * @returns true if still rate-limited
 */
export function isStillRateLimited(info: RateLimitInfo): boolean {
  if (info.retryAfterMs === null) {
    return false; // Unknown, assume not limited
  }
  const elapsed = Date.now() - info.detectedAt;
  return elapsed < info.retryAfterMs;
}

/**
 * Get remaining wait time for a rate-limited session
 * @param info The rate limit info
 * @returns Remaining milliseconds, or 0 if not limited
 */
export function getRemainingWaitTime(info: RateLimitInfo): number {
  if (info.retryAfterMs === null) {
    return 0;
  }
  const elapsed = Date.now() - info.detectedAt;
  return Math.max(0, info.retryAfterMs - elapsed);
}

/**
 * Format remaining wait time as human-readable string
 */
export function formatWaitTime(ms: number): string {
  if (ms <= 0) return "now";
  if (ms < 1000) return "< 1s";

  const seconds = Math.ceil(ms / 1000);
  if (seconds < 60) return `${seconds}s`;

  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) {
    return remainingSeconds > 0 ? `${minutes}m ${remainingSeconds}s` : `${minutes}m`;
  }

  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes > 0 ? `${hours}h ${remainingMinutes}m` : `${hours}h`;
}
