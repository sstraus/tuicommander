import { onCleanup } from "solid-js";
import { terminalsStore } from "../stores/terminals";

/**
 * Centralized terminal focus restoration.
 *
 * When a dialog/overlay rendered with <Show> closes, the browser removes its
 * DOM nodes and focus falls to document.body. This hook detects that pattern
 * and restores focus to the active terminal — no per-component patches needed.
 *
 * Only restores when the previously focused element was removed from the DOM
 * (dialog closed), not when the user clicks empty space or tabs away.
 */
export function useFocusRestore(): void {
  let prevFocused: EventTarget | null = null;

  const onFocusOut = (e: FocusEvent) => {
    prevFocused = e.target;
  };

  const onFocusIn = (e: FocusEvent) => {
    if (
      e.target !== document.body
      || !prevFocused
      || !(prevFocused instanceof Node)
    ) {
      return;
    }

    // The previously focused element was removed from the DOM — a dialog closed.
    if (!document.contains(prevFocused)) {
      requestAnimationFrame(() => {
        // Double-check: still on body (no other handler claimed focus)
        if (document.activeElement === document.body || document.activeElement === null) {
          terminalsStore.getActive()?.ref?.focus();
        }
      });
    }
  };

  document.addEventListener("focusout", onFocusOut);
  document.addEventListener("focusin", onFocusIn);

  onCleanup(() => {
    document.removeEventListener("focusout", onFocusOut);
    document.removeEventListener("focusin", onFocusIn);
  });
}
