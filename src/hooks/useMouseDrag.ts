/**
 * Pointer-based drag utility — replaces HTML5 DnD which conflicts with
 * Tauri's dragDropEnabled=true on macOS (WKWebView intercepts all
 * NSDragging events, preventing HTML5 drop from firing on targets).
 *
 * Why pointer events (not mouse events): in WKWebView with dragDropEnabled,
 * starting a drag on selectable text/elements kicks off a native NSDragging
 * session that swallows `mouseup`, so a mouse-based drag never receives its
 * release and stays glued to the cursor. setPointerCapture guarantees the
 * pointerup/pointercancel is delivered, ending the drag reliably. We capture
 * only once the movement threshold is crossed, so plain clicks keep their
 * normal click semantics (no capture, no interference with child onClick).
 *
 * Touch vs mouse: a pointer drag that preventDefaults early (load-bearing on
 * desktop WKWebView — see the comment in handleMove) would, on a touchscreen,
 * kill native scrolling and turn every vertical swipe into a drag. So on touch
 * we use a long-press to arm the drag and never preventDefault until it starts:
 * a swipe that moves before the hold completes is a scroll and we bail out,
 * leaving the browser to scroll normally. Mouse/pen keep the original
 * threshold-based behavior unchanged.
 *
 * Usage: call initMouseDrag() from a pointerdown handler. It waits for a
 * movement threshold (mouse) or a hold (touch) before starting the drag,
 * creates a ghost clone, and calls back with CSS-pixel coordinates on
 * move/drop.
 */

export interface MouseDragCallbacks {
	onStart?: () => void;
	onMove: (x: number, y: number) => void;
	onDrop: (x: number, y: number) => void;
	onCancel?: () => void;
}

export function initMouseDrag(
	e: PointerEvent,
	sourceEl: HTMLElement,
	callbacks: MouseDragCallbacks,
	options?: { threshold?: number; ghostOpacity?: number; touchLongPressMs?: number; touchSlop?: number },
): void {
	if (e.button !== 0) return;

	const threshold = options?.threshold ?? 5;
	const ghostOpacity = options?.ghostOpacity ?? 0.8;
	const isTouch = e.pointerType === "touch";
	// Touch drag arms after a hold; a move beyond touchSlop before then is a scroll.
	const longPressMs = options?.touchLongPressMs ?? 350;
	const touchSlop = options?.touchSlop ?? 10;

	const startX = e.clientX;
	const startY = e.clientY;
	const pointerId = e.pointerId;
	const rect = sourceEl.getBoundingClientRect();
	const offsetX = e.clientX - rect.left;
	const offsetY = e.clientY - rect.top;

	let started = false;
	let captured = false;
	let ghost: HTMLElement | null = null;
	// Touch only: set when the long-press timer fires; the drag may begin from then on.
	let longPressArmed = false;
	let longPressTimer: ReturnType<typeof setTimeout> | undefined;
	// Latest finger position, so the long-press timer can lift the item in place.
	let lastX = e.clientX;
	let lastY = e.clientY;
	// Restored on cleanup for touch drags (set to "none" to stop iOS scroll mid-drag).
	const prevTouchAction = sourceEl.style.touchAction;

	const beginDrag = (clientX: number, clientY: number) => {
		started = true;

		// Capture only now (real drag, not a click/scroll): guarantees pointerup
		// reaches us even if WKWebView tries to start a native drag.
		try {
			sourceEl.setPointerCapture(pointerId);
			captured = true;
		} catch {
			// Pointer may already be released; ignore.
		}

		// Stop iOS from stealing the captured pointer for scrolling now that the
		// drag is live. DEFERRED (2026-06-27) — touch-action set mid-gesture is
		// best-effort; if iOS still scrolls during a sidebar drag, the source
		// rows may need a static touch-action audit. Verify on a real iPad.
		if (isTouch) sourceEl.style.touchAction = "none";

		ghost = sourceEl.cloneNode(true) as HTMLElement;
		ghost.style.position = "fixed";
		ghost.style.pointerEvents = "none";
		ghost.style.zIndex = "10000";
		ghost.style.opacity = String(ghostOpacity);
		ghost.style.width = `${sourceEl.offsetWidth}px`;
		ghost.style.height = `${sourceEl.offsetHeight}px`;
		ghost.style.margin = "0";
		ghost.style.boxSizing = "border-box";
		ghost.style.left = `${clientX - offsetX}px`;
		ghost.style.top = `${clientY - offsetY}px`;
		document.body.appendChild(ghost);

		sourceEl.style.opacity = "0.35";
		callbacks.onStart?.();
	};

	const moveGhost = (clientX: number, clientY: number) => {
		if (ghost) {
			ghost.style.left = `${clientX - offsetX}px`;
			ghost.style.top = `${clientY - offsetY}px`;
		}
		callbacks.onMove(clientX, clientY);
	};

	const handleMove = (ev: PointerEvent) => {
		if (ev.pointerId !== pointerId) return;

		if (isTouch) {
			lastX = ev.clientX;
			lastY = ev.clientY;
			if (!started) {
				if (!longPressArmed) {
					// Moved before the hold completed → it's a scroll, not a drag.
					// Bail out WITHOUT preventDefault so the browser scrolls natively.
					const dx = ev.clientX - startX;
					const dy = ev.clientY - startY;
					if (Math.abs(dx) + Math.abs(dy) > touchSlop) cleanup();
					return;
				}
				// Hold completed and the finger is now moving → start the drag.
				ev.preventDefault();
				beginDrag(ev.clientX, ev.clientY);
			} else {
				ev.preventDefault();
			}
			moveGhost(ev.clientX, ev.clientY);
			return;
		}

		// ⚠️ DO NOT MOVE OR REMOVE THIS preventDefault, AND DO NOT GATE IT BEHIND
		// THE THRESHOLD. It must fire on EVERY pointermove, including sub-threshold
		// moves before the drag starts. On macOS WKWebView (dragDropEnabled=true)
		// it is what stops the native NSDragging session from starting; without it
		// the OS swallows pointerup and the drag ghost stays glued to the cursor.
		// This regressed once when it was "optimized" to fire only after the 5px
		// threshold — see mdkb: usemousedrag-preventdefault-load-bearing (2026-06-06).
		// Touch handles its own preventDefault above; this path is mouse/pen only.
		ev.preventDefault();
		if (!started) {
			const dx = ev.clientX - startX;
			const dy = ev.clientY - startY;
			if (Math.abs(dx) + Math.abs(dy) < threshold) return;
			beginDrag(ev.clientX, ev.clientY);
		}
		moveGhost(ev.clientX, ev.clientY);
	};

	const cleanup = () => {
		if (longPressTimer !== undefined) clearTimeout(longPressTimer);
		document.removeEventListener("pointermove", handleMove);
		document.removeEventListener("pointerup", handleUp);
		document.removeEventListener("pointercancel", handleCancel);
		document.removeEventListener("keydown", handleEsc);
		if (captured) {
			try {
				sourceEl.releasePointerCapture(pointerId);
			} catch {
				// Already released; ignore.
			}
		}
		if (isTouch) sourceEl.style.touchAction = prevTouchAction;
		if (ghost) ghost.remove();
		sourceEl.style.opacity = "";
	};

	const handleUp = (ev: PointerEvent) => {
		if (ev.pointerId !== pointerId) return;
		cleanup();
		if (started) {
			callbacks.onDrop(ev.clientX, ev.clientY);
		}
	};

	const handleCancel = (ev: PointerEvent) => {
		if (ev.pointerId !== pointerId) return;
		cleanup();
		if (started) callbacks.onCancel?.();
	};

	const handleEsc = (ev: KeyboardEvent) => {
		if (ev.key === "Escape") {
			cleanup();
			if (started) callbacks.onCancel?.();
		}
	};

	document.addEventListener("pointermove", handleMove);
	document.addEventListener("pointerup", handleUp);
	document.addEventListener("pointercancel", handleCancel);
	document.addEventListener("keydown", handleEsc);

	// Touch: arm the drag after a hold. If the finger is still within slop when
	// it fires, lift the item in place so the user sees drag mode immediately.
	if (isTouch) {
		longPressTimer = setTimeout(() => {
			longPressArmed = true;
			if (!started) beginDrag(lastX, lastY);
		}, longPressMs);
	}
}
