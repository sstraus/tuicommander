import { Component, createMemo, createSignal, onCleanup } from "solid-js";
import s from "./ScrollbackScrollbar.module.css";

const MIN_THUMB_HEIGHT = 30; // px

export interface ScrollbackScrollbarProps {
  scrollTop: number;
  maxScrollTop: number;
  viewportHeight: number;
  totalContentHeight: number;
  onScrollTo: (scrollTop: number) => void;
}

/** Compute the thumb height in pixels. */
export function computeThumbHeight(
  viewportHeight: number,
  totalContentHeight: number,
  trackHeight: number,
): number {
  if (totalContentHeight <= viewportHeight) return trackHeight;
  const ratio = viewportHeight / totalContentHeight;
  return Math.max(MIN_THUMB_HEIGHT, Math.round(ratio * trackHeight));
}

/** Compute the thumb top position in pixels. */
export function computeThumbTop(
  scrollTop: number,
  maxScrollTop: number,
  trackHeight: number,
  thumbHeight: number,
): number {
  if (maxScrollTop <= 0) return 0;
  const ratio = scrollTop / maxScrollTop;
  return Math.round(ratio * (trackHeight - thumbHeight));
}

/** Compute the scrollTop from a click position on the track. */
function computeScrollTopFromTrackClick(
  clickY: number,
  trackHeight: number,
  thumbHeight: number,
  maxScrollTop: number,
): number {
  const thumbTop = clickY - thumbHeight / 2;
  const maxThumbTop = trackHeight - thumbHeight;
  const clamped = Math.max(0, Math.min(thumbTop, maxThumbTop));
  if (maxThumbTop <= 0) return 0;
  return Math.round((clamped / maxThumbTop) * maxScrollTop);
}

export const ScrollbackScrollbar: Component<ScrollbackScrollbarProps> = (props) => {
  let trackEl: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal(false);

  // Drag state captured on mousedown
  let dragStartY = 0;
  let dragStartScrollTop = 0;

  const trackHeight = createMemo(() => props.viewportHeight);

  const thumbHeight = createMemo(() =>
    computeThumbHeight(props.viewportHeight, props.totalContentHeight, trackHeight()),
  );

  const thumbTop = createMemo(() =>
    computeThumbTop(props.scrollTop, props.maxScrollTop, trackHeight(), thumbHeight()),
  );

  const visible = createMemo(() =>
    props.totalContentHeight > props.viewportHeight && props.viewportHeight > 0,
  );

  const handleTrackClick = (e: MouseEvent) => {
    if (!trackEl) return;
    // Ignore clicks on the thumb itself
    if ((e.target as HTMLElement).classList.contains(s.thumb)) return;
    const rect = trackEl.getBoundingClientRect();
    const clickY = e.clientY - rect.top;
    const newScrollTop = computeScrollTopFromTrackClick(
      clickY,
      trackHeight(),
      thumbHeight(),
      props.maxScrollTop,
    );
    props.onScrollTo(newScrollTop);
  };

  const handleThumbMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragging(true);
    dragStartY = e.clientY;
    dragStartScrollTop = props.scrollTop;
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  };

  const handleMouseMove = (e: MouseEvent) => {
    const deltaY = e.clientY - dragStartY;
    const maxThumbTop = trackHeight() - thumbHeight();
    if (maxThumbTop <= 0) return;
    // Convert pixel drag delta to scrollTop delta
    const scrollDelta = (deltaY / maxThumbTop) * props.maxScrollTop;
    const newScrollTop = Math.max(0, Math.min(
      dragStartScrollTop + scrollDelta,
      props.maxScrollTop,
    ));
    props.onScrollTo(newScrollTop);
  };

  const handleMouseUp = () => {
    setDragging(false);
    document.removeEventListener("mousemove", handleMouseMove);
    document.removeEventListener("mouseup", handleMouseUp);
  };

  onCleanup(() => {
    document.removeEventListener("mousemove", handleMouseMove);
    document.removeEventListener("mouseup", handleMouseUp);
  });

  return (
    <div
      class={s.scrollbar}
      style={{ display: visible() ? "block" : "none" }}
    >
      <div
        ref={trackEl}
        class={s.track}
        onMouseDown={handleTrackClick}
      >
        <div
          class={s.thumb}
          classList={{ [s.thumbDragging]: dragging() }}
          style={{
            top: `${thumbTop()}px`,
            height: `${thumbHeight()}px`,
          }}
          onMouseDown={handleThumbMouseDown}
        />
      </div>
    </div>
  );
};
