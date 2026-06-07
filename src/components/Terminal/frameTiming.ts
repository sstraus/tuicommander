// --- Phase 0 measurement harness: decode + paint timing for CanvasTerminal ---
//
// Pure, DOM-free ring buffer so it stays unit-testable and can be reused inside
// the render worker in Phase 1. Decode and paint are recorded as separate
// samples because they happen at different times (decode in onFrame, paint in
// the rAF callback), not back-to-back.

/** Max samples retained per metric per session (a few seconds of frames). */
export const FRAME_TIMING_CAPACITY = 256;

// "decode" + "paint" are durations on whichever thread owns them. "sched" is
// worker-mode only: the gap from a frame making the grid dirty to the worker's
// repaint callback actually running (rAF or timer fallback, whichever wins). It
// isolates WebKit deprioritizing the worker's rAF/timer for sporadic paints —
// the suspected input-lag cause. NOTE: "sched" is NOT end-to-end perceived
// latency: it excludes upstream (keystroke→PTY→Rust ticker→IPC→main decode→
// transfer, all before dirty) AND the compositor commit (callback→pixels). Read
// it as worker render-scheduling delay only.
export type FrameTimingKind = "decode" | "paint" | "sched";

export interface MetricStats {
	count: number;
	p50: number;
	p95: number;
	max: number;
}

export interface FrameTimingStats {
	decode: MetricStats;
	paint: MetricStats;
	sched: MetricStats;
}

interface SessionRings {
	decode: number[];
	paint: number[];
	sched: number[];
}

const sessions = new Map<string, SessionRings>();

function ringFor(sessionId: string, kind: FrameTimingKind): number[] {
	let rings = sessions.get(sessionId);
	if (!rings) {
		rings = { decode: [], paint: [], sched: [] };
		sessions.set(sessionId, rings);
	}
	return rings[kind];
}

/** Record one timing sample (milliseconds) for a session metric. */
export function recordFrameTiming(sessionId: string, kind: FrameTimingKind, ms: number): void {
	const ring = ringFor(sessionId, kind);
	ring.push(ms);
	if (ring.length > FRAME_TIMING_CAPACITY) {
		ring.shift();
	}
}

/** Nearest-rank percentile (p in 0..100) over an unsorted sample array. */
function percentile(sorted: number[], p: number): number {
	if (sorted.length === 0) return 0;
	const rank = Math.ceil((p / 100) * sorted.length);
	const idx = Math.min(sorted.length - 1, Math.max(0, rank - 1));
	return sorted[idx];
}

function statsFor(ring: number[]): MetricStats {
	if (ring.length === 0) {
		return { count: 0, p50: 0, p95: 0, max: 0 };
	}
	const sorted = [...ring].sort((a, b) => a - b);
	return {
		count: sorted.length,
		p50: percentile(sorted, 50),
		p95: percentile(sorted, 95),
		max: sorted[sorted.length - 1],
	};
}

/** Aggregate decode/paint percentiles for a session (zeroed if unknown). */
export function getFrameTimingStats(sessionId: string): FrameTimingStats {
	const rings = sessions.get(sessionId);
	if (!rings) {
		const empty: MetricStats = { count: 0, p50: 0, p95: 0, max: 0 };
		return { decode: { ...empty }, paint: { ...empty }, sched: { ...empty } };
	}
	return { decode: statsFor(rings.decode), paint: statsFor(rings.paint), sched: statsFor(rings.sched) };
}

/** Clear timing for one session, or all sessions when no id is given. */
export function resetFrameTiming(sessionId?: string): void {
	if (sessionId === undefined) {
		sessions.clear();
	} else {
		sessions.delete(sessionId);
	}
}

// --- Enable flag + debug hook ---
//
// Recording itself is pure/unconditional (see recordFrameTiming) so it stays
// trivially testable. The flag gates the *call sites* in CanvasTerminal so the
// harness costs nothing (no performance.now(), no push) when off.

let enabled = false;

// Worker-mode terminals register here so toggling timing also reaches their
// render worker (a separate module instance with its own `enabled` flag). Each
// listener posts the new state to its worker; unregistered on terminal cleanup.
const enabledListeners = new Set<(on: boolean) => void>();

export function onFrameTimingEnabledChange(cb: (on: boolean) => void): () => void {
	enabledListeners.add(cb);
	return () => enabledListeners.delete(cb);
}

export function setFrameTimingEnabled(on: boolean): void {
	enabled = on;
	for (const cb of enabledListeners) cb(on);
}

export function isFrameTimingEnabled(): boolean {
	return enabled;
}

/**
 * Expose a read-only console accessor at `globalThis.__terminalFrameTiming`
 * so a baseline can be captured manually (e.g. via the devtools attached to
 * the app server on :9876). Idempotent.
 */
export function installFrameTimingDebugHook(): void {
	(globalThis as Record<string, unknown>).__terminalFrameTiming = {
		stats: getFrameTimingStats,
		enable: setFrameTimingEnabled,
		reset: resetFrameTiming,
	};
}
