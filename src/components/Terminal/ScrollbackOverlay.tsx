import {
  Component,
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import type { ScrollbackCache } from "./scrollbackCache";
import { computeVisibleRange } from "./scrollbackVirtualize";
import {
  computeScrollTopForMatch,
  findSpanHighlightSegments,
} from "./scrollbackSearchUtils";
import { spanStyle, type LogLine, type LogSpan } from "../../mobile/utils/logLine";
import { ScrollbackScrollbar } from "./ScrollbackScrollbar";
import s from "./ScrollbackOverlay.module.css";

/** A single search match returned by the Rust `search_vt_log` command. */
export interface ScrollbackMatch {
  offset: number;
  col_start: number;
  col_end: number;
}

/**
 * Scrollback overlay that renders VtLogBuffer history above the live
 * xterm.js viewport. Mounts only while `visible === true`; uses
 * `computeVisibleRange` + the injected `ScrollbackCache` to keep memory
 * bounded (only ~1–2 viewports worth of lines are in the DOM at any time).
 *
 * The parent wires:
 *   - `cache`: a `ScrollbackCache` with the Tauri/HTTP fetcher plugged in
 *   - a total-growth listener that calls `cache.setTotal` when
 *     `pty-vt-log-total-{sessionId}` fires
 *   - `onReachBottom`: hides the overlay when the user scrolls down to
 *     the seam with xterm (i.e. the live screen is "below" the overlay)
 */
export interface ScrollbackOverlayProps {
  cache: ScrollbackCache;
  visible: boolean;
  /** Called when the user has scrolled to the bottom of the overlay.
   *  Parent should hide the overlay and hand focus back to xterm.js. */
  onReachBottom: () => void;
  /** Currently active search match — when set, the overlay scrolls to
   *  center the match's line and highlights the matching column range. */
  activeMatch?: ScrollbackMatch | null;
  /** Ref callback — exposes the scroll container so the parent can call
   *  scrollBy() for keyboard Page Up/Down navigation. */
  containerRef?: (el: HTMLDivElement) => void;
  /** Bumped by parent when terminal font size changes, triggering
   *  line-height re-measurement in the overlay. */
  fontVersion?: number;
}

/** Overscan rows above/below the visible window — smooths fast wheel. */
const OVERSCAN = 8;

/** Fallback line height if measurement fails (same as typical xterm). */
const FALLBACK_LINE_HEIGHT = 18;

/** How close to the bottom (in px) counts as "at the seam". */
const BOTTOM_THRESHOLD_PX = 4;

export const ScrollbackOverlay: Component<ScrollbackOverlayProps> = (props) => {
  const [total, setTotal] = createSignal(props.cache.total);
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewportHeight, setViewportHeight] = createSignal(0);
  const [lineHeight, setLineHeight] = createSignal(FALLBACK_LINE_HEIGHT);
  // Tick bumps on every `chunkLoaded` event so the `For` re-reads the cache.
  const [chunkTick, setChunkTick] = createSignal(0);

  let containerEl: HTMLDivElement | undefined;
  let measureEl: HTMLDivElement | undefined;

  // --- cache subscription ---
  // Debounce chunkLoaded → chunkTick via rAF to coalesce parallel chunk
  // arrivals into a single re-render (ensureLoaded fires 1-2 chunks in
  // parallel at chunk boundaries, each emitting chunkLoaded separately).
  let chunkTickRaf = 0;
  onMount(() => {
    const off = props.cache.subscribe((e) => {
      if (e.type === "total") {
        setTotal(e.total);
      } else if (e.type === "chunkLoaded") {
        if (!chunkTickRaf) {
          chunkTickRaf = requestAnimationFrame(() => {
            chunkTickRaf = 0;
            setChunkTick((t) => t + 1);
          });
        }
      }
    });
    onCleanup(() => {
      off();
      if (chunkTickRaf) cancelAnimationFrame(chunkTickRaf);
    });
  });

  // --- line-height measurement ---
  // Remeasures whenever `fontVersion` changes (parent bumps it on font size
  // change). Initial measurement happens on mount via the same effect.
  createEffect(() => {
    // Track fontVersion so the effect re-runs on font changes.
    void props.fontVersion;
    if (measureEl) {
      const rect = measureEl.getBoundingClientRect();
      if (rect.height > 0) setLineHeight(rect.height);
    }
  });

  // --- viewport height tracking via ResizeObserver ---
  // Keeps viewportHeight in sync when the terminal pane is resized while
  // the overlay is open. Initial value is set on mount.
  onMount(() => {
    if (containerEl) {
      setViewportHeight(containerEl.clientHeight);
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const h = entry.contentRect.height;
          if (h > 0) setViewportHeight(h);
        }
      });
      ro.observe(containerEl);
      onCleanup(() => ro.disconnect());
    }
  });

  // --- initial scroll position: start at the bottom on first visible ---
  // Skipped when an `activeMatch` is already set at open time — that effect
  // below wins and scrolls to the match instead.
  createEffect(() => {
    if (props.visible && containerEl && !props.activeMatch) {
      // Defer until the next frame so the height is applied.
      requestAnimationFrame(() => {
        if (containerEl) {
          containerEl.scrollTop = containerEl.scrollHeight;
          setScrollTop(containerEl.scrollTop);
        }
      });
    }
  });

  // --- scroll to active search match ---
  // Runs on every `activeMatch` change (including nav next/prev). The
  // overlay must be visible (parent guarantees this when driving search).
  createEffect(() => {
    const match = props.activeMatch;
    if (!match || !props.visible || !containerEl) return;
    // Ensure the line fetch is in-flight — the overlay may not have visited
    // this area yet. The chunk listener will bump `chunkTick` and re-render.
    void props.cache.ensureLoaded(match.offset, match.offset + 1);
    requestAnimationFrame(() => {
      if (!containerEl) return;
      const target = computeScrollTopForMatch({
        matchLineOffset: match.offset,
        lineHeight: lineHeight(),
        viewportHeight: containerEl.clientHeight,
        contentHeight: containerEl.scrollHeight,
      });
      containerEl.scrollTop = target;
      setScrollTop(containerEl.scrollTop);
    });
  });

  // --- scroll handler ---
  function handleScroll() {
    if (!containerEl) return;
    const top = containerEl.scrollTop;
    setScrollTop(top);
    // Detect reach-bottom: if the user scrolled (or wheeled) to the very
    // bottom of the virtualized area, hand focus back to xterm.js.
    const distanceFromBottom =
      containerEl.scrollHeight - top - containerEl.clientHeight;
    if (distanceFromBottom <= BOTTOM_THRESHOLD_PX) {
      props.onReachBottom();
    }
  }

  // --- wheel-down at bottom triggers close too (handles rubber-band) ---
  function handleWheel(ev: WheelEvent) {
    if (!containerEl) return;
    if (ev.deltaY <= 0) return; // up = keep scrolling history
    const distanceFromBottom =
      containerEl.scrollHeight -
      containerEl.scrollTop -
      containerEl.clientHeight;
    if (distanceFromBottom <= BOTTOM_THRESHOLD_PX) {
      props.onReachBottom();
    }
  }

  // --- visible window ---
  const range = createMemo(() =>
    computeVisibleRange({
      scrollTop: scrollTop(),
      viewportHeight: viewportHeight(),
      lineHeight: lineHeight(),
      totalLines: total(),
      overscan: OVERSCAN,
    }),
  );

  // --- progressive loading: fetch chunks covering the visible range ---
  createEffect(() => {
    const r = range();
    if (r.end <= r.start) return;
    // Fire-and-forget; the cache dedups concurrent requests.
    void props.cache.ensureLoaded(r.start, r.end);
  });

  // --- rendered indices array (depends on range + chunk loads) ---
  const indices = createMemo(() => {
    // Depend on chunkTick so re-renders happen after fetches resolve.
    chunkTick();
    const r = range();
    const out: number[] = [];
    for (let i = r.start; i < r.end; i++) out.push(i);
    return out;
  });

  // --- total scrollback height in CSS pixels ---
  const contentHeightPx = createMemo(() => total() * lineHeight());
  const maxScrollTop = createMemo(() => Math.max(0, contentHeightPx() - viewportHeight()));

  // --- spacer heights for flow-based virtualization ---
  // Instead of position: absolute on each line, we use a top spacer to push
  // visible lines to the correct scroll position. This enables native text
  // selection across multiple lines (absolute positioning breaks cross-element
  // drag selection in browsers).
  const topSpacerPx = createMemo(() => range().start * lineHeight());
  const bottomSpacerPx = createMemo(() =>
    Math.max(0, (total() - range().end) * lineHeight()),
  );

  return (
    <Show when={props.visible}>
      <div
        ref={(el) => {
          containerEl = el;
          props.containerRef?.(el);
        }}
        class={s.overlay}
        onScroll={handleScroll}
        onWheel={handleWheel}
        role="log"
        aria-label="Terminal scrollback"
      >
        {/* Hidden measurement reference for line-height */}
        <div ref={measureEl} class={s.measure} aria-hidden="true">
          M
        </div>
        <div class={s.content}>
          {/* Top spacer pushes visible lines to their scroll position */}
          <div style={{ height: `${topSpacerPx()}px` }} />
          <For each={indices()}>
            {(idx) => {
              const line = props.cache.getLine(idx);
              const match =
                props.activeMatch && props.activeMatch.offset === idx
                  ? props.activeMatch
                  : undefined;
              return (
                <div
                  class={s.line}
                  style={{ height: `${lineHeight()}px` }}
                >
                  {renderLine(line, match)}
                </div>
              );
            }}
          </For>
          {/* Bottom spacer maintains total scroll height */}
          <div style={{ height: `${bottomSpacerPx()}px` }} />
        </div>
      </div>
      <ScrollbackScrollbar
        scrollTop={scrollTop()}
        maxScrollTop={maxScrollTop()}
        viewportHeight={viewportHeight()}
        totalContentHeight={contentHeightPx()}
        onScrollTo={(st) => {
          if (containerEl) containerEl.scrollTop = st;
        }}
      />
    </Show>
  );
};

/** Render a single LogLine as styled spans, or a blank row if not yet loaded.
 *  When `match` is set and covers this line, the matching column range is
 *  wrapped in a highlighted `<span class={s.matchActive}>` while preserving
 *  the per-span ANSI styles from `spanStyle`. */
function renderLine(line: LogLine | undefined, match?: ScrollbackMatch) {
  if (!line) {
    return <span class={s.placeholder}>&nbsp;</span>;
  }
  if (!match) {
    return (
      <For each={line.spans}>
        {(span) => {
          const style = spanStyle(span);
          return style ? (
            <span style={style}>{span.text}</span>
          ) : (
            <span>{span.text}</span>
          );
        }}
      </For>
    );
  }
  const segments = findSpanHighlightSegments(
    line.spans,
    match.col_start,
    match.col_end,
  );
  return (
    <For each={segments}>
      {(seg) => {
        const span = line.spans[seg.spanIdx] as LogSpan | undefined;
        const style = span ? spanStyle(span) : undefined;
        const classList = seg.highlight ? { [s.matchActive]: true } : undefined;
        if (style && classList) {
          return (
            <span classList={classList} style={style}>
              {seg.text}
            </span>
          );
        }
        if (style) {
          return <span style={style}>{seg.text}</span>;
        }
        if (classList) {
          return <span classList={classList}>{seg.text}</span>;
        }
        return <span>{seg.text}</span>;
      }}
    </For>
  );
}
