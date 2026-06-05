// Per-frame revision-bump coalescer for the repo-changed cascade.
//
// `repo-changed` events can arrive in same-frame bursts (a real change touching
// both .git/index and refs/**, or several repos at once). Bumping the repo
// revision synchronously on each one fires the full ~20-effect SolidJS flush per
// event. This collapses a burst into AT MOST ONE bump per repo per animation
// frame — without ever LOSING a bump (each distinct repo is flushed on the next
// frame), so panels still re-fetch fresh data exactly once.

export interface RevisionCoalescer {
	/** Queue a revision bump for `repoPath`, delivered once on the next frame. */
	bump(repoPath: string): void;
	/** Cancel any pending flush (teardown). */
	dispose(): void;
}

/** Schedule a callback to run on the next animation frame. */
export type FrameScheduler = (cb: () => void) => number;
/** Cancel a previously scheduled frame callback. */
export type FrameCanceller = (handle: number) => void;

const defaultSchedule: FrameScheduler = (cb) => requestAnimationFrame(cb);
const defaultCancel: FrameCanceller = (handle) => cancelAnimationFrame(handle);

/**
 * Create a coalescer that delivers at most one `bump(repoPath)` per repo per
 * scheduled frame. `schedule`/`cancel` are injectable for deterministic tests.
 */
export function createRevisionCoalescer(
	bump: (repoPath: string) => void,
	schedule: FrameScheduler = defaultSchedule,
	cancel: FrameCanceller = defaultCancel,
): RevisionCoalescer {
	const pending = new Set<string>();
	let handle: number | null = null;

	const flush = () => {
		handle = null;
		// Snapshot before delivering: a bump fired during flush re-arms a new frame
		// rather than mutating the set we're iterating.
		const paths = [...pending];
		pending.clear();
		for (const path of paths) bump(path);
	};

	return {
		bump(repoPath: string): void {
			pending.add(repoPath);
			if (handle === null) handle = schedule(flush);
		},
		dispose(): void {
			if (handle !== null) {
				cancel(handle);
				handle = null;
			}
			pending.clear();
		},
	};
}
