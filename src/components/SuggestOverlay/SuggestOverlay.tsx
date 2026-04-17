import { Component, For, createEffect, onCleanup, onMount } from "solid-js";
import styles from "./SuggestOverlay.module.css";

interface SuggestOverlayProps {
  items: string[];
  onSelect: (text: string) => void;
  onDismiss: () => void;
}

const DISMISS_TIMEOUT_MS = 30_000;
/** Preferred chip font size — matches --font-sm. Tried first. */
const MAX_CHIP_FONT_PX = 14;
/** Floor font size; below this chips become unreadable. */
const MIN_CHIP_FONT_PX = 11;

/** Strip leading "N) " or "N. " numbering that duplicates the shortcut badge. */
const stripNumberPrefix = (text: string): string => text.replace(/^\d+[).]\s*/, "");

const SuggestOverlay: Component<SuggestOverlayProps> = (props) => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let overlayEl: HTMLDivElement | undefined;
  let resizeObserver: ResizeObserver | undefined;

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onDismiss();
      return;
    }
    // Number keys 1-4 select the corresponding suggestion
    const num = parseInt(e.key, 10);
    if (num >= 1 && num <= 4 && num <= props.items.length) {
      e.preventDefault();
      props.onSelect(props.items[num - 1]);
      return;
    }
    // Any printable key (typing) dismisses the overlay
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      props.onDismiss();
    }
  };

  /** Shrink chip font-size uniformly until the single-row overlay (flex-wrap:
   *  nowrap) stops overflowing its max-width, or we hit MIN_CHIP_FONT_PX.
   *  Compares overlay scrollWidth (intrinsic row width) vs clientWidth
   *  (cap = viewport − margin), so overflow caused by the *sum* of chips —
   *  not just the widest one — triggers scaling. */
  function fitChips() {
    if (!overlayEl) return;
    // Reset to preferred size before measuring.
    overlayEl.style.setProperty("--chip-font-size", `${MAX_CHIP_FONT_PX}px`);

    const visible = overlayEl.clientWidth;
    const intrinsic = overlayEl.scrollWidth;
    if (visible <= 0 || intrinsic <= visible) return; // already fits

    // Proportional downscale. Text width dominates scrollWidth for long chip
    // rows, so scale roughly linearly with the visible/intrinsic ratio, then
    // clamp into [MIN, MAX] and round down.
    const estimated = Math.floor(MAX_CHIP_FONT_PX * (visible / intrinsic));
    const target = Math.max(MIN_CHIP_FONT_PX, Math.min(MAX_CHIP_FONT_PX, estimated));
    overlayEl.style.setProperty("--chip-font-size", `${target}px`);
  }

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown, true);
    timer = setTimeout(() => props.onDismiss(), DISMISS_TIMEOUT_MS);
    if (overlayEl && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => fitChips());
      resizeObserver.observe(overlayEl);
    }
  });

  // Re-measure whenever the items list changes.
  createEffect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    props.items; // track
    queueMicrotask(fitChips);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown, true);
    if (timer) clearTimeout(timer);
    if (resizeObserver) resizeObserver.disconnect();
  });

  return (
    <div class={styles.overlay} ref={overlayEl}>
      <For each={props.items}>
        {(item, index) => (
          <button class={styles.chip} onClick={() => props.onSelect(item)}>
            <span class={styles.shortcut} data-shortcut>
              {index() + 1}
            </span>
            {stripNumberPrefix(item)}
          </button>
        )}
      </For>
      <button class={styles.closeBtn} onClick={() => props.onDismiss()} title="Dismiss (Esc)">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
          <path d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"/>
        </svg>
      </button>
    </div>
  );
};

export default SuggestOverlay;
