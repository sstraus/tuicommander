import { appLogger } from "../stores/appLogger";
import { getLastCrumb } from "./perfTrace";

const THRESHOLD_MS = 200;
const MAX_ENTRIES = 100;
const LOG_COOLDOWN_MS = 2000;
/** Gaps above this are system sleep / tab suspension, not a real freeze — the
 *  JS thread cannot block for this long. Counting them inflates the total and
 *  floods the logs with multi-minute "freezes" on every lid reopen. Skip and
 *  re-baseline. */
const SLEEP_GAP_MS = 10_000;
/** Timer cadence for the main-thread detector. setTimeout drift is independent
 *  of vsync/compositor, so a gap here means the MAIN THREAD was actually blocked
 *  (JS, GC, or native main-thread work) — unlike rAF, which also stalls on
 *  compositor backpressure even when the main thread is idle. */
const TIMER_INTERVAL_MS = 50;

interface FreezeEntry {
	at: number;
	gapMs: number;
	/** "main" = real main-thread block (timer drift). "paint" = rAF cadence
	 *  stall with the main thread responsive (compositor/GPU backpressure). */
	kind: "main" | "paint";
}

let running = false;
let lastTimerTick = 0;
let lastRafTick = 0;
let lastMainLogAt = 0;
let lastPaintLogAt = 0;
const freezes: FreezeEntry[] = [];

function record(kind: "main" | "paint", gapMs: number, at: number) {
	if (freezes.length < MAX_ENTRIES) freezes.push({ at, gapMs, kind });
}

/** Main-thread freeze detector: setTimeout-driven, so a gap is a genuine
 *  main-thread stall (the historic rAF detector conflated this with paint jank,
 *  which sent every prior freeze investigation down the wrong path). */
function timerTick() {
	if (!running) return;
	const now = performance.now();
	const gap = now - lastTimerTick;
	lastTimerTick = now;
	if (gap > SLEEP_GAP_MS) return; // sleep/suspend — re-baseline silently
	// When the window is hidden/occluded the OS clamps setInterval to ~1Hz and
	// pauses rAF — every clamped tick looks like a ~1000ms "freeze". Skip: a
	// throttled background timer says nothing about main-thread responsiveness.
	if (document.hidden) return;
	if (gap > THRESHOLD_MS) {
		record("main", Math.round(gap), now);
		if (now - lastMainLogAt > LOG_COOLDOWN_MS) {
			lastMainLogAt = now;
			const crumb = getLastCrumb();
			appLogger.warn("app", `UI freeze: ${Math.round(gap)}ms main-thread block`, crumb ?? undefined);
		}
	}
}

/** Paint-cadence detector: rAF-driven. A gap here with NO matching main-thread
 *  gap means the compositor fell behind (e.g. heavy terminal frame load) while
 *  the main thread stayed responsive — visual stutter, not an input freeze.
 *  Logged at debug so it no longer masquerades as a "UI freeze". */
function rafTick() {
	if (!running) return;
	const now = performance.now();
	const gap = now - lastRafTick;
	lastRafTick = now;
	if (gap > SLEEP_GAP_MS) {
		requestAnimationFrame(rafTick);
		return;
	}
	if (gap > THRESHOLD_MS) {
		record("paint", Math.round(gap), now);
		if (now - lastPaintLogAt > LOG_COOLDOWN_MS) {
			lastPaintLogAt = now;
			appLogger.debug("app", `Paint jank: ${Math.round(gap)}ms rAF gap (main thread responsive)`);
		}
	}
	requestAnimationFrame(rafTick);
}

export function startFreezeDetector() {
	if (running) return;
	running = true;
	const now = performance.now();
	lastTimerTick = now;
	lastRafTick = now;
	setInterval(timerTick, TIMER_INTERVAL_MS);
	requestAnimationFrame(rafTick);
	appLogger.debug("app", "Freeze detector started (timer 200ms main-thread + rAF paint-jank)");
}

export function getFreezes(): FreezeEntry[] {
	return freezes;
}

export function clearFreezes() {
	freezes.length = 0;
}
