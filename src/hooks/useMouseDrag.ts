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
 * Usage: call initMouseDrag() from a pointerdown handler. It waits for a
 * movement threshold before starting the drag, creates a ghost clone, and
 * calls back with CSS-pixel coordinates on move/drop.
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
	options?: { threshold?: number; ghostOpacity?: number },
): void {
	if (e.button !== 0) return;

	const threshold = options?.threshold ?? 5;
	const ghostOpacity = options?.ghostOpacity ?? 0.8;
	const startX = e.clientX;
	const startY = e.clientY;
	const pointerId = e.pointerId;
	const rect = sourceEl.getBoundingClientRect();
	const offsetX = e.clientX - rect.left;
	const offsetY = e.clientY - rect.top;

	let started = false;
	let captured = false;
	let ghost: HTMLElement | null = null;

	const handleMove = (ev: PointerEvent) => {
		if (ev.pointerId !== pointerId) return;
		// ⚠️ DO NOT MOVE OR REMOVE THIS preventDefault, AND DO NOT GATE IT BEHIND
		// THE THRESHOLD. It must fire on EVERY pointermove, including sub-threshold
		// moves before the drag starts. On macOS WKWebView (dragDropEnabled=true)
		// it is what stops the native NSDragging session from starting; without it
		// the OS swallows pointerup and the drag ghost stays glued to the cursor.
		// This regressed once when it was "optimized" to fire only after the 5px
		// threshold — see mdkb: usemousedrag-preventdefault-load-bearing (2026-06-06).
		ev.preventDefault();
		if (!started) {
			const dx = ev.clientX - startX;
			const dy = ev.clientY - startY;
			if (Math.abs(dx) + Math.abs(dy) < threshold) return;
			started = true;

			// Capture only now (real drag, not a click): guarantees pointerup
			// reaches us even if WKWebView tries to start a native drag.
			try {
				sourceEl.setPointerCapture(pointerId);
				captured = true;
			} catch {
				// Pointer may already be released; ignore.
			}

			ghost = sourceEl.cloneNode(true) as HTMLElement;
			ghost.style.position = "fixed";
			ghost.style.pointerEvents = "none";
			ghost.style.zIndex = "10000";
			ghost.style.opacity = String(ghostOpacity);
			ghost.style.width = `${sourceEl.offsetWidth}px`;
			ghost.style.height = `${sourceEl.offsetHeight}px`;
			ghost.style.margin = "0";
			ghost.style.boxSizing = "border-box";
			ghost.style.left = `${ev.clientX - offsetX}px`;
			ghost.style.top = `${ev.clientY - offsetY}px`;
			document.body.appendChild(ghost);

			sourceEl.style.opacity = "0.35";
			callbacks.onStart?.();
		}

		ghost!.style.left = `${ev.clientX - offsetX}px`;
		ghost!.style.top = `${ev.clientY - offsetY}px`;
		callbacks.onMove(ev.clientX, ev.clientY);
	};

	const cleanup = () => {
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
}
