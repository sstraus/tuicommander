import type { Terminal } from "@xterm/xterm";
import { isSuggestBlock, continuationRowsAfterSuggest } from "./suggestOverlay";

/** Regex to identify suggest rows in xterm buffer text (column-0 anchor) */
export const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
/** Regex to identify intent rows in xterm buffer text */
export const INTENT_RE = /^[\s●⏺]*intent:\s+/;

/** Observe xterm renders and cover suggest/intent rows with CSS overlays.
 *  Unlike decorations (which attach to buffer markers and break when Ink
 *  redraws the same lines), this scans visible rows on every render pass
 *  and positions absolute divs over matching rows. Zero timing hacks. */
export function installRenderObserver(
  term: Terminal,
  container: HTMLElement,
): () => void {
  // Overlay container — must be inside .xterm-screen to sit above WebGL canvases.
  // The canvases are direct children of .xterm-screen, so appending after them
  // puts the overlay on top via natural stacking order + z-index.
  const screen = container.querySelector<HTMLElement>(".xterm-screen");
  if (!screen) return () => {};
  const overlay = document.createElement("div");
  overlay.style.cssText = "position:absolute;top:0;left:0;right:0;bottom:0;pointer-events:none;z-index:10;overflow:hidden";
  screen.appendChild(overlay);

  // Cache the last html string so we can skip innerHTML assignment when
  // content hasn't changed. xterm's onRender fires on every write and every
  // scroll frame; without this cache, every render rebuilds the overlay DOM
  // even when no suggest/intent rows are visible, adding visible jank to
  // wheel scrolls on busy terminals.
  let lastHtml = "<uninitialized>";

  const scan = () => {
    const buf = term.buffer.active;
    const cellH = (term as any)._core?._renderService?.dimensions?.css?.cell?.height;
    if (!cellH) return;

    // Compute top offset: viewport rows start at viewportY in the buffer.
    // The first visible row is drawn at y=0 in the canvas.
    const viewportY = buf.viewportY;
    const rows = term.rows;
    const bg = term.options.theme?.background ?? "#1e1e1e";

    let html = "";
    for (let row = 0; row < rows; row++) {
      const line = buf.getLine(viewportY + row);
      if (!line) continue;
      const text = line.translateToString(true);

      const getRowSnapshot = (i: number) => {
        const ln = buf.getLine(viewportY + i);
        if (!ln) return null;
        return { text: ln.translateToString(true), isWrapped: ln.isWrapped };
      };
      if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, rows, getRowSnapshot)) {
        const top = row * cellH;
        html += `<div style="position:absolute;left:0;right:0;top:${top}px;height:${cellH}px;background:${bg}"></div>`;
        // Delegate the bounded continuation scan to the pure helper so
        // Makefile/table/diff rows that merely contain `|` aren't swallowed
        // by an unbounded pipe-tail consumer (story 1276-a3c2).
        const hiddenRows = continuationRowsAfterSuggest(row, rows, getRowSnapshot);
        for (const contRow of hiddenRows) {
          html += `<div style="position:absolute;left:0;right:0;top:${contRow * cellH}px;height:${cellH}px;background:${bg}"></div>`;
        }
        // Skip covered rows so the outer loop doesn't re-process them
        if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
      } else if (INTENT_RE.test(text)) {
        const top = row * cellH;
        html += `<div style="position:absolute;left:0;right:0;top:${top}px;height:${cellH}px;background:rgba(181,147,90,0.12)"></div>`;
      }
    }
    if (html !== lastHtml) {
      overlay.innerHTML = html;
      lastHtml = html;
    }
  };

  // onRender covers writes; onScroll covers scrollback navigation.
  // Without the onScroll hook, when the user scrolls up through scrollback,
  // xterm redraws cells but does NOT fire onRender — the overlay rectangles
  // stay pinned at their old viewport-relative positions and visually
  // "stick" to the viewport top while content slides beneath them.
  const renderDisposable = term.onRender(scan);
  const scrollDisposable = term.onScroll(scan);
  // Initial scan
  scan();

  // Debug: expose a function to dump buffer for the dev console
  (window as any).__tuicDebugBuffer = () => {
    const buf = term.buffer.active;
    const lines: string[] = [];
    for (let row = 0; row < term.rows; row++) {
      const line = buf.getLine(buf.viewportY + row);
      if (line) lines.push(line.translateToString(true));
    }
    return {
      rows: term.rows,
      viewportY: buf.viewportY,
      intentLines: lines.map((l, i) => ({ i, l })).filter(x => x.l.includes("intent")),
      suggestLines: lines.map((l, i) => ({ i, l })).filter(x => x.l.includes("suggest")),
      allLines: lines,
    };
  };

  return () => {
    renderDisposable.dispose();
    scrollDisposable.dispose();
    overlay.remove();
  };
}
