import { createSignal } from "solid-js";
import { appLogger } from "./appLogger";
import { isTauri } from "../transport";

/**
 * Absolute OS paths extracted from the most recent Tauri drag-drop event.
 * Populated via `onDragDropEvent` listener registered in initDragDrop().
 *
 * Rationale: with `dragDropEnabled: true` in tauri.conf.json, Tauri intercepts
 * OS-level file drops at the webview layer. HTML5 `File.path` is NOT populated
 * by the browser for security reasons — Tauri's dedicated event is the only
 * way to get absolute paths.
 */
export interface TauriDropPayload {
  paths: string[];
  /** Physical pixel position inside the webview. */
  position: { x: number; y: number };
}

const [dropPayload, setDropPayload] = createSignal<TauriDropPayload | null>(null);
const [isOverWindow, setIsOverWindow] = createSignal(false);
/** True while ⌥/Option (macOS) or Ctrl (Linux/Windows) is held — copy instead of move. */
const [copyModifierHeld, setCopyModifierHeld] = createSignal(false);

/** Element currently highlighted as a folder drop target (null when none). */
let highlightedEl: HTMLElement | null = null;
const DROP_HOVER_CLASS = "drop-target-hover";

function updateDropHover(physicalX: number, physicalY: number): void {
  const dpr = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(physicalX / dpr, physicalY / dpr);
  // Walk up to find a data-drop-target="folder"
  let cur: Element | null = el;
  let target: HTMLElement | null = null;
  while (cur) {
    const t = (cur as HTMLElement).dataset?.dropTarget;
    if (t === "folder") { target = cur as HTMLElement; break; }
    cur = cur.parentElement;
  }
  if (target === highlightedEl) return;
  if (highlightedEl) highlightedEl.classList.remove(DROP_HOVER_CLASS);
  highlightedEl = target;
  if (highlightedEl) highlightedEl.classList.add(DROP_HOVER_CLASS);
}

function clearDropHover(): void {
  if (highlightedEl) {
    highlightedEl.classList.remove(DROP_HOVER_CLASS);
    highlightedEl = null;
  }
}

export const dragDropStore = {
  /** Latest drop payload; consumed by the drop dispatcher. */
  dropPayload,
  /** Drag-entered the webview; UI can show "drop here" overlay. */
  isOverWindow,
  /** Whether the user is holding the copy modifier while dragging. */
  copyModifierHeld,
};

function isMac(): boolean {
  if (typeof navigator === "undefined") return false;
  return navigator.platform.toLowerCase().includes("mac");
}

/** Update modifier state from a keyboard event. */
function updateModifierFromEvent(e: KeyboardEvent) {
  setCopyModifierHeld(isMac() ? e.altKey : e.ctrlKey);
}

/**
 * Register listeners for:
 *  - Tauri `onDragDropEvent` (absolute paths + position)
 *  - Global keydown/keyup (track Alt/Ctrl for copy vs move)
 *
 * Idempotent: calling twice is safe (no-op after first init).
 */
let initialised = false;
export async function initDragDrop(): Promise<void> {
  if (initialised) return;
  initialised = true;

  // Modifier tracking works in both browser and Tauri contexts.
  document.addEventListener("keydown", updateModifierFromEvent);
  document.addEventListener("keyup", updateModifierFromEvent);
  // Clear on blur so the flag doesn't get stuck if the user releases outside.
  window.addEventListener("blur", () => setCopyModifierHeld(false));

  if (!isTauri()) return;

  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    const webview = getCurrentWebview();
    await webview.onDragDropEvent((event) => {
      const payload = event.payload;
      // Tauri v2 payload shapes: { type: "enter"|"over"|"drop"|"leave", paths?, position? }
      if (payload.type === "enter" || payload.type === "over") {
        setIsOverWindow(true);
        if (payload.position) updateDropHover(payload.position.x, payload.position.y);
      } else if (payload.type === "leave") {
        setIsOverWindow(false);
        clearDropHover();
      } else if (payload.type === "drop") {
        setIsOverWindow(false);
        clearDropHover();
        setDropPayload({
          paths: payload.paths ?? [],
          position: payload.position ?? { x: 0, y: 0 },
        });
      }
    });
  } catch (err) {
    appLogger.warn("app", "Failed to register Tauri drag-drop listener", err);
  }
}

/** Clear the stored payload after it has been processed by a drop handler. */
export function clearDropPayload(): void {
  setDropPayload(null);
}
