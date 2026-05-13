/**
 * Forward TUIC keyboard shortcuts from same-origin iframes to the parent document.
 *
 * When an iframe has focus, keydown events fire in the iframe's document and
 * never reach the parent's keyboard shortcut handler. This utility attaches a
 * listener to the iframe's contentWindow that re-dispatches keydowns on the
 * parent document — but ONLY for combos that are registered TUIC shortcuts.
 * Standard browser shortcuts (Cmd+C/V/Z/X/A etc.) are left alone.
 */

import { eventToCombo } from "../hooks/useKeyboardShortcuts";
import { keybindingsStore } from "../stores/keybindings";

const MODIFIER_ONLY = new Set(["Control", "Meta", "Alt", "Shift"]);

/**
 * Attach a key-forwarding listener to a same-origin iframe.
 * Returns a cleanup function, or `undefined` if the iframe is cross-origin.
 */
export function attachIframeKeyForwarder(iframe: HTMLIFrameElement): (() => void) | undefined {
	try {
		const cw = iframe.contentWindow;
		if (!cw || typeof cw.addEventListener !== "function") return undefined;
		void iframe.contentDocument;

		const handler = (e: KeyboardEvent) => {
			if (!e.metaKey && !e.ctrlKey && !e.altKey) return;
			if (MODIFIER_ONLY.has(e.key)) return;

			const combo = eventToCombo(e);
			if (!combo) return;

			// Only intercept if this combo is a registered TUIC shortcut
			if (!keybindingsStore.getActionForCombo(combo)) return;

			e.preventDefault();
			e.stopPropagation();

			const synth = new KeyboardEvent(e.type, {
				key: e.key,
				code: e.code,
				keyCode: e.keyCode,
				ctrlKey: e.ctrlKey,
				metaKey: e.metaKey,
				altKey: e.altKey,
				shiftKey: e.shiftKey,
				bubbles: true,
				cancelable: true,
			});
			document.dispatchEvent(synth);
		};

		cw.addEventListener("keydown", handler);
		return () => {
			try {
				cw.removeEventListener("keydown", handler);
			} catch {
				// iframe already detached
			}
		};
	} catch {
		return undefined;
	}
}
