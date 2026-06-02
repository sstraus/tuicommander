export interface TouchHandlerOptions {
	/** Called on every pixel of scroll movement (raw dy + momentum). */
	onScrollPixels: (dy: number) => void;
	/** Called when momentum finishes or touch ends without movement. */
	onScrollEnd: () => void;
	onInput: (data: string) => void;
	onFocus: () => void;
	onFontSizeChange: (delta: number) => void;
	onSelectionMode: () => void;
}

const LONG_PRESS_MS = 600;
const MOVE_CANCEL_PX = 10;
const PINCH_SCALE_FACTOR = 0.1;
// Momentum: xterm uses SCROLL_FRICTION = -0.005 (px/ms²).
// We apply per-frame: v += FRICTION * dt, stop when v <= 0.
const MOMENTUM_FRICTION = -0.003;
const VELOCITY_WINDOW_MS = 100;

function touchDist(a: Touch, b: Touch): number {
	const dx = a.clientX - b.clientX;
	const dy = a.clientY - b.clientY;
	return Math.sqrt(dx * dx + dy * dy);
}

export function installTouchHandlers(
	canvas: HTMLCanvasElement,
	textarea: HTMLTextAreaElement,
	opts: TouchHandlerOptions,
): () => void {
	let startY = 0;
	let startDist = 0;
	let longPressTimer: ReturnType<typeof setTimeout> | null = null;
	let longPressFired = false;

	let momentumRaf = 0;
	let momentumVelocity = 0; // px/ms
	let momentumDir = 0; // +1 or -1
	let momentumLastTime = 0;
	const moveHistory: { dy: number; time: number }[] = [];

	function cancelLongPress() {
		if (longPressTimer !== null) {
			clearTimeout(longPressTimer);
			longPressTimer = null;
		}
	}

	function stopMomentum() {
		if (momentumRaf) {
			cancelAnimationFrame(momentumRaf);
			momentumRaf = 0;
		}
		momentumVelocity = 0;
	}

	function momentumTick() {
		const now = performance.now();
		const dt = now - momentumLastTime;
		momentumLastTime = now;

		momentumVelocity += MOMENTUM_FRICTION * dt;
		if (momentumVelocity <= 0) {
			stopMomentum();
			opts.onScrollEnd();
			return;
		}

		const dy = momentumDir * momentumVelocity * dt;
		opts.onScrollPixels(dy);
		momentumRaf = requestAnimationFrame(momentumTick);
	}

	function computeVelocity(): { speed: number; dir: number } {
		const now = performance.now();
		const cutoff = now - VELOCITY_WINDOW_MS;
		let totalDy = 0;
		let oldest = now;
		for (let i = moveHistory.length - 1; i >= 0; i--) {
			if (moveHistory[i].time < cutoff) break;
			totalDy += moveHistory[i].dy;
			oldest = moveHistory[i].time;
		}
		const elapsed = now - oldest;
		if (elapsed < 8) return { speed: 0, dir: 0 };
		return { speed: Math.abs(totalDy / elapsed), dir: totalDy > 0 ? 1 : -1 };
	}

	function onTouchStart(e: TouchEvent) {
		stopMomentum();
		if (e.touches.length === 1) {
			startY = e.touches[0].clientY;
			moveHistory.length = 0;
			longPressFired = false;
			longPressTimer = setTimeout(() => {
				longPressFired = true;
				opts.onSelectionMode();
			}, LONG_PRESS_MS);
		} else if (e.touches.length === 2) {
			cancelLongPress();
			startDist = touchDist(e.touches[0], e.touches[1]);
		}
	}

	function onTouchMove(e: TouchEvent) {
		if (e.touches.length === 1) {
			e.preventDefault();
			const dy = e.touches[0].clientY - startY;
			if (Math.abs(dy) > MOVE_CANCEL_PX) {
				cancelLongPress();
			}
			const now = performance.now();
			moveHistory.push({ dy, time: now });
			while (moveHistory.length > 0 && moveHistory[0].time < now - VELOCITY_WINDOW_MS) {
				moveHistory.shift();
			}
			opts.onScrollPixels(dy);
			startY = e.touches[0].clientY;
		} else if (e.touches.length === 2) {
			const dist = touchDist(e.touches[0], e.touches[1]);
			const ratio = dist / startDist - 1;
			if (Math.abs(ratio) > 0.02) {
				opts.onFontSizeChange(ratio * PINCH_SCALE_FACTOR * 100);
				startDist = dist;
			}
		}
	}

	function onTouchEnd(e: TouchEvent) {
		if (e.changedTouches.length > 0 && !longPressFired && e.touches.length === 0) {
			const moved = moveHistory.length > 0;
			if (!moved) {
				textarea.focus({ preventScroll: true });
				opts.onFocus();
			} else {
				const v = computeVelocity();
				if (v.speed > 0.1) {
					momentumVelocity = v.speed;
					momentumDir = v.dir;
					momentumLastTime = performance.now();
					momentumRaf = requestAnimationFrame(momentumTick);
				} else {
					opts.onScrollEnd();
				}
			}
		}
		cancelLongPress();
	}

	function onInput() {
		const value = textarea.value;
		if (value) {
			opts.onInput(value);
			textarea.value = "";
		}
	}

	canvas.addEventListener("touchstart", onTouchStart, { passive: true });
	canvas.addEventListener("touchmove", onTouchMove, { passive: false });
	canvas.addEventListener("touchend", onTouchEnd);
	textarea.addEventListener("input", onInput);

	return () => {
		canvas.removeEventListener("touchstart", onTouchStart);
		canvas.removeEventListener("touchmove", onTouchMove);
		canvas.removeEventListener("touchend", onTouchEnd);
		textarea.removeEventListener("input", onInput);
		cancelLongPress();
		stopMomentum();
	};
}
