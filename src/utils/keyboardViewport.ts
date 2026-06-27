import { createSignal } from "solid-js";

/**
 * Bottom pixels occluded by the on-screen (virtual) keyboard, tracked via the
 * VisualViewport API. 0 when no keyboard is shown or the platform has no
 * VisualViewport (desktop).
 *
 * The layout viewport (`window.innerHeight`) does NOT shrink when the soft
 * keyboard opens, but the visual viewport does — so the occluded band is
 * `innerHeight - (visualViewport.offsetTop + visualViewport.height)`.
 *
 * Consumers (e.g. CanvasTerminal) read this to slide ONLY the focused terminal
 * up so the cursor stays visible above the keyboard — without resizing the app
 * layout or the PTY (which would trigger an expensive reflow/SIGWINCH).
 */
const [keyboardOcclusion, setKeyboardOcclusion] = createSignal(0);

export { keyboardOcclusion };

let installed = false;

/**
 * Install the single, shared VisualViewport listener (idempotent). Safe to call
 * from every terminal mount. No-op on platforms without VisualViewport.
 */
export function ensureKeyboardViewportTracking(): void {
	if (installed) return;
	installed = true;
	const vv = window.visualViewport;
	if (!vv) return;

	let raf = 0;
	const update = () => {
		cancelAnimationFrame(raf);
		raf = requestAnimationFrame(() => {
			const occ = Math.max(0, Math.round(window.innerHeight - (vv.offsetTop + vv.height)));
			setKeyboardOcclusion(occ);
		});
	};

	update();
	vv.addEventListener("resize", update);
	vv.addEventListener("scroll", update);
	// Never torn down: the listener is process-global and cheap; terminals come
	// and go but the keyboard state is shared across all of them.
}
