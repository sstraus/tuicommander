import type { Terminal } from "@xterm/xterm";

/** Keep xterm's vertical scrollbar visible whenever scrollback exists.
 *
 *  xterm v6 hard-codes `ScrollbarVisibility.Auto`. The class transitions are:
 *    !_isNeeded                          → "invisible scrollbar vertical"
 *    _isNeeded && !_shouldBeVisible      → "invisible scrollbar vertical fade"
 *    _isNeeded && _shouldBeVisible       → "visible scrollbar vertical"
 *
 *  CRITICAL: `_hide(e)` early-returns when `_isVisible === false`, so a
 *  terminal that has never been interacted with stays in the plain
 *  "invisible scrollbar vertical" state (no fade) even when scrollback
 *  accumulates — xterm only reaches the fade branch after the first reveal.
 *
 *  A CSS-only override on `.scrollbar.fade` therefore misses the most
 *  common case: an agent streaming output into a tab the user hasn't
 *  hovered yet. Source overflow truth from the xterm buffer model itself
 *  (`buffer.active.length > term.rows`) rather than from the slider DOM:
 *  the WebGL renderer mutates slider geometry asynchronously and reading
 *  inline `style.height` can miss real overflow. Force the scrollbar
 *  visible via inline style — inline styles beat xterm's class-based rules
 *  without fighting over className.
 *
 *  Remove this workaround when xterm exposes a `ScrollbarVisibility.Visible`
 *  option or fixes the auto-hide logic for never-interacted terminals. */
export function installScrollbarVisibilityFix(
  term: Terminal,
  container: HTMLElement,
): () => void {
  const scrollbar = container.querySelector<HTMLElement>(
    ".xterm-scrollable-element > .scrollbar.vertical",
  );
  if (!scrollbar) return () => {};

  const update = () => {
    const hasOverflow = term.buffer.active.length > term.rows;
    if (hasOverflow) {
      scrollbar.style.setProperty("opacity", "1", "important");
      scrollbar.style.setProperty("pointer-events", "auto", "important");
    } else {
      scrollbar.style.removeProperty("opacity");
      scrollbar.style.removeProperty("pointer-events");
    }
  };

  // Buffer-level events cover every path that can add/remove scrollback:
  // writes (onLineFeed), scrollback navigation (onScroll), and resize
  // (onResize shrinks/grows term.rows). The MutationObserver is still
  // needed because xterm re-applies its class-based styles on every render
  // cycle, so our inline override must be re-asserted if xterm mutates it.
  const lf = term.onLineFeed(update);
  const sc = term.onScroll(update);
  const rs = term.onResize(update);
  const observer = new MutationObserver(update);
  observer.observe(scrollbar, {
    attributes: true,
    attributeFilter: ["class", "style"],
  });
  update();

  return () => {
    lf.dispose();
    sc.dispose();
    rs.dispose();
    observer.disconnect();
  };
}
