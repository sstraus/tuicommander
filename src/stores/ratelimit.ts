import { createStore, produce } from "solid-js/store";
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
      setState(produce((s) => { delete s.rateLimits[sessionId]; }));
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
      const expired = Object.entries(state.rateLimits)
        .filter(([, info]) => !isStillRateLimited(info))
        .map(([id]) => id);
      if (expired.length === 0) return;
      setState(produce((s) => {
        for (const id of expired) delete s.rateLimits[id];
      }));
    },

    /** Clear all rate limits */
    clearAll(): void {
      setState("rateLimits", {});
    },
  };

  return { state, ...actions };
}

export const rateLimitStore = createRateLimitStore();
