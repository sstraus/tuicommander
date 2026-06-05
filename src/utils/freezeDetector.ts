import { appLogger } from "../stores/appLogger";
import { getLastCrumb } from "./perfTrace";

const THRESHOLD_MS = 200;
const MAX_ENTRIES = 100;
const LOG_COOLDOWN_MS = 2000;
/** RAF gaps above this are system sleep / tab suspension, not a main-thread
 *  freeze — the JS thread cannot block for this long. Counting them inflates
 *  the freeze total and floods the logs with multi-minute "freezes" on every
 *  lid reopen. Skip them and just re-baseline. */
const SLEEP_GAP_MS = 10_000;

interface FreezeEntry {
	at: number;
	gapMs: number;
}

let running = false;
let lastTick = 0;
let lastLogAt = 0;
const freezes: FreezeEntry[] = [];

function tick() {
	if (!running) return;
	const now = performance.now();
	const gap = now - lastTick;
	if (gap > SLEEP_GAP_MS) {
		// System slept / tab suspended — re-baseline without recording a freeze.
		lastTick = now;
		requestAnimationFrame(tick);
		return;
	}
	if (gap > THRESHOLD_MS) {
		if (freezes.length < MAX_ENTRIES) {
			freezes.push({ at: now, gapMs: Math.round(gap) });
		}
		if (now - lastLogAt > LOG_COOLDOWN_MS) {
			lastLogAt = now;
			const crumb = getLastCrumb();
			appLogger.warn(
				"app",
				`UI freeze: ${Math.round(gap)}ms gap (${freezes.length} total)`,
				crumb ?? undefined,
			);
		}
	}
	lastTick = now;
	requestAnimationFrame(tick);
}

export function startFreezeDetector() {
	if (running) return;
	running = true;
	lastTick = performance.now();
	requestAnimationFrame(tick);
	appLogger.debug("app", "Freeze detector started (threshold: 200ms)");
}

export function getFreezes(): FreezeEntry[] {
	return freezes;
}

export function clearFreezes() {
	freezes.length = 0;
}
