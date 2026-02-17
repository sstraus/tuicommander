import { createStore, reconcile } from "solid-js/store";
import type { RateLimitInfo } from "../rate-limit";
import { isStillRateLimited, getRemainingWaitTime } from "../rate-limit";

/** Rate limit store state */
interface RateLimitStoreState {
  /** Active rate limits by session ID */
  rateLimits: Record<string, RateLimitInfo>;
}

/** Create the rate limit store */
function createRateLimitStore() {
  const [state, setState] = createStore<RateLimitStoreState>({
    rateLimits: {},
  });

  const actions = {
    /** Record a new rate limit */
    addRateLimit(info: RateLimitInfo): void {
      setState("rateLimits", info.sessionId, info);
    },

    /** Remove a rate limit (session recovered) */
    removeRateLimit(sessionId: string): void {
      const { [sessionId]: _removed, ...rest } = state.rateLimits;
      setState("rateLimits", reconcile(rest));
    },

    /** Check if a session is rate-limited */
    isRateLimited(sessionId: string): boolean {
      const info = state.rateLimits[sessionId];
      if (!info) return false;
      return isStillRateLimited(info);
    },

    /** Get rate limit info for a session */
    getRateLimitInfo(sessionId: string): RateLimitInfo | undefined {
      return state.rateLimits[sessionId];
    },

    /** Get remaining wait time for a session */
    getWaitTime(sessionId: string): number {
      const info = state.rateLimits[sessionId];
      if (!info) return 0;
      return getRemainingWaitTime(info);
    },

    /** Get all rate-limited session IDs */
    getRateLimitedSessions(): string[] {
      return Object.entries(state.rateLimits)
        .filter(([_, info]) => isStillRateLimited(info))
        .map(([sessionId]) => sessionId);
    },

    /** Get count of rate-limited sessions */
    getRateLimitedCount(): number {
      return Object.values(state.rateLimits).filter(isStillRateLimited).length;
    },

    /** Clean up expired rate limits */
    cleanupExpired(): void {
      const active: Record<string, RateLimitInfo> = {};

      for (const [sessionId, info] of Object.entries(state.rateLimits)) {
        if (isStillRateLimited(info)) {
          active[sessionId] = info;
        }
      }

      setState("rateLimits", reconcile(active));
    },

    /** Clear all rate limits */
    clearAll(): void {
      setState("rateLimits", reconcile({}));
    },
  };

  return { state, ...actions };
}

export const rateLimitStore = createRateLimitStore();
