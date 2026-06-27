import { appLogger } from "../stores/appLogger";
import { isPerfDebug } from "./perfDebug";

// Poor-man's profiler for the repo-switch freeze investigation.
//
// PerformanceObserver({type:'longtask'|'long-animation-frame'}) is SILENTLY DEAD
// in our WKWebView build (verified 2026-06-05: a deliberate 420ms main-thread
// block produced a freezeDetector gap but ZERO observer entries). So we attribute
// freezes the old way: suspect regions drop a breadcrumb on entry, and the freeze
// detector reports the freshest breadcrumb when it catches a gap. A FRESH crumb
// (small ageMs) names the culprit; a STALE crumb means the block is unmarked.
//
// Off-main-thread rendering is active, so canvas PAINT is offloaded — these marks
// cover the main-thread work that remains (reactivity, git refresh, frame ack).

interface Breadcrumb {
	label: string;
	detail?: unknown;
	ts: number;
}

let lastCrumb: Breadcrumb | null = null;

/** Drop a breadcrumb on entry to a suspect region (cheap, no log). */
export function markPerf(label: string, detail?: unknown): void {
	if (!isPerfDebug()) return;
	lastCrumb = { label, detail, ts: performance.now() };
}

/** Freshest breadcrumb + its age, for the freeze detector to attribute a gap. */
export function getLastCrumb(): { label: string; ageMs: number; detail?: unknown } | null {
	if (!lastCrumb) return null;
	return {
		label: lastCrumb.label,
		ageMs: Math.round(performance.now() - lastCrumb.ts),
		detail: lastCrumb.detail,
	};
}

const SLOW_SYNC_MS = 50;

/** Time a synchronous region; breadcrumbs it and logs if it blocks > SLOW_SYNC_MS. */
export function timeSync<T>(label: string, fn: () => T, detail?: unknown): T {
	if (!isPerfDebug()) return fn();
	markPerf(label, detail);
	const t0 = performance.now();
	try {
		return fn();
	} finally {
		const dt = performance.now() - t0;
		if (dt > SLOW_SYNC_MS) {
			appLogger.warn("app", `SLOW ${label}: ${Math.round(dt)}ms`, detail);
		}
	}
}

/** Time a Solid `batch()`, separating the synchronous *body* (our setState
 *  calls) from the reactive *flush* that runs when the outermost batch() returns
 *  (dependent effects/memos waking). `run` receives `markBodyEnd`, which it MUST
 *  call as its last statement inside batch() — everything after that timestamp is
 *  flush. Dormant unless perfDebug; then logs only if the whole thing blocks
 *  > SLOW_SYNC_MS. This is how we attribute git.refreshBatch freezes to body vs
 *  flush without guessing. */
export function timeBatch(label: string, run: (markBodyEnd: () => void) => void): void {
	if (!isPerfDebug()) {
		run(() => {});
		return;
	}
	markPerf(label);
	const t0 = performance.now();
	let bodyEnd = t0;
	run(() => {
		bodyEnd = performance.now();
	});
	const total = performance.now() - t0;
	if (total > SLOW_SYNC_MS) {
		const body = bodyEnd - t0;
		appLogger.warn(
			"app",
			`SLOW ${label}: ${Math.round(total)}ms (body ${Math.round(body)}ms + flush ${Math.round(total - body)}ms)`,
		);
	}
}

// Frame-request burst detector — confirms/denies the repo-switch thundering-herd
// theory (many terminals firing terminal_request_frame in one tick on show).
const FRAME_BURST_WINDOW_MS = 250;
const FRAME_BURST_THRESHOLD = 6;
let frameReqWindowStart = 0;
let frameReqCount = 0;

export function noteFrameRequest(): void {
	if (!isPerfDebug()) return;
	const now = performance.now();
	if (now - frameReqWindowStart > FRAME_BURST_WINDOW_MS) {
		frameReqWindowStart = now;
		frameReqCount = 0;
	}
	frameReqCount++;
	if (frameReqCount === FRAME_BURST_THRESHOLD) {
		appLogger.warn(
			"app",
			`FRAME BURST: >=${FRAME_BURST_THRESHOLD} terminal_request_frame within ${FRAME_BURST_WINDOW_MS}ms`,
		);
	}
}
