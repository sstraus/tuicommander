import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	isTerminalVisible,
	latestIntersectionVisibility,
	needsGridResubscribe,
	REATTACH_PHASE_INITIAL,
	type ReattachPhase,
	retryUntilMeasured,
	retryUntilSized,
	SIZE_RETRY_MAX_FRAMES,
	stepReattachPhase,
} from "../../../components/Terminal/visibilityLifecycle";

describe("latestIntersectionVisibility", () => {
	it("uses the final state after a resize queues hidden and visible crossings together", () => {
		expect(
			latestIntersectionVisibility([
				{ isIntersecting: false, time: 10 },
				{ isIntersecting: true, time: 11 },
			]),
		).toBe(true);
	});

	it("uses entry time rather than assuming callback entries are already ordered", () => {
		expect(
			latestIntersectionVisibility([
				{ isIntersecting: true, time: 11 },
				{ isIntersecting: false, time: 10 },
			]),
		).toBe(true);
	});

	it("treats an empty delivery as hidden", () => {
		expect(latestIntersectionVisibility([])).toBe(false);
	});
});

/**
 * A hand-driven `requestAnimationFrame`. The real one never fires in happy-dom,
 * and a retry loop can only be proven to terminate by draining every frame it
 * asks for and then finding the queue empty.
 */
function fakeRaf() {
	let nextId = 1;
	const pending = new Map<number, FrameRequestCallback>();
	const cancelled: number[] = [];

	const request = vi.fn((cb: FrameRequestCallback) => {
		const id = nextId++;
		pending.set(id, cb);
		return id;
	});
	const cancel = vi.fn((id: number) => {
		cancelled.push(id);
		pending.delete(id);
	});

	/** Run every frame currently queued. Frames queued by those frames wait for the next call. */
	function step() {
		const due = [...pending.entries()];
		pending.clear();
		for (const [, cb] of due) cb(performance.now());
		return due.length;
	}

	/**
	 * Drain until nothing is queued. `limit` is a safety net, not an
	 * expectation — a loop that needs it has not terminated.
	 */
	function drain(limit: number) {
		let frames = 0;
		while (pending.size > 0) {
			if (frames >= limit) return { frames, drained: false };
			frames += step();
		}
		return { frames, drained: true };
	}

	return { request, cancel, step, drain, pending, cancelled, requestCount: () => request.mock.calls.length };
}

// File scope, so both frame-driven suites below drive the same fake without a
// second copy of this scaffolding. `stepReattachPhase` asks for no frames, so
// the stub is inert there.
let raf: ReturnType<typeof fakeRaf>;
let originalRequest: typeof globalThis.requestAnimationFrame;
let originalCancel: typeof globalThis.cancelAnimationFrame;

beforeEach(() => {
	raf = fakeRaf();
	originalRequest = globalThis.requestAnimationFrame;
	originalCancel = globalThis.cancelAnimationFrame;
	globalThis.requestAnimationFrame = raf.request as unknown as typeof globalThis.requestAnimationFrame;
	globalThis.cancelAnimationFrame = raf.cancel as unknown as typeof globalThis.cancelAnimationFrame;
});

afterEach(() => {
	globalThis.requestAnimationFrame = originalRequest;
	globalThis.cancelAnimationFrame = originalCancel;
});

describe("retryUntilSized", () => {
	/**
	 * The defect: a container that never gets a box — a collapsed pane, a hidden
	 * split — kept the old loop re-arming a frame every ~16 ms for the lifetime
	 * of the page, one loop per terminal.
	 */
	it("stops asking for frames when the container stays zero-size forever", () => {
		const onSized = vi.fn();
		const onExhausted = vi.fn();

		retryUntilSized(() => false, onSized, onExhausted);

		// Generous safety net: ten times the cap. Reaching it means no cap exists.
		const { drained, frames } = raf.drain(SIZE_RETRY_MAX_FRAMES * 10);

		expect(drained).toBe(true);
		expect(frames).toBe(SIZE_RETRY_MAX_FRAMES);
		expect(raf.pending.size).toBe(0);
		expect(onSized).not.toHaveBeenCalled();
		expect(onExhausted).toHaveBeenCalledTimes(1);
	});

	it("runs the callback on the frame the container gets a size, then stops", () => {
		const onSized = vi.fn();
		const onExhausted = vi.fn();
		let sized = false;

		retryUntilSized(() => sized, onSized, onExhausted);

		raf.step();
		raf.step();
		expect(onSized).not.toHaveBeenCalled();

		sized = true;
		raf.step();

		expect(onSized).toHaveBeenCalledTimes(1);
		expect(onExhausted).not.toHaveBeenCalled();
		expect(raf.pending.size).toBe(0);

		// Nothing left to run, so nothing can call it a second time.
		raf.step();
		expect(onSized).toHaveBeenCalledTimes(1);
	});

	/**
	 * Dispose during the retry: the old loop never kept the handle it would have
	 * had to cancel, so it outlived the component and called into a torn-down
	 * one.
	 */
	it("cancels the in-flight frame on dispose and never runs the callback afterwards", () => {
		const onSized = vi.fn();
		const onExhausted = vi.fn();
		let sized = false;

		const dispose = retryUntilSized(() => sized, onSized, onExhausted);

		raf.step();
		expect(raf.pending.size).toBe(1);
		const inFlight = [...raf.pending.keys()][0];

		dispose();

		expect(raf.cancelled).toContain(inFlight);
		expect(raf.pending.size).toBe(0);

		// The container getting a size after dispose must change nothing.
		sized = true;
		const { drained } = raf.drain(SIZE_RETRY_MAX_FRAMES * 10);
		expect(drained).toBe(true);
		expect(onSized).not.toHaveBeenCalled();
		expect(onExhausted).not.toHaveBeenCalled();
	});

	it("is idempotent when disposed twice and after it already gave up", () => {
		const dispose = retryUntilSized(() => false, vi.fn(), vi.fn(), 3);
		raf.drain(50);
		expect(() => {
			dispose();
			dispose();
		}).not.toThrow();
		expect(raf.pending.size).toBe(0);
	});

	it("honours an explicit frame cap", () => {
		const onExhausted = vi.fn();
		retryUntilSized(() => false, vi.fn(), onExhausted, 3);
		const { frames, drained } = raf.drain(100);
		expect(drained).toBe(true);
		expect(frames).toBe(3);
		expect(onExhausted).toHaveBeenCalledTimes(1);
	});
});

/**
 * The defect this exists for: `CanvasTerminal.remeasure()` read the pane's box,
 * found it degenerate, and returned — recording nothing and arming nothing. A
 * full page reload (any src/ edit under `make dev`) mounts every terminal
 * before layout has run, so that early return was the normal case, and the
 * canvas kept its mount-time geometry — a small box in the corner of the pane,
 * the rest black — until a window resize happened to run the measurement again.
 *
 * The measurement must therefore be re-taken when the pane gets a real box, and
 * the retry must be bounded and cancellable for the same reason `retryUntilSized`
 * is: a collapsed pane never gets one.
 */
describe("retryUntilMeasured", () => {
	/**
	 * A pane and the canvas it sizes. `measure` is the shape of `remeasure`: it
	 * reads the pane's box and can do nothing at all without one.
	 */
	function stubPane(width: number, height: number) {
		const pane = { width, height };
		const canvas = { width: 0, height: 0 };
		const measure = vi.fn(() => {
			if (pane.width <= 0 || pane.height <= 0) return false;
			canvas.width = pane.width;
			canvas.height = pane.height;
			return true;
		});
		return { pane, canvas, measure };
	}

	it("sizes the canvas on the frame a zero-sized pane gets a real box", () => {
		const { pane, canvas, measure } = stubPane(0, 0);

		retryUntilMeasured(measure);

		raf.step();
		expect(canvas).toEqual({ width: 0, height: 0 });

		pane.width = 1200;
		pane.height = 800;
		raf.step();

		expect(canvas).toEqual({ width: 1200, height: 800 });
		expect(raf.pending.size).toBe(0);
	});

	it("stops once it has measured, instead of re-measuring every frame", () => {
		const { pane, canvas, measure } = stubPane(0, 0);

		retryUntilMeasured(measure);
		raf.step();
		pane.width = 640;
		pane.height = 480;
		raf.step();

		const measured = measure.mock.calls.length;
		const { drained } = raf.drain(50);

		expect(drained).toBe(true);
		expect(measure.mock.calls.length).toBe(measured);
		expect(canvas).toEqual({ width: 640, height: 480 });
	});

	it("gives up on a pane that never gets a box instead of re-arming forever", () => {
		const { canvas, measure } = stubPane(0, 0);
		const onExhausted = vi.fn();

		retryUntilMeasured(measure, onExhausted);

		// Ten times the cap: reaching it means no cap exists.
		const { drained, frames } = raf.drain(SIZE_RETRY_MAX_FRAMES * 10);

		expect(drained).toBe(true);
		expect(frames).toBe(SIZE_RETRY_MAX_FRAMES);
		expect(onExhausted).toHaveBeenCalledTimes(1);
		expect(canvas).toEqual({ width: 0, height: 0 });
	});

	it("measures nothing after dispose, even once the pane is sized", () => {
		const { pane, canvas, measure } = stubPane(0, 0);

		const dispose = retryUntilMeasured(measure);
		raf.step();
		dispose();

		pane.width = 800;
		pane.height = 600;
		const { drained } = raf.drain(SIZE_RETRY_MAX_FRAMES * 10);

		expect(drained).toBe(true);
		expect(canvas).toEqual({ width: 0, height: 0 });
	});
});

/**
 * Only a floating window takes the session's grid channel away from this
 * terminal. A plain tab switch leaves the subscription intact, so resubscribing
 * on it bought nothing and cost a visible paint → wipe → paint.
 */
describe("stepReattachPhase", () => {
	const run = (phase: ReattachPhase, visible: boolean, detached: boolean) =>
		stepReattachPhase(phase, visible, detached);

	it("does not resubscribe when the terminal first becomes visible at mount", () => {
		const first = run(REATTACH_PHASE_INITIAL, true, false);
		expect(first.resubscribe).toBe(false);
	});

	it("does not resubscribe on a tab switch away and back", () => {
		let phase = run(REATTACH_PHASE_INITIAL, true, false).phase;

		const away = run(phase, false, false);
		expect(away.resubscribe).toBe(false);
		phase = away.phase;

		const back = run(phase, true, false);
		expect(back.resubscribe).toBe(false);
	});

	it("does not resubscribe on repeated visible runs", () => {
		let phase = run(REATTACH_PHASE_INITIAL, true, false).phase;
		for (let i = 0; i < 5; i++) {
			const again = run(phase, true, false);
			expect(again.resubscribe).toBe(false);
			phase = again.phase;
		}
	});

	it("resubscribes once after a detach into a floating window and back", () => {
		let phase = run(REATTACH_PHASE_INITIAL, true, false).phase;

		// Detach: the tab moves to a floating window, so it is no longer visible here.
		phase = run(phase, false, true).phase;
		// Another tab is selected while detached — several effect runs, still detached.
		phase = run(phase, false, true).phase;

		const reattached = run(phase, true, false);
		expect(reattached.resubscribe).toBe(true);

		// The edge is consumed: staying visible must not resubscribe again.
		expect(run(reattached.phase, true, false).resubscribe).toBe(false);
	});

	/**
	 * `reattach()` and `handleTerminalSelect()` are two separate store writes, so
	 * the effect can run once with the detach already cleared but the tab not yet
	 * selected. The "a detach happened" fact has to survive that run.
	 */
	it("resubscribes when the reattach and the tab selection land in separate runs", () => {
		let phase = run(REATTACH_PHASE_INITIAL, true, false).phase;
		phase = run(phase, false, true).phase;

		// Run 1: no longer detached, but the tab is not the active one yet.
		const cleared = run(phase, false, false);
		expect(cleared.resubscribe).toBe(false);

		// Run 2: the tab becomes active again.
		expect(run(cleared.phase, true, false).resubscribe).toBe(true);
	});

	it("never resubscribes while the terminal is still detached", () => {
		let phase = run(REATTACH_PHASE_INITIAL, true, false).phase;
		// A pane group can report this tab as active even while it is detached.
		const detached = run(phase, true, true);
		expect(detached.resubscribe).toBe(false);
		phase = detached.phase;
		expect(run(phase, true, true).resubscribe).toBe(false);
	});

	it("exposes the same decision as a predicate over the phase", () => {
		expect(needsGridResubscribe({ visible: false, sawDetach: true }, true, false)).toBe(true);
		expect(needsGridResubscribe({ visible: false, sawDetach: false }, true, false)).toBe(false);
		expect(needsGridResubscribe({ visible: false, sawDetach: true }, false, false)).toBe(false);
		expect(needsGridResubscribe({ visible: false, sawDetach: true }, true, true)).toBe(false);
	});
});

describe("isTerminalVisible", () => {
	const base = {
		alwaysVisible: false,
		paneHosted: false,
		active: false,
		detached: false,
		inPaneGroup: false,
		activeInPaneGroup: false,
	};

	// catches: the flat-list copy of a split terminal counted as visible, so both
	// copies subscribed the session's single grid channel and the pane froze.
	it("hides the flat-list copy while its tab is in a split pane", () => {
		expect(isTerminalVisible({ ...base, active: true, inPaneGroup: true, activeInPaneGroup: true })).toBe(false);
	});

	// catches: the pane copy shown for a tab its pane is not showing.
	it("shows the pane copy only when its pane shows that tab", () => {
		expect(isTerminalVisible({ ...base, paneHosted: true, inPaneGroup: true, activeInPaneGroup: true })).toBe(true);
		expect(isTerminalVisible({ ...base, paneHosted: true, active: true, inPaneGroup: true })).toBe(false);
	});

	// catches: a split rule leaking into the unsplit layout, hiding every terminal.
	it("shows the active flat-list terminal when there is no split", () => {
		expect(isTerminalVisible({ ...base, active: true })).toBe(true);
		expect(isTerminalVisible({ ...base })).toBe(false);
	});

	// catches: a terminal outside every pane (an orphan) hidden while a split is on.
	it("shows the flat-list copy of an active terminal outside every pane", () => {
		expect(isTerminalVisible({ ...base, active: true, inPaneGroup: false })).toBe(true);
	});

	// catches: the main window drawing a terminal that lives in a floating window.
	it("hides a detached terminal in the main window", () => {
		expect(isTerminalVisible({ ...base, active: true, detached: true })).toBe(false);
	});

	// catches: the floating window losing its only terminal to the split rules.
	it("always shows the floating-window terminal", () => {
		expect(isTerminalVisible({ ...base, alwaysVisible: true, inPaneGroup: true })).toBe(true);
	});
});
