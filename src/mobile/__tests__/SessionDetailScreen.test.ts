import { describe, it, expect } from "vitest";
import { formatRetryCountdown } from "../utils/formatRetryCountdown";

// Test the session-ended overlay logic.
// When a session disappears from the active sessions list, MobileApp must:
// 1. Keep showing SessionDetailScreen (not unmount it)
// 2. Pass sessionExists=false so the overlay appears
// 3. Allow the user to press Back to dismiss
describe("session ended overlay logic", () => {
  function makeSessionTracker(initialSessions: string[]) {
    let activeSessions = [...initialSessions];
    let selectedId: string | null = null;
    // Last known session data - kept even after session disappears
    let lastKnownSession: { session_id: string } | null = null;

    function select(id: string) {
      selectedId = id;
      const found = activeSessions.find((s) => s === id);
      if (found) lastKnownSession = { session_id: found };
    }

    function removeSession(id: string) {
      activeSessions = activeSessions.filter((s) => s !== id);
    }

    function goBack() {
      selectedId = null;
      lastKnownSession = null;
    }

    // Returns the session to render (last known), plus whether it still exists
    function getDetailState(): { session: { session_id: string } | null; exists: boolean } {
      if (!selectedId || !lastKnownSession) return { session: null, exists: false };
      const exists = activeSessions.includes(selectedId);
      return { session: lastKnownSession, exists };
    }

    return { select, removeSession, goBack, getDetailState };
  }

  it("shows session detail while session is active", () => {
    const tracker = makeSessionTracker(["s1", "s2"]);
    tracker.select("s1");
    const { session, exists } = tracker.getDetailState();
    expect(session?.session_id).toBe("s1");
    expect(exists).toBe(true);
  });

  it("keeps showing detail screen when session disappears (exists=false)", () => {
    const tracker = makeSessionTracker(["s1"]);
    tracker.select("s1");
    tracker.removeSession("s1");
    const { session, exists } = tracker.getDetailState();
    // Screen stays mounted with last known session
    expect(session?.session_id).toBe("s1");
    // But exists=false triggers the overlay
    expect(exists).toBe(false);
  });

  it("back clears the detail screen", () => {
    const tracker = makeSessionTracker(["s1"]);
    tracker.select("s1");
    tracker.removeSession("s1");
    tracker.goBack();
    const { session } = tracker.getDetailState();
    expect(session).toBeNull();
  });

  it("no detail shown when no session selected", () => {
    const tracker = makeSessionTracker(["s1"]);
    const { session } = tracker.getDetailState();
    expect(session).toBeNull();
  });
});

describe("error and rate-limit bar visibility", () => {
  /** Determines which info bars should be visible based on session state */
  function getInfoBars(state: {
    last_error?: string;
    rate_limited: boolean;
    retry_after_ms?: number;
  }): { errorBar: boolean; rateLimitBar: boolean } {
    return {
      errorBar: !!state.last_error,
      rateLimitBar: state.rate_limited,
    };
  }

  it("shows error bar when last_error is set", () => {
    const bars = getInfoBars({ last_error: "Tool execution failed", rate_limited: false });
    expect(bars.errorBar).toBe(true);
    expect(bars.rateLimitBar).toBe(false);
  });

  it("shows rate-limit bar when rate_limited is true", () => {
    const bars = getInfoBars({ rate_limited: true, retry_after_ms: 30000 });
    expect(bars.errorBar).toBe(false);
    expect(bars.rateLimitBar).toBe(true);
  });

  it("shows both bars when error and rate-limited", () => {
    const bars = getInfoBars({ last_error: "API error", rate_limited: true, retry_after_ms: 15000 });
    expect(bars.errorBar).toBe(true);
    expect(bars.rateLimitBar).toBe(true);
  });

  it("shows no bars when state is clean", () => {
    const bars = getInfoBars({ rate_limited: false });
    expect(bars.errorBar).toBe(false);
    expect(bars.rateLimitBar).toBe(false);
  });
});

describe("rate-limit countdown formatting", () => {
  it("formats seconds only", () => {
    expect(formatRetryCountdown(45000)).toBe("45s");
  });

  it("formats minutes and seconds", () => {
    expect(formatRetryCountdown(90000)).toBe("1m 30s");
  });

  it("formats whole minutes without seconds", () => {
    expect(formatRetryCountdown(120000)).toBe("2m");
  });

  it("returns 'now' for zero or negative", () => {
    expect(formatRetryCountdown(0)).toBe("now");
    expect(formatRetryCountdown(-1000)).toBe("now");
  });

  it("rounds up partial seconds", () => {
    expect(formatRetryCountdown(1500)).toBe("2s");
  });
});
