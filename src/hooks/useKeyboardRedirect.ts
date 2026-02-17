import { createEffect, onCleanup } from "solid-js";
import { terminalsStore } from "../stores/terminals";

/** Keys that should NOT be redirected */
const EXCLUDED_KEYS = new Set([
  "Tab",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Enter",
  "Escape",
  "F1",
  "F2",
  "F3",
  "F4",
  "F5",
  "F6",
  "F7",
  "F8",
  "F9",
  "F10",
  "F11",
  "F12",
]);

/** Elements that should be allowed to receive keyboard input */
const INPUT_ELEMENTS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

/**
 * Check if an element is inside a terminal pane
 */
function isInsideTerminal(element: Element | null): boolean {
  while (element) {
    if (element.classList?.contains("terminal-pane")) {
      return true;
    }
    if (element.classList?.contains("xterm")) {
      return true;
    }
    element = element.parentElement;
  }
  return false;
}

/**
 * Check if the key should be redirected
 */
function shouldRedirect(e: KeyboardEvent): boolean {
  // Don't redirect if modifier keys are pressed (except shift for uppercase)
  if (e.ctrlKey || e.metaKey || e.altKey) {
    return false;
  }

  // Don't redirect excluded keys
  if (EXCLUDED_KEYS.has(e.key)) {
    return false;
  }

  // Only redirect printable characters and some control keys
  // Printable: single character keys, space, backspace, delete
  if (e.key.length === 1) {
    return true;
  }

  if (e.key === "Backspace" || e.key === "Delete") {
    return true;
  }

  return false;
}

/**
 * Hook that redirects keyboard input to the active terminal
 * when focus is outside the terminal pane.
 *
 * @param autoFocus - If true, automatically focuses the terminal on first keystroke
 */
export function useKeyboardRedirect(autoFocus: boolean = true) {
  createEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      // Get active element
      const activeElement = document.activeElement;

      // Don't redirect if focus is in an input element
      if (activeElement && INPUT_ELEMENTS.has(activeElement.tagName)) {
        return;
      }

      // Don't redirect if focus is inside a terminal
      if (isInsideTerminal(activeElement)) {
        return;
      }

      // Check if this key should be redirected
      if (!shouldRedirect(e)) {
        return;
      }

      // Get active terminal
      const activeTerminal = terminalsStore.getActive();
      if (!activeTerminal?.ref) {
        return;
      }

      // Prevent default to avoid duplicate input
      e.preventDefault();

      // Focus terminal if auto-focus is enabled
      if (autoFocus) {
        activeTerminal.ref.focus();
      }

      // Write the key to the terminal
      if (e.key === "Backspace") {
        activeTerminal.ref.write("\x7f"); // DEL character
      } else if (e.key === "Delete") {
        activeTerminal.ref.write("\x1b[3~"); // Delete key escape sequence
      } else {
        activeTerminal.ref.write(e.key);
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });
}
