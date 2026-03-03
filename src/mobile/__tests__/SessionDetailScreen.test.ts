import { describe, it, expect } from "vitest";

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
