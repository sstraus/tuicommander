/**
 * perfDebug — runtime control flag for the frontend performance/debug
 * instrumentation layer (perfTrace breadcrumbs, timeSync, the frame-burst
 * detector, and the freeze detector).
 *
 * Default: `import.meta.env.DEV` → ACTIVE while developing, DORMANT in a
 * release build, so we ship a quiet binary and never distribute hyper-logging.
 *
 * Runtime-toggleable (deliberately NOT `import.meta.env.DEV` inline, so it is
 * NOT tree-shaken): a release build can be woken up to diagnose a field issue
 * via `window.__TUIC__.setPerfDebug(true)`. The choice is persisted to
 * localStorage so it survives a reload.
 *
 * Dormant cost is a single boolean read at each instrumentation entry point —
 * negligible even on per-frame hot paths.
 */

const STORAGE_KEY = "tuic.perfDebug";

function initialValue(): boolean {
	try {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (stored === "true") return true;
		if (stored === "false") return false;
	} catch {
		// localStorage unavailable (sandboxed iframe / SSR) — fall back to build default.
	}
	return import.meta.env.DEV;
}

let enabled = initialValue();

/** Whether the frontend perf/debug instrumentation should record + log. */
export function isPerfDebug(): boolean {
	return enabled;
}

/** Toggle the perf/debug layer at runtime; persists across reloads. */
export function setPerfDebug(on: boolean): void {
	enabled = on;
	try {
		localStorage.setItem(STORAGE_KEY, on ? "true" : "false");
	} catch {
		// Persistence is best-effort; the in-memory flag still applies this session.
	}
}
