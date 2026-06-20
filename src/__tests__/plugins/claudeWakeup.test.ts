/**
 * Tests for claude-wakeup plugin logic.
 *
 * The plugin is a standalone JS module loaded at runtime. We can't import it
 * directly, so we duplicate the pure logic (SessionTracker, canWake, shell-state
 * handler) and test the state machine in isolation.
 */
import { describe, expect, it } from "vitest";

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

/**
 * Simulates the "user typing" skip branch of checkWakeups(): a wake was
 * committed (pendingWake set, counters incremented), then the input-buffer
 * check found unsent text. Rolls back the attempt and records the unsent
 * text as user activity so the idle guard suppresses re-attempts.
 */
function applyTypingSkip(session: SessionTracker, now: number): void {
	session.pendingWake = false;
	session.pendingWakeAt = 0;
	session.wakeBusySeen = false;
	session.wakeCount--;
	session.totalWakesEver--;
	session.lastUserInputAt = now;
}

const WAKE_MESSAGE = "Continue, or reply `done` if finished.";
const WAKE_MESSAGE_CLEAN = WAKE_MESSAGE.replace(/`/g, "");
const DONE_RE = /^[\s\-*>⏺●◉⬤·•]*done[.!?"'`,:;]*\s*$/i;
const ANSI_RE = /\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]/g;

function isWakeMessage(text: string): boolean {
	const clean = text.replace(ANSI_RE, "");
	if (clean.includes(WAKE_MESSAGE) || clean.includes(WAKE_MESSAGE_CLEAN)) return true;
	return clean.includes("Continue") && clean.includes("finished");
}

function isDoneReply(line: string): boolean {
	if (isWakeMessage(line)) return false;
	return DONE_RE.test(line.replace(ANSI_RE, ""));
}

/**
 * Simulates the sleep/wake guard at the top of checkWakeups(): when the gap
 * since the last tick exceeds the threshold, reset every session's idle clock,
 * drop in-flight wakes, and skip the round. Returns true if a sleep gap was
 * detected (round skipped).
 */
function checkSleepWake(
	sessions: Map<string, SessionTracker>,
	lastCheckAt: number,
	now: number,
	config = DEFAULTS,
): boolean {
	const sleepGapMs = Math.max(30_000, config.checkIntervalMs * 4);
	if (lastCheckAt > 0 && now - lastCheckAt > sleepGapMs) {
		for (const session of sessions.values()) {
			session.lastIdleAt = now;
			session.lastBusyAt = now;
			session.pendingWake = false;
			session.pendingWakeAt = 0;
			session.wakeBusySeen = false;
		}
		return true;
	}
	return false;
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

describe("typing skip backs off (no 5s busy-retry on unsent input)", () => {
	/** Build a session that canWake() would fire on, then commit the wake. */
	function committedWake(now: number): SessionTracker {
		const s = new SessionTracker();
		s.shellState = "idle";
		s.lastIdleAt = now - DEFAULTS.idleThresholdMs - 1;
		expect(canWake(s, now)).toBe(true);
		// checkWakeups commits the attempt before the async buffer check:
		s.pendingWake = true;
		s.pendingWakeAt = now;
		s.wakeCount++;
		s.totalWakesEver++;
		return s;
	}

	it("rolls back the committed wake counters", () => {
		const now = 100_000;
		const s = committedWake(now);
		expect(s.wakeCount).toBe(1);

		applyTypingSkip(s, now);

		expect(s.pendingWake).toBe(false);
		expect(s.wakeCount).toBe(0);
		expect(s.totalWakesEver).toBe(0);
	});

	it("suppresses re-attempt on the next 5s check tick", () => {
		const now = 100_000;
		const s = committedWake(now);

		applyTypingSkip(s, now);

		// Without recording the unsent input as activity, canWake would return
		// true again on the very next tick → log + IPC every 5s forever.
		expect(canWake(s, now + DEFAULTS.checkIntervalMs)).toBe(false);
	});

	it("resumes waking once typing stops (after idle backoff elapses)", () => {
		const now = 100_000;
		const s = committedWake(now);
		applyTypingSkip(s, now);

		// Still suppressed within 2x idle threshold...
		expect(canWake(s, now + DEFAULTS.idleThresholdMs * 2 - 1)).toBe(false);
		// ...but eligible again once the user has been quiet long enough.
		s.lastIdleAt = now; // genuinely idle since the skip
		expect(canWake(s, now + DEFAULTS.idleThresholdMs * 2 + 1)).toBe(true);
	});
});

describe("sleep/wake gap detection (no nudge burst on resume)", () => {
	/** A session left idle (and wake-eligible) just before the machine slept. */
	function eligibleBeforeSleep(t: number): SessionTracker {
		const s = new SessionTracker();
		s.shellState = "idle";
		s.lastIdleAt = t - DEFAULTS.idleThresholdMs - 1;
		return s;
	}

	it("a session that was wake-eligible before sleep is NOT eligible right after resume", () => {
		const tBeforeSleep = 100_000;
		const s = eligibleBeforeSleep(tBeforeSleep);
		// Sanity: it WAS eligible the instant before sleep.
		expect(canWake(s, tBeforeSleep)).toBe(true);

		const sessions = new Map([["sess", s]]);
		const wakeNow = tBeforeSleep + 8 * 60 * 60 * 1000; // 8h sleep
		const skipped = checkSleepWake(sessions, tBeforeSleep, wakeNow);

		expect(skipped).toBe(true);
		// Idle clock reset to wake time → the 20s countdown restarts from resume.
		expect(s.lastIdleAt).toBe(wakeNow);
		expect(canWake(s, wakeNow)).toBe(false);
		// Still suppressed just under the threshold...
		expect(canWake(s, wakeNow + DEFAULTS.idleThresholdMs - 1)).toBe(false);
		// ...and only a GENUINE post-resume stall (idle 20s+) nudges.
		expect(canWake(s, wakeNow + DEFAULTS.idleThresholdMs + 1)).toBe(true);
	});

	it("drops an in-flight wake that straddled the sleep", () => {
		const s = new SessionTracker();
		s.shellState = "idle";
		s.pendingWake = true;
		s.pendingWakeAt = 100_000;
		s.wakeBusySeen = true;

		const sessions = new Map([["sess", s]]);
		checkSleepWake(sessions, 100_000, 100_000 + 60 * 60 * 1000);

		expect(s.pendingWake).toBe(false);
		expect(s.pendingWakeAt).toBe(0);
		expect(s.wakeBusySeen).toBe(false);
	});

	it("normal 5s ticks are never read as sleep", () => {
		const s = eligibleBeforeSleep(100_000);
		const sessions = new Map([["sess", s]]);
		const skipped = checkSleepWake(sessions, 100_000, 100_000 + DEFAULTS.checkIntervalMs);
		expect(skipped).toBe(false);
		// Untouched → still eligible (no false reset on a normal tick).
		expect(canWake(s, 100_000 + DEFAULTS.checkIntervalMs)).toBe(true);
	});

	it("the very first tick (lastCheckAt=0) never reads as sleep", () => {
		const sessions = new Map([["sess", new SessionTracker()]]);
		expect(checkSleepWake(sessions, 0, 999_999_999)).toBe(false);
	});
});

describe("done detection survives ANSI private-mode sequences", () => {
	it("matches a 'done' line wrapped in cursor show/hide escapes", () => {
		// Claude renders the reply with private-mode toggles like \x1b[?25h
		// (show cursor). The old [0-9;]*[A-Za-z] strip left '?25h' garbage and
		// isDoneReply failed → the session never disarmed → wake loop.
		const line = "\x1b[?25l● done.\x1b[?25h";
		expect(isDoneReply(line)).toBe(true);
	});

	it("still rejects the wake message echoed back (even with backticks stripped)", () => {
		// Terminal rendering may strip the backticks from `done`.
		expect(isWakeMessage("Continue, or reply done if finished.")).toBe(true);
		expect(isWakeMessage(WAKE_MESSAGE)).toBe(true);
		expect(isDoneReply("Continue, or reply done if finished.")).toBe(false);
	});

	it("does not match prose containing the word done", () => {
		expect(isDoneReply("I am done with the first part, continuing now")).toBe(false);
	});
});
