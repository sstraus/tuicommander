/**
 * Visibility and sizing lifecycle rules for `Terminal.tsx` and
 * `CanvasTerminal.tsx`, kept out of the components so they can be tested —
 * mounting either needs the whole canvas/transport mock stack, which is why
 * both are excluded from coverage.
 */

/**
 * How many frames to wait for a zero-size container before giving up.
 * ~2 s at 60 fps: longer than any layout pass, and bounded.
 */
export const SIZE_RETRY_MAX_FRAMES = 120;

/**
 * Return the newest visibility state from one IntersectionObserver delivery.
 *
 * A fast viewport/layout resize can cross the threshold twice before the
 * callback runs, so the delivery may contain both the transient hidden entry
 * and the final visible entry. Reading `entries[0]` leaves the terminal stuck
 * hidden even though its canvas is on screen; live grid frames are then decoded
 * but deliberately skipped before they update the row map or schedule a paint.
 */
export function latestIntersectionVisibility(
	entries: readonly Pick<IntersectionObserverEntry, "isIntersecting" | "time">[],
): boolean {
	let latest: Pick<IntersectionObserverEntry, "isIntersecting" | "time"> | undefined;
	for (const entry of entries) {
		if (!latest || entry.time >= latest.time) latest = entry;
	}
	return latest?.isIntersecting ?? false;
}

/**
 * Wait for a terminal container to get a real box, then run `onSized` once.
 *
 * A container can mount with a zero box — a tab that becomes visible one frame
 * before layout runs, a pane mid-resize — so a retry is needed. It must be
 * bounded and cancellable: a pane the user leaves collapsed never gets a size,
 * and an unbounded loop re-arms a frame every ~16 ms for the lifetime of the
 * page, once per terminal, with no handle to cancel when the component goes
 * away.
 *
 * Returns a disposer. After it runs, neither callback can fire again.
 */
export function retryUntilSized(
	isSized: () => boolean,
	onSized: () => void,
	onExhausted?: () => void,
	maxFrames: number = SIZE_RETRY_MAX_FRAMES,
): () => void {
	let handle = 0;
	let framesLeft = maxFrames;
	let disposed = false;

	const tick = () => {
		handle = 0;
		if (disposed) return;
		if (isSized()) {
			onSized();
			return;
		}
		if (--framesLeft <= 0) {
			onExhausted?.();
			return;
		}
		handle = requestAnimationFrame(tick);
	};

	handle = requestAnimationFrame(tick);

	return () => {
		disposed = true;
		if (handle) cancelAnimationFrame(handle);
		handle = 0;
	};
}

/** `retryUntilMeasured`'s attempt does its own work; there is nothing left to do after it. */
const NOOP = () => {};

/**
 * Re-run a measurement that could not take its container's box, until it can.
 *
 * `CanvasTerminal.remeasure()` reads `getBoundingClientRect()` and can do
 * nothing at all with a degenerate box — no metrics, no canvas size, no
 * `resize_pty`. It used to just return there, which loses the measurement: a
 * full page reload mounts every terminal before layout runs, and the canvas
 * then kept its mount-time geometry (a small box in the corner of the pane)
 * until a window resize happened to run the measurement again. The pane's
 * ResizeObserver is no answer on its own — it is installed only after `onMount`
 * has awaited three IPC subscriptions and a webfont load.
 *
 * `attempt` returns true once it has measured. It is its own predicate on
 * purpose: it reads the box and returns immediately when there is nothing to
 * measure, so polling it costs exactly what polling a separate `isSized` would.
 *
 * Returns a disposer, with the bound and the cancellation semantics of
 * `retryUntilSized`.
 */
export function retryUntilMeasured(
	attempt: () => boolean,
	onExhausted?: () => void,
	maxFrames: number = SIZE_RETRY_MAX_FRAMES,
): () => void {
	return retryUntilSized(attempt, NOOP, onExhausted, maxFrames);
}

/** What the visibility effect carries between runs. */
export interface ReattachPhase {
	/** Was the terminal visible on the previous run? */
	visible: boolean;
	/** Has a detach been seen since the terminal was last visible? */
	sawDetach: boolean;
}

export const REATTACH_PHASE_INITIAL: ReattachPhase = { visible: false, sawDetach: false };

/**
 * True only for a real reattach: the tab was detached into a floating window —
 * whose own Terminal took over the session's grid channel — and is visible here
 * again.
 *
 * A plain tab switch also flips visibility false → true, but nothing touched
 * this terminal's subscription while it was hidden. Resubscribing there bought
 * nothing and cost a visible paint → wipe → paint, because `refresh()` drops the
 * current frame before the replacement arrives.
 */
export function needsGridResubscribe(prev: ReattachPhase, visible: boolean, detached: boolean): boolean {
	return visible && !detached && !prev.visible && prev.sawDetach;
}

/**
 * Advance the phase and say whether this run is a reattach.
 *
 * `sawDetach` is a latch, not an edge on the previous run: `reattach()` and the
 * tab selection that follows it are two separate store writes, so the effect can
 * run once with the detach already cleared and the tab not yet active. An edge
 * test would lose the reattach in that gap.
 */
export function stepReattachPhase(
	prev: ReattachPhase,
	visible: boolean,
	detached: boolean,
): { phase: ReattachPhase; resubscribe: boolean } {
	if (detached) return { phase: { visible: false, sawDetach: true }, resubscribe: false };
	if (!visible) return { phase: { visible: false, sawDetach: prev.sawDetach }, resubscribe: false };
	return { phase: { visible: true, sawDetach: false }, resubscribe: needsGridResubscribe(prev, visible, detached) };
}

/** Where one `Terminal` instance is mounted, and the state that decides whether it is on screen. */
export interface TerminalVisibilityInput {
	/** A floating window: its only terminal is always on screen. */
	alwaysVisible: boolean;
	/** Mounted inside a split pane, not in the flat terminal list. */
	paneHosted: boolean;
	/** The terminal is the active one in the terminals store. */
	active: boolean;
	detached: boolean;
	/** The split is on and this terminal belongs to one of its panes. */
	inPaneGroup: boolean;
	/** The split is on and this terminal is the tab its pane shows. */
	activeInPaneGroup: boolean;
}

/**
 * Whether this `Terminal` instance is the one on screen.
 *
 * A terminal in a split pane is mounted twice: once in the flat list (always
 * mounted, hidden while the split owns the tab) and once in its pane. Only one
 * of the two may count as visible. When both did, both initialised and
 * subscribed the session's grid, and the desktop backend keeps one grid channel
 * per session: the hidden copy could take the frames, and the shown one froze.
 */
export function isTerminalVisible(input: TerminalVisibilityInput): boolean {
	if (input.alwaysVisible) return true;
	if (input.paneHosted) return input.activeInPaneGroup;
	return input.active && !input.detached && !input.inPaneGroup;
}
