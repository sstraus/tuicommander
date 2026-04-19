/**
 * Mouse-based drag utility — replaces HTML5 DnD which conflicts with
 * Tauri's dragDropEnabled=true on macOS (WKWebView intercepts all
 * NSDragging events, preventing HTML5 drop from firing on targets).
 *
 * Usage: call initMouseDrag() from a mousedown handler. It waits for
 * a movement threshold before starting the drag, creates a ghost clone,
 * and calls back with CSS-pixel coordinates on move/drop.
 */

export interface MouseDragCallbacks {
  onStart?: () => void;
  onMove: (x: number, y: number) => void;
  onDrop: (x: number, y: number) => void;
  onCancel?: () => void;
}

export function initMouseDrag(
  e: MouseEvent,
  sourceEl: HTMLElement,
  callbacks: MouseDragCallbacks,
  options?: { threshold?: number; ghostOpacity?: number },
): void {
  if (e.button !== 0) return;

  const threshold = options?.threshold ?? 5;
  const ghostOpacity = options?.ghostOpacity ?? 0.8;
  const startX = e.clientX;
  const startY = e.clientY;
  const rect = sourceEl.getBoundingClientRect();
  const offsetX = e.clientX - rect.left;
  const offsetY = e.clientY - rect.top;

  let started = false;
  let ghost: HTMLElement | null = null;

  const handleMove = (ev: MouseEvent) => {
    ev.preventDefault();
    if (!started) {
      const dx = ev.clientX - startX;
      const dy = ev.clientY - startY;
      if (Math.abs(dx) + Math.abs(dy) < threshold) return;
      started = true;

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
    document.removeEventListener("mousemove", handleMove);
    document.removeEventListener("mouseup", handleUp);
    document.removeEventListener("keydown", handleEsc);
    if (ghost) ghost.remove();
    sourceEl.style.opacity = "";
  };

  const handleUp = (ev: MouseEvent) => {
    cleanup();
    if (started) {
      callbacks.onDrop(ev.clientX, ev.clientY);
    }
  };

  const handleEsc = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") {
      cleanup();
      if (started) callbacks.onCancel?.();
    }
  };

  document.addEventListener("mousemove", handleMove);
  document.addEventListener("mouseup", handleUp);
  document.addEventListener("keydown", handleEsc);
}
