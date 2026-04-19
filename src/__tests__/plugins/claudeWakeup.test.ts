/**
 * Tests for claude-wakeup plugin logic.
 *
 * The plugin is a standalone JS module loaded at runtime. We can't import it
 * directly, so we duplicate the pure logic (SessionTracker, canWake, shell-state
 * handler) and test the state machine in isolation.
 */
import { describe, it, expect } from "vitest";

// ── Duplicated from plugins/claude-wakeup/main.js ───────────────────

const DEFAULTS = {
  idleThresholdMs: 20_000,
  maxWakes: 3,
  maxWakesEver: 12,
  checkIntervalMs: 5_000,
  minBusyDurationMs: 1500,
  questionStaleMs: 30 * 60_000,
  pendingTimeoutMs: 60_000,
  doneMaxBusyMs: 8_000,
};

class SessionTracker {
  shellState: string | null = null;
  lastIdleAt = 0;
  lastBusyAt = 0;
  lastUserInputAt = 0;
  hasQuestionAt = 0;
  activeSubtasks = 0;
  choicePromptActive = false;
  disarmed = false;
  disarmedAt = 0;
  wakeCount = 0;
  totalWakesEver = 0;
  pendingWake = false;
  pendingWakeAt = 0;
  lastWakeSentAt = 0;
  wakeBusySeen = false;
}

function canWake(session: SessionTracker, now: number, config = DEFAULTS): boolean {
  if (session.disarmed) return false;
  if (session.shellState !== "idle") return false;
  if (session.pendingWake) return false;
  if (session.wakeCount >= config.maxWakes) return false;
  if (session.totalWakesEver >= config.maxWakesEver) return false;
  if (session.activeSubtasks > 0) return false;
  if (session.choicePromptActive) return false;
  if (session.hasQuestionAt > 0 && now - session.hasQuestionAt < config.questionStaleMs) {
    return false;
  }
  if (session.lastUserInputAt > 0 && now - session.lastUserInputAt < config.idleThresholdMs * 2) {
    return false;
  }
  if (session.lastIdleAt === 0) return false;
  if (now - session.lastIdleAt < config.idleThresholdMs) return false;
  return true;
}

/** Simulates the shell-state handler logic for busy→idle transitions. */
function handleShellState(
  session: SessionTracker,
  state: "busy" | "idle",
  now: number,
  config = DEFAULTS,
): { confirmed?: boolean; continued?: boolean } {
  const prev = session.shellState;
  session.shellState = state;

  if (state === "busy") {
    session.lastBusyAt = now;
    if (session.pendingWake && !session.wakeBusySeen) {
      session.wakeBusySeen = true;
    }
    return {};
  }

  // state === "idle"
  if (prev === "busy") {
    const busyDuration = session.lastBusyAt ? now - session.lastBusyAt : 0;

    // ALWAYS reset the idle clock
    session.lastIdleAt = now;

    // Done detection
    if (session.pendingWake && session.wakeBusySeen && busyDuration < config.doneMaxBusyMs) {
      session.pendingWake = false;
      session.pendingWakeAt = 0;
      session.wakeBusySeen = false;
      session.disarmed = true;
      session.disarmedAt = now;
      session.wakeCount = 0;
      return { confirmed: true };
    }

    // Long busy after wake
    if (session.pendingWake && session.wakeBusySeen && busyDuration >= config.doneMaxBusyMs) {
      session.pendingWake = false;
      session.pendingWakeAt = 0;
      session.wakeBusySeen = false;
      return { continued: true };
    }

    if (busyDuration < config.minBusyDurationMs) {
      return {};
    }

    // Re-arm
    if (
      session.disarmed &&
      busyDuration > 10_000 &&
      session.lastUserInputAt > session.disarmedAt &&
      now - session.lastUserInputAt < 5 * 60_000
    ) {
      session.disarmed = false;
      session.disarmedAt = 0;
      session.wakeCount = 0;
    }
  }
  if (session.lastIdleAt === 0) session.lastIdleAt = now;
  return {};
}

// ── Tests ───────────────────────────────────────────────────────────

describe("canWake", () => {
  it("returns true when idle for longer than threshold", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    expect(canWake(s, 1000 + DEFAULTS.idleThresholdMs + 1)).toBe(true);
  });

  it("returns false when idle for less than threshold", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    expect(canWake(s, 1000 + DEFAULTS.idleThresholdMs - 1)).toBe(false);
  });

  it("returns false when shell is busy", () => {
    const s = new SessionTracker();
    s.shellState = "busy";
    s.lastIdleAt = 1000;
    expect(canWake(s, 100_000)).toBe(false);
  });

  it("returns false when disarmed", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.disarmed = true;
    expect(canWake(s, 100_000)).toBe(false);
  });

  it("returns false when pendingWake is true", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.pendingWake = true;
    expect(canWake(s, 100_000)).toBe(false);
  });

  it("returns false when maxWakes reached", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.wakeCount = DEFAULTS.maxWakes;
    expect(canWake(s, 100_000)).toBe(false);
  });

  it("returns false when user input was recent", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    const now = 50_000;
    s.lastUserInputAt = now - DEFAULTS.idleThresholdMs; // within 2x threshold
    expect(canWake(s, now)).toBe(false);
  });

  it("returns false when question is pending", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.hasQuestionAt = 40_000;
    expect(canWake(s, 50_000)).toBe(false);
  });

  it("returns false when subtasks are active", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.activeSubtasks = 1;
    expect(canWake(s, 100_000)).toBe(false);
  });

  it("returns false when choice prompt is active", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.lastIdleAt = 1000;
    s.choicePromptActive = true;
    expect(canWake(s, 100_000)).toBe(false);
  });
});

describe("typing resets lastIdleAt (prevents wake while user types)", () => {
  it("short busy blips from typing push lastIdleAt forward", () => {
    const s = new SessionTracker();
    // Agent went idle 30s ago
    s.shellState = "idle";
    s.lastIdleAt = 0;

    const t0 = 10_000;
    handleShellState(s, "idle", t0); // initial idle
    expect(s.lastIdleAt).toBe(t0);

    // 25s later: user starts typing (generates short busy/idle oscillations)
    const t1 = t0 + 25_000;
    handleShellState(s, "busy", t1);
    // 100ms later: keystroke echo done
    handleShellState(s, "idle", t1 + 100);
    // lastIdleAt should be updated to now, not stuck at t0
    expect(s.lastIdleAt).toBe(t1 + 100);

    // canWake should be false because lastIdleAt was just updated
    expect(canWake(s, t1 + 100)).toBe(false);
    // Even 10s later, still < 20s threshold
    expect(canWake(s, t1 + 100 + 10_000)).toBe(false);
  });

  it("continuous typing keeps pushing lastIdleAt forward indefinitely", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    const t0 = 10_000;
    handleShellState(s, "idle", t0);

    // Simulate 60s of typing (one keystroke every 500ms)
    for (let i = 0; i < 120; i++) {
      const t = t0 + 25_000 + i * 500;
      handleShellState(s, "busy", t);
      handleShellState(s, "idle", t + 50);
    }

    const lastIdle = s.lastIdleAt;
    // canWake should be false — lastIdleAt was just updated
    expect(canWake(s, lastIdle)).toBe(false);
    expect(canWake(s, lastIdle + 10_000)).toBe(false);
  });

  it("wake fires only after 20s of TRUE idle (no typing)", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    const t0 = 10_000;
    handleShellState(s, "idle", t0);

    // User types at t0+5s
    handleShellState(s, "busy", t0 + 5_000);
    handleShellState(s, "idle", t0 + 5_100);

    // 19s after last typing — still too soon
    expect(canWake(s, t0 + 5_100 + 19_000)).toBe(false);
    // 21s after last typing — now it can fire
    expect(canWake(s, t0 + 5_100 + 21_000)).toBe(true);
  });
});

describe("done detection via busy-cycle duration", () => {
  it("short busy cycle after wake → confirmed done", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.pendingWake = true;
    s.pendingWakeAt = 1000;

    // Agent goes busy (processing our wake)
    handleShellState(s, "busy", 2000);
    expect(s.wakeBusySeen).toBe(true);

    // Agent replies quickly (3s) → done
    const result = handleShellState(s, "idle", 5000);
    expect(result.confirmed).toBe(true);
    expect(s.pendingWake).toBe(false);
    expect(s.disarmed).toBe(true);
  });

  it("long busy cycle after wake → agent continued working", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.pendingWake = true;
    s.pendingWakeAt = 1000;

    handleShellState(s, "busy", 2000);

    // Agent works for 15s → continued
    const result = handleShellState(s, "idle", 17_000);
    expect(result.continued).toBe(true);
    expect(s.pendingWake).toBe(false);
    expect(s.disarmed).toBe(false);
  });

  it("very short busy cycle (<1.5s) after wake still detects done", () => {
    const s = new SessionTracker();
    s.shellState = "idle";
    s.pendingWake = true;
    s.pendingWakeAt = 1000;

    handleShellState(s, "busy", 2000);

    // Agent replies in 500ms — under minBusyDurationMs but should still confirm
    const result = handleShellState(s, "idle", 2500);
    expect(result.confirmed).toBe(true);
    expect(s.disarmed).toBe(true);
  });
});

describe("re-arm after disarm", () => {
  it("re-arms when user gives NEW input after disarm and agent works >10s", () => {
    const s = new SessionTracker();
    s.disarmed = true;
    s.disarmedAt = 10_000;
    s.shellState = "idle";

    // User types new input AFTER disarm
    s.lastUserInputAt = 15_000;

    // Agent works for 12s
    handleShellState(s, "busy", 20_000);
    handleShellState(s, "idle", 32_000);

    expect(s.disarmed).toBe(false);
  });

  it("does NOT re-arm when user input was BEFORE disarm", () => {
    const s = new SessionTracker();
    s.disarmed = true;
    s.disarmedAt = 10_000;
    s.shellState = "idle";

    // User input was BEFORE disarm
    s.lastUserInputAt = 5_000;

    handleShellState(s, "busy", 20_000);
    handleShellState(s, "idle", 32_000);

    expect(s.disarmed).toBe(true);
  });

  it("does NOT re-arm for short busy cycles", () => {
    const s = new SessionTracker();
    s.disarmed = true;
    s.disarmedAt = 10_000;
    s.lastUserInputAt = 15_000;
    s.shellState = "idle";

    // Agent works for 5s (under 10s threshold)
    handleShellState(s, "busy", 20_000);
    handleShellState(s, "idle", 25_000);

    expect(s.disarmed).toBe(true);
  });
});
