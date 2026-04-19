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

      // Internal drag (tab/pane/sidebar move) — only handle hover highlight,
      // don't treat as file drop. The actual move happens via dragend fallback.
      if (isInternalDrag()) {
        if ((payload.type === "enter" || payload.type === "over") && payload.position) {
          const dpr = window.devicePixelRatio || 1;
          _lastDragCssPosition = { x: payload.position.x / dpr, y: payload.position.y / dpr };
          if (_pendingInternalDrag) updatePaneDropHover(payload.position.x, payload.position.y);
        } else if (payload.type === "drop" && payload.position) {
          const dpr = window.devicePixelRatio || 1;
          _lastDragCssPosition = { x: payload.position.x / dpr, y: payload.position.y / dpr };
          if (_pendingInternalDrag) clearPaneDropHover();
        } else if (payload.type === "leave") {
          if (_pendingInternalDrag) clearPaneDropHover();
        }
        return;
      }

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

// ---- Internal drag counter ----
// Tracks whether a drag originated inside the webview (tab reorder, sidebar,
// etc.) vs an OS file drop. Used by useFileDrop to suppress the overlay
// and by the Tauri event handler to skip file-drop processing.

let internalDragCount = 0;
export function markInternalDragStart(): void { internalDragCount++; }
export function markInternalDragEnd(): void { internalDragCount = Math.max(0, internalDragCount - 1); }
export function isInternalDrag(): boolean { return internalDragCount > 0; }

// ---- Internal drag tracking (Tauri dragDropEnabled=true workaround) ----
//
// When dragDropEnabled is true, Tauri intercepts ALL native drag events —
// including those originating within the webview (HTML5 DnD for tab reorder,
// cross-pane moves, sidebar drag). The HTML5 `drop` event may never fire on
// the target DOM element. We work around this by:
//   1. Storing the drag payload on dragstart (which always fires)
//   2. Using dragend (which always fires) to hit-test and perform the move
//   3. Using Tauri's "over" event for visual feedback on the target pane

export interface InternalDragPayload {
  tabId: string;
  fromGroupId: string | null;
  type: string;
}

let _pendingInternalDrag: InternalDragPayload | null = null;
let _internalDragHandled = false;
let _lastDragCssPosition: { x: number; y: number } | null = null;

export function setInternalDragPayload(payload: InternalDragPayload): void {
  _pendingInternalDrag = payload;
  _internalDragHandled = false;
}

export function markInternalDragHandled(): void {
  _internalDragHandled = true;
}

export function getInternalDragPayload(): InternalDragPayload | null {
  return _pendingInternalDrag;
}

export function wasInternalDragHandled(): boolean {
  return _internalDragHandled;
}

export function clearInternalDragState(): void {
  _pendingInternalDrag = null;
  _internalDragHandled = false;
  _lastDragCssPosition = null;
  clearPaneDropHover();
}

/** Last known CSS-pixel position during an internal drag (from Tauri events). */
export function getLastDragPosition(): { x: number; y: number } | null {
  return _lastDragCssPosition;
}

/** Element currently highlighted as a pane drop target during internal drags. */
let highlightedPaneEl: HTMLElement | null = null;
const PANE_DROP_HOVER_CLASS = "pane-drop-hover";

export function updatePaneDropHover(physicalX: number, physicalY: number): void {
  const dpr = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(physicalX / dpr, physicalY / dpr);
  let cur: Element | null = el;
  let target: HTMLElement | null = null;
  while (cur) {
    if ((cur as HTMLElement).dataset?.dropTarget === "pane") {
      target = cur as HTMLElement;
      break;
    }
    cur = cur.parentElement;
  }
  // Don't highlight the source pane
  if (target && _pendingInternalDrag?.fromGroupId === target.dataset.groupId) {
    target = null;
  }
  if (target === highlightedPaneEl) return;
  if (highlightedPaneEl) highlightedPaneEl.classList.remove(PANE_DROP_HOVER_CLASS);
  highlightedPaneEl = target;
  if (highlightedPaneEl) highlightedPaneEl.classList.add(PANE_DROP_HOVER_CLASS);
}

export function clearPaneDropHover(): void {
  if (highlightedPaneEl) {
    highlightedPaneEl.classList.remove(PANE_DROP_HOVER_CLASS);
    highlightedPaneEl = null;
  }
}

/** Find pane group ID at CSS pixel coordinates. */
export function findPaneGroupAtPoint(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y);
  let cur: Element | null = el;
  while (cur) {
    const dt = (cur as HTMLElement).dataset;
    if (dt?.dropTarget === "pane") return dt.groupId ?? null;
    cur = cur.parentElement;
  }
  return null;
}
