import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initMouseDrag, type MouseDragCallbacks } from "../../hooks/useMouseDrag";

// Pointer state machine tested against real DOM events (happy-dom) — no mocks
// beyond vi.fn callbacks. useMouseDrag is AGENTS-flagged fragile (the
// load-bearing preventDefault and the three cleanup exit paths), so these lock
// in: left-button gating, the movement threshold, preventDefault-on-every-move,
// and cleanup via pointerup / pointercancel / Escape.

type Cbs = {
	onStart: ReturnType<typeof vi.fn>;
	onMove: ReturnType<typeof vi.fn>;
	onDrop: ReturnType<typeof vi.fn>;
	onCancel: ReturnType<typeof vi.fn>;
};

function pointer(type: string, init: PointerEventInit): PointerEvent {
	return new PointerEvent(type, {
		bubbles: true,
		cancelable: true,
		button: 0,
		pointerId: 1,
		pointerType: "mouse",
		...init,
	});
}

describe("initMouseDrag", () => {
	let source: HTMLElement;
	let cbs: Cbs;

	beforeEach(() => {
		source = document.createElement("div");
		document.body.appendChild(source);
		cbs = { onStart: vi.fn(), onMove: vi.fn(), onDrop: vi.fn(), onCancel: vi.fn() };
	});

	afterEach(() => {
		// Tear down any drag left mid-flight so its document listeners/timers don't
		// bleed into the next test.
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
		document.body.innerHTML = "";
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	const start = (x = 100, y = 100, opts?: Parameters<typeof initMouseDrag>[3], pointerType = "mouse") =>
		initMouseDrag(
			pointer("pointerdown", { clientX: x, clientY: y, pointerType }),
			source,
			cbs as unknown as MouseDragCallbacks,
			opts,
		);
	const move = (x: number, y: number, pointerType = "mouse") => {
		const e = pointer("pointermove", { clientX: x, clientY: y, pointerType });
		document.dispatchEvent(e);
		return e;
	};
	const up = (x: number, y: number) => document.dispatchEvent(pointer("pointerup", { clientX: x, clientY: y }));

	it("catches an owner-disposed drag leaving a ghost or dispatching a late drop", () => {
		const cancel = start();
		move(160, 160);
		expect(document.body.children).toHaveLength(2);
		cancel?.();
		cancel?.();
		expect(document.body.children).toHaveLength(1);
		up(180, 180);
		expect(cbs.onDrop).not.toHaveBeenCalled();
		expect(cbs.onCancel).toHaveBeenCalledTimes(1);
	});

	it("ignores a non-left-button pointerdown (never attaches the drag)", () => {
		initMouseDrag(
			pointer("pointerdown", { clientX: 0, clientY: 0, button: 2 }),
			source,
			cbs as unknown as MouseDragCallbacks,
		);
		move(60, 60);
		expect(cbs.onMove).not.toHaveBeenCalled();
		expect(cbs.onStart).not.toHaveBeenCalled();
	});

	it("does not start the drag until the movement threshold is crossed", () => {
		start(100, 100);
		move(102, 101); // |2|+|1| = 3 < 5
		expect(cbs.onStart).not.toHaveBeenCalled();
		expect(source.style.opacity).toBe(""); // source not yet dimmed
		move(106, 100); // |6|+|0| = 6 >= 5
		expect(cbs.onStart).toHaveBeenCalledTimes(1);
		expect(cbs.onMove).toHaveBeenCalled();
		expect(source.style.opacity).toBe("0.35"); // dimmed once dragging
	});

	it("preventDefaults on EVERY mouse move, including sub-threshold (load-bearing)", () => {
		start(100, 100);
		const sub = move(101, 100); // below threshold
		expect(sub.defaultPrevented).toBe(true);
	});

	it("drops via pointerup and then removes its listeners", () => {
		start(100, 100);
		move(112, 100); // cross threshold → started
		up(120, 105);
		expect(cbs.onDrop).toHaveBeenCalledWith(120, 105);
		expect(cbs.onCancel).not.toHaveBeenCalled();
		expect(source.style.opacity).toBe(""); // cleaned up
		// Listeners gone: a further move is inert.
		cbs.onMove.mockClear();
		move(140, 100);
		expect(cbs.onMove).not.toHaveBeenCalled();
	});

	it("treats a pointerup below threshold as a click (no onDrop, no onStart)", () => {
		start(100, 100);
		up(101, 100); // never crossed threshold
		expect(cbs.onStart).not.toHaveBeenCalled();
		expect(cbs.onDrop).not.toHaveBeenCalled();
	});

	it("cancels via pointercancel after a started drag", () => {
		start(100, 100);
		move(112, 100);
		document.dispatchEvent(pointer("pointercancel", { clientX: 112, clientY: 100 }));
		expect(cbs.onCancel).toHaveBeenCalledTimes(1);
		expect(cbs.onDrop).not.toHaveBeenCalled();
		expect(source.style.opacity).toBe("");
	});

	it("cancels via Escape after a started drag and removes its listeners", () => {
		start(100, 100);
		move(112, 100);
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
		expect(cbs.onCancel).toHaveBeenCalledTimes(1);
		expect(source.style.opacity).toBe("");
		cbs.onMove.mockClear();
		move(140, 100);
		expect(cbs.onMove).not.toHaveBeenCalled();
	});

	it("Escape before the drag starts cleans up without firing onCancel", () => {
		start(100, 100);
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
		expect(cbs.onCancel).not.toHaveBeenCalled();
		cbs.onMove.mockClear();
		move(140, 100);
		expect(cbs.onMove).not.toHaveBeenCalled(); // listeners removed
	});

	it("touch: arms the drag in place after the long-press hold", () => {
		vi.useFakeTimers();
		start(0, 0, undefined, "touch");
		expect(cbs.onStart).not.toHaveBeenCalled(); // hold not elapsed
		vi.advanceTimersByTime(350);
		expect(cbs.onStart).toHaveBeenCalledTimes(1); // armed → beginDrag
	});

	it("touch: a move beyond slop before the hold bails out as a scroll", () => {
		vi.useFakeTimers();
		start(0, 0, undefined, "touch");
		move(0, 20, "touch"); // dy 20 > slop 10 → cleanup, no drag
		vi.advanceTimersByTime(350); // long-press timer was cleared
		expect(cbs.onStart).not.toHaveBeenCalled();
	});

	const stubHover = (hover: boolean) =>
		vi.stubGlobal("matchMedia", (q: string) => ({ matches: hover && q === "(hover: hover)" }));

	// Catches: a pen swipe starting a drag after 5px like a mouse (a scroll then drops the item).
	it("pen without hover: a move beyond slop before the hold bails out as a scroll", () => {
		stubHover(false);
		vi.useFakeTimers();
		start(0, 0, undefined, "pen");
		move(0, 20, "pen");
		vi.advanceTimersByTime(350);
		expect(cbs.onStart).not.toHaveBeenCalled();
		expect(cbs.onMove).not.toHaveBeenCalled();
	});

	// Catches: a hover-capable desktop pen (Wacom/Surface) forced into the 350 ms hold.
	it("pen on a hover device drags immediately like a mouse", () => {
		stubHover(true);
		start(0, 0, undefined, "pen");
		move(0, 20, "pen");
		expect(cbs.onStart).toHaveBeenCalledTimes(1);
	});
});
