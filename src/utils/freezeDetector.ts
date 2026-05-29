import { appLogger } from "../stores/appLogger";

const THRESHOLD_MS = 200;
const MAX_ENTRIES = 100;
const LOG_COOLDOWN_MS = 2000;

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
	if (gap > THRESHOLD_MS) {
		if (freezes.length < MAX_ENTRIES) {
			freezes.push({ at: now, gapMs: Math.round(gap) });
		}
		if (now - lastLogAt > LOG_COOLDOWN_MS) {
			lastLogAt = now;
			appLogger.warn("app", `UI freeze: ${Math.round(gap)}ms gap (${freezes.length} total)`);
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
