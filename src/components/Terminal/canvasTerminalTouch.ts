export interface TouchHandlerOptions {
  onScroll: (deltaY: number) => void;
  onInput: (data: string) => void;
  onFocus: () => void;
  onFontSizeChange: (delta: number) => void;
  onSelectionMode: () => void;
}

const LONG_PRESS_MS = 600;
const MOVE_CANCEL_PX = 10;
const PINCH_SCALE_FACTOR = 0.1;

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

  function cancelLongPress() {
    if (longPressTimer !== null) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function onTouchStart(e: TouchEvent) {
    if (e.touches.length === 1) {
      startY = e.touches[0].clientY;
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
      const dy = e.touches[0].clientY - startY;
      if (Math.abs(dy) > MOVE_CANCEL_PX) {
        cancelLongPress();
      }
      opts.onScroll(dy);
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
      // Tap: focus textarea to trigger virtual keyboard
      textarea.focus();
      opts.onFocus();
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
  canvas.addEventListener("touchmove", onTouchMove, { passive: true });
  canvas.addEventListener("touchend", onTouchEnd);
  textarea.addEventListener("input", onInput);

  return () => {
    canvas.removeEventListener("touchstart", onTouchStart);
    canvas.removeEventListener("touchmove", onTouchMove);
    canvas.removeEventListener("touchend", onTouchEnd);
    textarea.removeEventListener("input", onInput);
    cancelLongPress();
  };
}
