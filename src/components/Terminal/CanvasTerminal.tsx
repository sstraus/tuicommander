import { Component, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { settingsStore } from "../../stores/settings";
import {
  decodeBinaryFrame,
  computeCursorRect,
  snapLineHeight,
  ATTR_BOLD,
  ATTR_ITALIC,
  ATTR_UNDERLINE,
  ATTR_STRIKEOUT,
  ATTR_DIM,
  ATTR_INVERSE,
  ATTR_DEFAULT_FG,
  ATTR_DEFAULT_BG,
  type CellMetrics,
  type CursorShape,
  type DecodedFrame,
} from "./canvasTerminalUtils";
import {
  getSharedMetrics,
  acquireCache,
  releaseCache,
  invalidateGlyphCache,
} from "./glyphCache";
import { keyToSequence, altSequenceFromCode } from "./terminalInput";
import { kittySequenceForKey } from "./kittyKeyboard";
import { isSuggestBlock, continuationRowsAfterSuggest } from "./suggestOverlay";
import {
  filePathRegex,
  fileUrlRegex,
} from "./linkProvider";
import { handleOpenUrl } from "../../utils/openUrl";
import { terminalsStore } from "../../stores/terminals";
import { isMacOS, isWindows } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { createTransport, type TerminalTransport } from "./canvasTerminalTransport";
import { installTouchHandlers } from "./canvasTerminalTouch";
// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame };

export interface CanvasTerminalRef {
  focus: () => void;
  refresh: () => void;
  getSelectionText: () => string;
  searchFind: (query: string) => Promise<{ index: number; count: number }>;
  searchNext: () => { index: number; count: number };
  searchPrev: () => { index: number; count: number };
  searchClear: () => void;
}

export interface CanvasTerminalProps {
  sessionId: string;
  terminalId: string;
  onOpenFilePath?: (path: string, line?: number, col?: number) => void;
  onSearchOpen?: () => void;
  onSearchClose?: () => void;
  searchVisible?: boolean;
  onResume?: () => void;
  onResumeDismiss?: () => void;
  hasPendingResume?: boolean;
  onCwdChange?: (id: string, cwd: string) => void;
  onFocus?: () => void;
  onRef?: (ref: CanvasTerminalRef) => void;
  onBell?: () => void;
}

const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
const INTENT_RE = /^[\s●⏺]*intent:\s+/;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
  let canvasRef!: HTMLCanvasElement;
  let overlayCanvasRef!: HTMLCanvasElement;
  let touchTextareaRef!: HTMLTextAreaElement;
  let scrollbarRef!: HTMLDivElement;
  let scrollThumbRef!: HTMLDivElement;
  let overlayRef!: HTMLDivElement;
  let containerRef!: HTMLDivElement;
  let ctx!: CanvasRenderingContext2D;
  let octx!: CanvasRenderingContext2D;

  const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
  const [focused, setFocused] = createSignal(false);
  let currentFrame: DecodedFrame | null = null;
  let lastDisplayOffset = -1;
  let lastScreenRows = -1;
  let lastScreenCols = -1;
  let searchMatches: { row: number; col_start: number; col_end: number }[] = [];
  let activeSearchIndex = -1;
  let cursorBlinkOn = true;
  let blinkInterval: ReturnType<typeof setInterval> | undefined;
  let unsubscribe: (() => void) | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let visibilityObserver: IntersectionObserver | undefined;
  let lastResizeCols = 0;
  let lastResizeRows = 0;
  let transport: TerminalTransport | undefined;
  let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;
  let rafId: number | undefined;
  let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
  let dprMediaQuery: MediaQueryList | undefined;
  let dprChangeHandler: (() => void) | undefined;
  let cleanupTouch: (() => void) | undefined;
  let alive = true;
  let linkCheckGeneration = 0;
  const ipcErr = (cmd: string) => (e: unknown) => appLogger.debug("terminal", `${cmd} failed`, { sessionId: props.sessionId, error: e });

  // Selection state — row coordinates are absolute (viewportTop + viewportRow)
  // so the highlight stays anchored to the original content when scrolling.
  let selecting = false;
  let selectionStart: { col: number; row: number } | null = null;
  let selectionEnd: { col: number; row: number } | null = null;
  let cachedSelectionText = "";

  // Link detection
  const linkCache = new Map<string, { text: string; path: string; line?: number; col?: number; index: number }[] | null>();
  let hoveredLink: { row: number; colStart: number; colEnd: number; path: string; line?: number; col?: number } | null = null;

  // Cached CSS custom properties (re-read on remeasure, not every frame)
  let cachedBgDefault = "#1e1e1e";
  let cachedFgDefault = "#d4d4d4";

  // Row index → row data lookup (persistent, updated incrementally)
  let rowMap = new Map<number, DecodedFrame["rows"][0]>();
  // Rows that arrived in the latest onFrame batch (drives incremental repaint)
  let pendingDirtyRows = new Set<number>();
  // When true, next paint must redraw everything (scroll, resize, clear)
  let fullRepaintNeeded = true;
  let hidden = false;
  let lastHistorySize = -1;


  // Memoized color strings: pack RGB into u32 key → "rgb(r,g,b)" string
  const colorStringCache = new Map<number, string>();
  function cachedRgbString(r: number, g: number, b: number): string {
    const key = (r << 16) | (g << 8) | b;
    let s = colorStringCache.get(key);
    if (s === undefined) {
      s = `rgb(${r},${g},${b})`;
      colorStringCache.set(key, s);
    }
    return s;
  }

  // Memoized font style strings: pack attrs into a key → font string
  const fontStyleCache = new Map<string, string>();
  function cachedFontStyle(italic: boolean, bold: boolean, fontSize: number, fontFamily: string): string {
    const weight = bold ? "bold" : settingsStore.state.fontWeight;
    const key = `${italic ? "i" : ""}${weight}${fontSize}${fontFamily}`;
    let s = fontStyleCache.get(key);
    if (s === undefined) {
      s = `${italic ? "italic " : ""}${weight} ${fontSize}px ${fontFamily}`;
      fontStyleCache.set(key, s);
    }
    return s;
  }

  function writePty(data: string) {
    if (currentFrame && currentFrame.displayOffset > 0) {
      invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -(currentFrame.displayOffset) }).catch(ipcErr("terminal_scroll"));
    }
    invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch((e) => {
      appLogger.warn("terminal", "PTY write failed", { sessionId: props.sessionId, error: e });
    });
  }

  function scheduleRepaint() {
    if (rafId !== undefined || hidden || !alive) return;
    rafId = requestAnimationFrame(() => {
      rafId = undefined;
      if (!alive || hidden) return;
      const m = metrics();
      if (currentFrame && m) {
        const dirty = pendingDirtyRows.size > 0 ? new Set(pendingDirtyRows) : undefined;
        pendingDirtyRows.clear();
        paintFrame(currentFrame, m, dirty);
      }
    });
  }

  function canvasToGrid(e: MouseEvent): { col: number; row: number } {
    const m = metrics();
    if (!m) return { col: 0, row: 0 };
    const rect = canvasRef.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const maxCol = Math.max(0, Math.floor(rect.width / m.cellWidth) - 1);
    const maxRow = Math.max(0, Math.floor(rect.height / m.cellHeight) - 1);
    return {
      col: Math.max(0, Math.min(Math.floor(x / m.cellWidth), maxCol)),
      row: Math.max(0, Math.min(Math.floor(y / m.cellHeight), maxRow)),
    };
  }

  function sgrMouseSequence(button: number, col: number, row: number, press: boolean): string {
    return `\x1b[<${button};${col + 1};${row + 1}${press ? "M" : "m"}`;
  }

  function viewportRowToAbs(viewportRow: number): number | null {
    if (!currentFrame) return null;
    return (currentFrame.historySize - currentFrame.displayOffset) + viewportRow;
  }

  function remeasure() {
    if (!ctx) return;
    const rect = containerRef.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;

    const dpr = window.devicePixelRatio || 1;
    const perTerminalSize = terminalsStore.state.terminals[props.terminalId]?.fontSize;
    const fontSize = perTerminalSize ?? settingsStore.state.defaultFontSize;
    const fontFamily = settingsStore.getFontFamily();
    const fontWeight = settingsStore.state.fontWeight;
    const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
    setMetrics(m);

    cachedBgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    cachedFgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";

    const cols = Math.floor(rect.width / m.cellWidth);
    const rows = Math.floor(rect.height / m.cellHeight);
    if (cols <= 0 || rows <= 0) return;
    const logicalW = cols * m.cellWidth;
    const logicalH = rows * m.cellHeight;
    canvasRef.width = logicalW * dpr;
    canvasRef.height = logicalH * dpr;
    canvasRef.style.width = `${logicalW}px`;
    canvasRef.style.height = `${logicalH}px`;
    ctx.scale(dpr, dpr);
    overlayCanvasRef.width = logicalW * dpr;
    overlayCanvasRef.height = logicalH * dpr;
    overlayCanvasRef.style.width = `${logicalW}px`;
    overlayCanvasRef.style.height = `${logicalH}px`;
    octx.scale(dpr, dpr);
    if (cols > 0 && rows > 0 && logicalW > 0 && logicalH > 0 && invokeRef
        && (cols !== lastResizeCols || rows !== lastResizeRows)) {
      lastResizeCols = cols;
      lastResizeRows = rows;

      rowMap.clear();
      fullRepaintNeeded = true;
      lastDisplayOffset = -1;
      invokeRef("resize_pty", { sessionId: props.sessionId, rows, cols }).catch(ipcErr("resize_pty"));
    }

    if (currentFrame) {
      fullRepaintNeeded = true;
      paintFrame(currentFrame, m);
    }
  }

  function paintFrame(frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
    const w = canvasRef.width / m.dpr;
    const fontFamily = settingsStore.getFontFamily();

    if (fullRepaintNeeded || !dirtyIndices) {
      // Full repaint: clear canvas + paint all rows
      const h = canvasRef.height / m.dpr;
      ctx.fillStyle = cachedBgDefault;
      ctx.fillRect(0, 0, w, h);
      for (const [, row] of rowMap) {
        paintRow(row, row.index * m.cellHeight, m, fontFamily);
      }
      fullRepaintNeeded = false;
    } else {
      // Incremental: repaint only dirty text rows (overlay handles cursor/selection/search)
      for (const idx of dirtyIndices) {
        const y = idx * m.cellHeight;
        ctx.fillStyle = cachedBgDefault;
        ctx.fillRect(0, y, w, m.cellHeight);
        const row = rowMap.get(idx);
        if (row) paintRow(row, y, m, fontFamily);
      }
    }

    repaintOverlay(frame, m);

    updateScrollbar(frame);
    updateSuggestOverlay(frame, m, dirtyIndices);
  }

  function repaintOverlay(frame: DecodedFrame, m: CellMetrics) {
    octx.clearRect(0, 0, overlayCanvasRef.width / m.dpr, overlayCanvasRef.height / m.dpr);
    paintSelection(m);
    paintSearchHighlights(m);
    paintLinkUnderline(frame, m);
    paintGutterMarkers(m);
    paintCursor(frame, m);
  }

  function paintLinkUnderline(_frame: DecodedFrame, m: CellMetrics) {
    if (!hoveredLink) return;
    const vpRow = hoveredLink.row;
    if (vpRow < 0 || vpRow >= (currentFrame?.screenRows || lastResizeRows)) return;
    const x = hoveredLink.colStart * m.cellWidth;
    const w = (hoveredLink.colEnd - hoveredLink.colStart) * m.cellWidth;
    const y = vpRow * m.cellHeight + m.cellHeight - 1;
    octx.strokeStyle = cachedFgDefault;
    octx.lineWidth = 1;
    octx.setLineDash([3, 2]);
    octx.beginPath();
    octx.moveTo(x, y + 0.5);
    octx.lineTo(x + w, y + 0.5);
    octx.stroke();
    octx.setLineDash([]);
  }

  function findNearestVisibleMatch(matches: typeof searchMatches): number {
    if (!currentFrame) return 0;
    const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
    const screenLines = currentFrame.screenRows || lastResizeRows;
    const viewportBottom = viewportTop + screenLines;
    // Prefer the last visible match (closest to cursor / bottom of viewport)
    let best = 0;
    for (let i = matches.length - 1; i >= 0; i--) {
      if (matches[i].row >= viewportTop && matches[i].row < viewportBottom) {
        best = i;
        break;
      }
    }
    return best;
  }

  function scrollToMatch(match: { row: number; col_start: number; col_end: number }) {
    if (!currentFrame || !invokeRef) return;
    const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
    const screenLines = currentFrame.screenRows || lastResizeRows;
    const viewportBottom = viewportTop + screenLines;
    if (match.row >= viewportTop && match.row < viewportBottom) return;
    const targetOffset = currentFrame.historySize - match.row + Math.floor(screenLines / 2);
    const clamped = Math.max(0, Math.min(targetOffset, currentFrame.historySize));
    const delta = clamped - currentFrame.displayOffset;
    if (delta !== 0) {
      invokeRef("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
    }
  }

  function absRowToViewport(absRow: number): number | null {
    if (!currentFrame) return null;
    const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
    const viewportRow = absRow - viewportTop;
    if (viewportRow < 0 || viewportRow >= (currentFrame.screenRows || lastResizeRows)) return null;
    return viewportRow;
  }

  function paintSearchHighlights(m: CellMetrics) {
    if (searchMatches.length === 0) return;
    for (let i = 0; i < searchMatches.length; i++) {
      const match = searchMatches[i];
      const vpRow = absRowToViewport(match.row);
      if (vpRow === null) continue;
      const isActive = i === activeSearchIndex;
      const x = match.col_start * m.cellWidth;
      const y = vpRow * m.cellHeight;
      const w = (match.col_end - match.col_start) * m.cellWidth;
      octx.fillStyle = "rgba(255, 180, 50, 0.2)";
      octx.fillRect(x, y, w, m.cellHeight);
      if (isActive) {
        octx.fillStyle = "#e8984c";
        octx.fillRect(x, y + m.cellHeight - 2, w, 2);
      }
    }
  }

  function paintGutterMarkers(m: CellMetrics) {
    const term = terminalsStore.get(props.terminalId);
    if (!term) return;
    const blocks = term.commandBlocks;
    if (blocks.length === 0) return;
    for (const block of blocks) {
      if (block.exitCode === null || block.exitCode === 0) continue;
      const vpRow = absRowToViewport(block.promptLine);
      if (vpRow === null) continue;
      octx.fillStyle = "#f85149";
      octx.fillRect(0, vpRow * m.cellHeight, 3, m.cellHeight);
    }
  }

  function paintSelection(m: CellMetrics) {
    if (!selectionStart || !selectionEnd) return;
    const absStartRow = Math.min(selectionStart.row, selectionEnd.row);
    const absEndRow = Math.max(selectionStart.row, selectionEnd.row);

    octx.fillStyle = "rgba(58, 130, 220, 0.35)";

    for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
      const vpRow = absRowToViewport(absRi);
      if (vpRow === null) continue;
      const row = rowMap.get(vpRow);
      if (!row) continue;
      const y = vpRow * m.cellHeight;

      if (absStartRow === absEndRow) {
        const c0 = Math.min(selectionStart.col, selectionEnd.col);
        const c1 = Math.max(selectionStart.col, selectionEnd.col);
        octx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
      } else if (absRi === absStartRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
        octx.fillRect(startCol * m.cellWidth, y, (row.count - startCol) * m.cellWidth, m.cellHeight);
      } else if (absRi === absEndRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
        octx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
      } else {
        octx.fillRect(0, y, row.count * m.cellWidth, m.cellHeight);
      }
    }
  }

  function getLocalSelectionText(): string {
    if (!selectionStart || !selectionEnd) return "";
    const absStartRow = Math.min(selectionStart.row, selectionEnd.row);
    const absEndRow = Math.max(selectionStart.row, selectionEnd.row);
    const lines: string[] = [];

    for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
      const vpRow = absRowToViewport(absRi);
      const row = vpRow !== null ? rowMap.get(vpRow) : null;
      if (!row) {
        lines.push("");
        continue;
      }

      let startCol = 0;
      let endCol = row.count - 1;
      if (absStartRow === absEndRow) {
        startCol = Math.min(selectionStart.col, selectionEnd.col);
        endCol = Math.max(selectionStart.col, selectionEnd.col);
      } else if (absRi === absStartRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
      } else if (absRi === absEndRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
      }

      let rowText = "";
      for (let c = startCol; c <= endCol; c++) {
        const cp = row.codepoints[c];
        rowText += cp === 0 ? " " : String.fromCodePoint(cp);
      }
      lines.push(rowText.replace(/\s+$/, ""));
    }

    return lines.join("\n");
  }

  function resolveFg(fgP: number, bgP: number, a: number, defaultColor: string): string {
    if (a & ATTR_INVERSE) {
      return (a & ATTR_DEFAULT_BG) ? defaultColor : cachedRgbString((bgP >> 16) & 0xff, (bgP >> 8) & 0xff, bgP & 0xff);
    }
    return (a & ATTR_DEFAULT_FG) ? defaultColor : cachedRgbString((fgP >> 16) & 0xff, (fgP >> 8) & 0xff, fgP & 0xff);
  }

  function resolveBg(fgP: number, bgP: number, a: number, defaultColor: string): string {
    if (a & ATTR_INVERSE) {
      return (a & ATTR_DEFAULT_FG) ? defaultColor : cachedRgbString((fgP >> 16) & 0xff, (fgP >> 8) & 0xff, fgP & 0xff);
    }
    return (a & ATTR_DEFAULT_BG) ? defaultColor : cachedRgbString((bgP >> 16) & 0xff, (bgP >> 8) & 0xff, bgP & 0xff);
  }

  function buildFontStyle(a: number, fontSize: number, fontFamily: string): string {
    return cachedFontStyle((a & ATTR_ITALIC) !== 0, (a & ATTR_BOLD) !== 0, fontSize, fontFamily);
  }

  function paintCursor(frame: DecodedFrame, m: CellMetrics) {
    if (frame.displayOffset > 0) return;
    if (!cursorBlinkOn && focused()) return;

    const settingShape: CursorShape = settingsStore.state.cursorStyle === "block" ? "block"
      : settingsStore.state.cursorStyle === "underline" ? "underline" : "beam";
    const shape: CursorShape = frame.cursorVisible ? frame.cursorShape : settingShape;
    const rect = computeCursorRect(shape, frame.cursorRow, frame.cursorCol, m);

    if (!focused()) {
      octx.strokeStyle = cachedFgDefault;
      octx.lineWidth = 1;
      octx.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1);
      return;
    }

    octx.fillStyle = cachedFgDefault;
    octx.fillRect(rect.x, rect.y, rect.w, rect.h);

    if (shape === "block") {
      const row = rowMap.get(frame.cursorRow);
      const col = frame.cursorCol;
      if (row && col < row.count) {
        const cp = row.codepoints[col];
        if (cp !== 0 && cp !== 0x20) {
          const fontFamily = settingsStore.getFontFamily();
          octx.font = buildFontStyle(row.attrs[col], m.fontSize, fontFamily);
          octx.fillStyle = cachedBgDefault;
          octx.fillText(String.fromCodePoint(cp), rect.x, frame.cursorRow * m.cellHeight + m.baseline);
        }
      }
    }
  }

  // --- Scrollbar ---

  function updateScrollbar(frame: DecodedFrame) {
    if (!scrollbarRef || !scrollThumbRef) return;
    const total = frame.historySize + (frame.screenRows || lastResizeRows || 24);
    const visible = canvasRef.getBoundingClientRect().height / (metrics()?.cellHeight ?? 16);

    if (frame.historySize === 0) {
      scrollbarRef.style.display = "none";
      return;
    }
    scrollbarRef.style.display = "block";

    const thumbRatio = Math.min(1, visible / total);
    const thumbHeight = Math.max(20, scrollbarRef.clientHeight * thumbRatio);
    const scrollRange = scrollbarRef.clientHeight - thumbHeight;
    const scrollPos = frame.historySize > 0
      ? (1 - frame.displayOffset / frame.historySize) * scrollRange
      : scrollRange;

    scrollThumbRef.style.height = `${thumbHeight}px`;
    scrollThumbRef.style.transform = `translateY(${scrollPos}px)`;
  }

  // --- Suggest / Intent overlay ---

  function rowToText(row: DecodedFrame["rows"][0]): string {
    let text = "";
    for (let ci = 0; ci < row.count; ci++) {
      const cp = row.codepoints[ci];
      text += cp === 0 ? " " : String.fromCodePoint(cp);
    }
    return text;
  }

  function makeOverlayDiv(top: number, height: number, background: string): HTMLDivElement {
    const div = document.createElement("div");
    div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
    return div;
  }

  // Cached suggest/intent overlay state to avoid full DOM rebuild
  let lastSuggestOverlayKey = "";

  function updateSuggestOverlay(_frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
    if (!overlayRef) return;

    // Skip full rescan if no dirty rows touch suggest/intent patterns
    if (dirtyIndices && !fullRepaintNeeded) {
      let hasSuggestContent = false;
      for (const idx of dirtyIndices) {
        const row = rowMap.get(idx);
        if (!row) continue;
        const text = rowToText(row);
        if (SUGGEST_ANCHOR_RE.test(text) || INTENT_RE.test(text)) {
          hasSuggestContent = true;
          break;
        }
      }
      if (!hasSuggestContent) {
        if (lastSuggestOverlayKey === "") return;
        // Stale overlay — fall through to rebuild/clear it
      }
    }

    const bg = cachedBgDefault;
    const numRows = lastResizeRows || 24;

    const getRowSnapshot = (i: number) => {
      const row = rowMap.get(i);
      if (!row) return null;
      return { text: rowToText(row), isWrapped: false };
    };

    // Build new overlay key to detect changes
    const parts: string[] = [];
    const newChildren: HTMLDivElement[] = [];
    for (let row = 0; row < numRows; row++) {
      const snapshot = getRowSnapshot(row);
      if (!snapshot) continue;
      const text = snapshot.text;

      if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, numRows, getRowSnapshot)) {
        newChildren.push(makeOverlayDiv(row * m.cellHeight, m.cellHeight, bg));
        parts.push(`s${row}`);
        const hiddenRows = continuationRowsAfterSuggest(row, numRows, getRowSnapshot);
        for (const contRow of hiddenRows) {
          newChildren.push(makeOverlayDiv(contRow * m.cellHeight, m.cellHeight, bg));
          parts.push(`c${contRow}`);
        }
        if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
      } else if (INTENT_RE.test(text)) {
        newChildren.push(makeOverlayDiv(row * m.cellHeight, m.cellHeight, "rgba(181,147,90,0.12)"));
        parts.push(`i${row}`);
      }
    }

    const newKey = parts.join(",");
    if (newKey === lastSuggestOverlayKey) return;
    lastSuggestOverlayKey = newKey;

    overlayRef.textContent = "";
    for (const child of newChildren) {
      overlayRef.appendChild(child);
    }
  }

  function startBlink() {
    stopBlink();
    cursorBlinkOn = true;
    blinkInterval = setInterval(() => {
      cursorBlinkOn = !cursorBlinkOn;
      if (rafId === undefined) {
        rafId = requestAnimationFrame(() => {
          rafId = undefined;
          if (!alive || hidden) return;
          const m = metrics();
          if (currentFrame && m) repaintCursorOnly(currentFrame, m);
        });
      }
    }, 700);
  }

  function stopBlink() {
    if (blinkInterval != null) {
      clearInterval(blinkInterval);
      blinkInterval = undefined;
    }
  }

  function resetBlink() {
    cursorBlinkOn = true;
    startBlink();
  }

  function drawBoxDrawingChar(cp: number, x: number, y: number, m: CellMetrics): boolean {
    if (cp < 0x2500 || cp > 0x257F) return false;
    const w = m.cellWidth;
    const h = m.cellHeight;
    const cx = x + Math.floor(w / 2);
    const cy = y + Math.floor(h / 2);
    const lw = Math.max(1, Math.round(w / 8));
    const hlw = Math.max(1, Math.round(w / 4));

    const line = (x1: number, y1: number, x2: number, y2: number, heavy = false) => {
      ctx.lineWidth = heavy ? hlw : lw;
      ctx.beginPath();
      ctx.moveTo(x1 + 0.5, y1 + 0.5);
      ctx.lineTo(x2 + 0.5, y2 + 0.5);
      ctx.stroke();
    };

    const right = x + w;
    const bottom = y + h;

    const L = 1, R = 2, U = 4, D = 8;
    const HL = 16, HR = 32, HU = 64, HD = 128;

    let seg = 0;
    switch (cp) {
      case 0x2500: seg = L | R; break;           // ─
      case 0x2501: seg = HL | HR; break;          // ━
      case 0x2502: seg = U | D; break;            // │
      case 0x2503: seg = HU | HD; break;          // ┃
      case 0x250C: seg = R | D; break;            // ┌
      case 0x250D: seg = HR | D; break;           // ┍
      case 0x250E: seg = R | HD; break;           // ┎
      case 0x250F: seg = HR | HD; break;          // ┏
      case 0x2510: seg = L | D; break;            // ┐
      case 0x2511: seg = HL | D; break;           // ┑
      case 0x2512: seg = L | HD; break;           // ┒
      case 0x2513: seg = HL | HD; break;          // ┓
      case 0x2514: seg = R | U; break;            // └
      case 0x2515: seg = HR | U; break;           // ┕
      case 0x2516: seg = R | HU; break;           // ┖
      case 0x2517: seg = HR | HU; break;          // ┗
      case 0x2518: seg = L | U; break;            // ┘
      case 0x2519: seg = HL | U; break;           // ┙
      case 0x251A: seg = L | HU; break;           // ┚
      case 0x251B: seg = HL | HU; break;          // ┛
      case 0x251C: seg = U | D | R; break;        // ├
      case 0x251D: seg = U | D | HR; break;       // ┝
      case 0x251E: seg = HU | D | R; break;       // ┞
      case 0x251F: seg = U | HD | R; break;       // ┟
      case 0x2520: seg = HU | HD | R; break;      // ┠
      case 0x2521: seg = HU | D | HR; break;      // ┡
      case 0x2522: seg = U | HD | HR; break;      // ┢
      case 0x2523: seg = HU | HD | HR; break;     // ┣
      case 0x2524: seg = U | D | L; break;        // ┤
      case 0x2525: seg = U | D | HL; break;       // ┥
      case 0x2526: seg = HU | D | L; break;       // ┦
      case 0x2527: seg = U | HD | L; break;       // ┧
      case 0x2528: seg = HU | HD | L; break;      // ┨
      case 0x2529: seg = HU | D | HL; break;      // ┩
      case 0x252A: seg = U | HD | HL; break;      // ┪
      case 0x252B: seg = HU | HD | HL; break;     // ┫
      case 0x252C: seg = L | R | D; break;        // ┬
      case 0x252D: seg = HL | R | D; break;       // ┭
      case 0x252E: seg = L | HR | D; break;       // ┮
      case 0x252F: seg = HL | HR | D; break;      // ┯
      case 0x2530: seg = L | R | HD; break;       // ┰
      case 0x2531: seg = HL | R | HD; break;      // ┱
      case 0x2532: seg = L | HR | HD; break;      // ┲
      case 0x2533: seg = HL | HR | HD; break;     // ┳
      case 0x2534: seg = L | R | U; break;        // ┴
      case 0x2535: seg = HL | R | U; break;       // ┵
      case 0x2536: seg = L | HR | U; break;       // ┶
      case 0x2537: seg = HL | HR | U; break;      // ┷
      case 0x2538: seg = L | R | HU; break;       // ┸
      case 0x2539: seg = HL | R | HU; break;      // ┹
      case 0x253A: seg = L | HR | HU; break;      // ┺
      case 0x253B: seg = HL | HR | HU; break;     // ┻
      case 0x253C: seg = L | R | U | D; break;    // ┼
      case 0x253D: seg = HL | R | U | D; break;   // ┽
      case 0x253E: seg = L | HR | U | D; break;   // ┾
      case 0x253F: seg = HL | HR | U | D; break;  // ┿
      case 0x2540: seg = L | R | HU | D; break;   // ╀
      case 0x2541: seg = L | R | U | HD; break;   // ╁
      case 0x2542: seg = L | R | HU | HD; break;  // ╂
      case 0x2543: seg = HL | R | HU | D; break;  // ╃
      case 0x2544: seg = L | HR | HU | D; break;  // ╄
      case 0x2545: seg = HL | R | U | HD; break;  // ╅
      case 0x2546: seg = L | HR | U | HD; break;  // ╆
      case 0x2547: seg = HL | HR | HU | D; break; // ╇
      case 0x2548: seg = HL | HR | U | HD; break; // ╈
      case 0x2549: seg = HL | R | HU | HD; break; // ╉
      case 0x254A: seg = L | HR | HU | HD; break; // ╊
      case 0x254B: seg = HL | HR | HU | HD; break;// ╋
      case 0x2574: seg = L; break;                 // ╴
      case 0x2575: seg = U; break;                 // ╵
      case 0x2576: seg = R; break;                 // ╶
      case 0x2577: seg = D; break;                 // ╷
      case 0x2578: seg = HL; break;                // ╸
      case 0x2579: seg = HU; break;                // ╹
      case 0x257A: seg = HR; break;                // ╺
      case 0x257B: seg = HD; break;                // ╻
      case 0x257C: seg = L | HR; break;            // ╼
      case 0x257D: seg = U | HD; break;            // ╽
      case 0x257E: seg = HL | R; break;            // ╾
      case 0x257F: seg = HU | D; break;            // ╿
      // Dashes: render as regular line (close enough)
      case 0x2504: case 0x2505: seg = L | R; break;   // ┄┅
      case 0x2506: case 0x2507: seg = U | D; break;   // ┆┇
      case 0x2508: case 0x2509: seg = L | R; break;   // ┈┉
      case 0x250A: case 0x250B: seg = U | D; break;   // ┊┋
      // Rounded corners
      case 0x256D: seg = R | D; break;             // ╭
      case 0x256E: seg = L | D; break;             // ╮
      case 0x256F: seg = L | U; break;             // ╯
      case 0x2570: seg = R | U; break;             // ╰
      // Light/heavy dashes (additional)
      case 0x254C: case 0x254D: seg = L | R; break;   // ╌╍
      case 0x254E: case 0x254F: seg = U | D; break;   // ╎╏
      // Diagonals
      case 0x2571: {
        line(right, y, x, bottom);
        return true;
      }
      case 0x2572: {
        line(x, y, right, bottom);
        return true;
      }
      case 0x2573: {
        line(right, y, x, bottom);
        line(x, y, right, bottom);
        return true;
      }
      // Double-line box drawing: two parallel lines with gap
      case 0x2550: case 0x2551: case 0x2552: case 0x2553: case 0x2554:
      case 0x2555: case 0x2556: case 0x2557: case 0x2558: case 0x2559:
      case 0x255A: case 0x255B: case 0x255C: case 0x255D: case 0x255E:
      case 0x255F: case 0x2560: case 0x2561: case 0x2562: case 0x2563:
      case 0x2564: case 0x2565: case 0x2566: case 0x2567: case 0x2568:
      case 0x2569: case 0x256A: case 0x256B: case 0x256C: {
        const g = Math.max(1, Math.round(w / 6));
        const dbl = (x1: number, y1: number, x2: number, y2: number, horiz: boolean) => {
          const off = Math.floor(g / 2 + 0.5);
          if (horiz) {
            line(x1, y1 - off, x2, y2 - off);
            line(x1, y1 + off, x2, y2 + off);
          } else {
            line(x1 - off, y1, x2 - off, y2);
            line(x1 + off, y1, x2 + off, y2);
          }
        };
        // Encode: which directions are single (s) vs double (d)
        // Format: [sL, sR, sU, sD, dL, dR, dU, dD]
        const t: Record<number, number[]> = {
          0x2550: [0,0,0,0,1,1,0,0], // ═
          0x2551: [0,0,0,0,0,0,1,1], // ║
          0x2552: [0,1,0,0,0,0,0,1], // ╒
          0x2553: [0,0,0,1,0,1,0,0], // ╓
          0x2554: [0,0,0,0,0,1,0,1], // ╔
          0x2555: [1,0,0,0,0,0,0,1], // ╕
          0x2556: [0,0,0,1,1,0,0,0], // ╖
          0x2557: [0,0,0,0,1,0,0,1], // ╗
          0x2558: [0,1,0,0,0,0,1,0], // ╘
          0x2559: [0,0,1,0,0,1,0,0], // ╙
          0x255A: [0,0,0,0,0,1,1,0], // ╚
          0x255B: [1,0,0,0,0,0,1,0], // ╛
          0x255C: [0,0,1,0,1,0,0,0], // ╜
          0x255D: [0,0,0,0,1,0,1,0], // ╝
          0x255E: [0,1,0,0,0,0,1,1], // ╞
          0x255F: [0,0,1,1,0,1,0,0], // ╟
          0x2560: [0,0,0,0,0,1,1,1], // ╠
          0x2561: [1,0,0,0,0,0,1,1], // ╡
          0x2562: [0,0,1,1,1,0,0,0], // ╢
          0x2563: [0,0,0,0,1,0,1,1], // ╣
          0x2564: [0,0,0,1,1,1,0,0], // ╤
          0x2565: [1,1,0,0,0,0,0,1], // ╥
          0x2566: [0,0,0,0,1,1,0,1], // ╦
          0x2567: [0,0,1,0,1,1,0,0], // ╧
          0x2568: [1,1,0,0,0,0,1,0], // ╨
          0x2569: [0,0,0,0,1,1,1,0], // ╩
          0x256A: [0,0,1,1,1,1,0,0], // ╪
          0x256B: [1,1,0,0,0,0,1,1], // ╫
          0x256C: [0,0,0,0,1,1,1,1], // ╬
        };
        const d = t[cp];
        if (!d) return false;
        if (d[0]) line(x, cy, cx, cy);
        if (d[1]) line(cx, cy, right, cy);
        if (d[2]) line(cx, y, cx, cy);
        if (d[3]) line(cx, cy, cx, bottom);
        if (d[4]) dbl(x, cy, cx, cy, true);
        if (d[5]) dbl(cx, cy, right, cy, true);
        if (d[6]) dbl(cx, y, cx, cy, false);
        if (d[7]) dbl(cx, cy, cx, bottom, false);
        return true;
      }
      default: return false;
    }

    if (seg & L) line(x, cy, cx, cy);
    if (seg & R) line(cx, cy, right, cy);
    if (seg & U) line(cx, y, cx, cy);
    if (seg & D) line(cx, cy, cx, bottom);
    if (seg & HL) line(x, cy, cx, cy, true);
    if (seg & HR) line(cx, cy, right, cy, true);
    if (seg & HU) line(cx, y, cx, cy, true);
    if (seg & HD) line(cx, cy, cx, bottom, true);
    return true;
  }

  function drawBlockChar(cp: number, x: number, y: number, m: CellMetrics): boolean {
    const w = m.cellWidth;
    const h = m.cellHeight;
    const hw = Math.ceil(w / 2);
    const hh = Math.ceil(h / 2);
    const hw2 = w - hw;
    const hh2 = h - hh;
    switch (cp) {
      // Half blocks
      case 0x2580: ctx.fillRect(x, y, w, hh); return true;
      case 0x2584: ctx.fillRect(x, y + hh, w, hh2); return true;
      case 0x2588: ctx.fillRect(x, y, w, h); return true;
      case 0x258C: ctx.fillRect(x, y, hw, h); return true;
      case 0x2590: ctx.fillRect(x + hw, y, hw2, h); return true;
      // Shade blocks
      case 0x2591: { const a = ctx.globalAlpha; ctx.globalAlpha = a * 0.25; ctx.fillRect(x, y, w, h); ctx.globalAlpha = a; return true; }
      case 0x2592: { const a = ctx.globalAlpha; ctx.globalAlpha = a * 0.5; ctx.fillRect(x, y, w, h); ctx.globalAlpha = a; return true; }
      case 0x2593: { const a = ctx.globalAlpha; ctx.globalAlpha = a * 0.75; ctx.fillRect(x, y, w, h); ctx.globalAlpha = a; return true; }
      // Quadrant block elements
      case 0x2596: ctx.fillRect(x, y + hh, hw, hh2); return true;
      case 0x2597: ctx.fillRect(x + hw, y + hh, hw2, hh2); return true;
      case 0x2598: ctx.fillRect(x, y, hw, hh); return true;
      case 0x2599: ctx.fillRect(x, y, hw, h); ctx.fillRect(x + hw, y + hh, hw2, hh2); return true;
      case 0x259A: ctx.fillRect(x, y, hw, hh); ctx.fillRect(x + hw, y + hh, hw2, hh2); return true;
      case 0x259B: ctx.fillRect(x, y, w, hh); ctx.fillRect(x, y + hh, hw, hh2); return true;
      case 0x259C: ctx.fillRect(x, y, w, hh); ctx.fillRect(x + hw, y + hh, hw2, hh2); return true;
      case 0x259D: ctx.fillRect(x + hw, y, hw2, hh); return true;
      case 0x259E: ctx.fillRect(x + hw, y, hw2, hh); ctx.fillRect(x, y + hh, hw, hh2); return true;
      case 0x259F: ctx.fillRect(x + hw, y, hw2, h); ctx.fillRect(x, y + hh, hw, hh2); return true;
      default: return false;
    }
  }

  function paintRow(row: DecodedFrame["rows"][0], y: number, m: CellMetrics, fontFamily?: string) {
    fontFamily ??= settingsStore.getFontFamily();

    let lastVisibleCol = -1;
    for (let c = row.count - 1; c >= 0; c--) {
      const cp = row.codepoints[c];
      if (cp !== 0 && cp !== 0x20) { lastVisibleCol = c; break; }
    }

    // Pass 1: backgrounds
    for (let c = 0; c < row.count; c++) {
      const cp = row.codepoints[c];
      if (cp === 0 || (cp === 0x20 && c > lastVisibleCol)) continue;
      const a = row.attrs[c];
      if (!(a & ATTR_DEFAULT_BG) || (a & ATTR_INVERSE)) {
        ctx.fillStyle = resolveBg(row.fg[c], row.bg[c], a, cachedBgDefault);
        ctx.fillRect(c * m.cellWidth, y, m.cellWidth, m.cellHeight);
      }
    }

    // Pass 2: text runs — group adjacent cells with identical attributes so that
    // ligature-capable fonts (FiraCode, JetBrains Mono) can render multi-char
    // sequences (=>, !=, ===) as a single fillText call.
    let runStart = -1;
    let runText = "";
    let runFont = "";
    let runFg = "";
    let runDim = false;

    const flushRun = () => {
      if (runStart < 0) return;
      if (runDim) ctx.globalAlpha = 0.5;
      ctx.font = runFont;
      ctx.fillStyle = runFg;
      ctx.fillText(runText, runStart * m.cellWidth, y + m.baseline);
      if (runDim) ctx.globalAlpha = 1.0;
      runStart = -1;
      runText = "";
    };

    for (let c = 0; c < row.count; c++) {
      const cp = row.codepoints[c];
      if (cp === 0 || cp === 0x20) { flushRun(); continue; }

      const a = row.attrs[c];
      const fgP = row.fg[c];
      const bgP = row.bg[c];

      if (cp >= 0x2500 && cp <= 0x257F) {
        flushRun();
        ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
        ctx.strokeStyle = ctx.fillStyle;
        if (!drawBoxDrawingChar(cp, c * m.cellWidth, y, m)) {
          ctx.font = buildFontStyle(a, m.fontSize, fontFamily);
          ctx.fillText(String.fromCodePoint(cp), c * m.cellWidth, y + m.baseline);
        }
        continue;
      }
      if ((cp >= 0x2580 && cp <= 0x2593) || (cp >= 0x2596 && cp <= 0x259F)) {
        flushRun();
        ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
        drawBlockChar(cp, c * m.cellWidth, y, m);
        continue;
      }

      // PUA / symbol codepoints: render individually so the browser can
      // font-fallback per-glyph (Nerd Font icons, Powerline, etc.)
      if ((cp >= 0xE000 && cp <= 0xF8FF) || (cp >= 0xF0000 && cp <= 0xFFFFF)) {
        flushRun();
        const dim = (a & ATTR_DIM) !== 0;
        if (dim) ctx.globalAlpha = 0.5;
        ctx.font = buildFontStyle(a, m.fontSize, fontFamily);
        ctx.fillStyle = resolveFg(fgP, bgP, a, cachedFgDefault);
        ctx.fillText(String.fromCodePoint(cp), c * m.cellWidth, y + m.baseline);
        if (dim) ctx.globalAlpha = 1.0;
        continue;
      }

      const font = buildFontStyle(a, m.fontSize, fontFamily);
      const fg = resolveFg(fgP, bgP, a, cachedFgDefault);
      const dim = (a & ATTR_DIM) !== 0;

      if (runStart >= 0 && font === runFont && fg === runFg && dim === runDim) {
        runText += String.fromCodePoint(cp);
      } else {
        flushRun();
        runStart = c;
        runText = String.fromCodePoint(cp);
        runFont = font;
        runFg = fg;
        runDim = dim;
      }
    }
    flushRun();

    // Pass 3: decorations
    for (let c = 0; c < row.count; c++) {
      const a = row.attrs[c];
      if (!(a & (ATTR_UNDERLINE | ATTR_STRIKEOUT))) continue;
      const x = c * m.cellWidth;
      const fg = resolveFg(row.fg[c], row.bg[c], a, cachedFgDefault);
      if (a & ATTR_UNDERLINE) {
        ctx.fillStyle = fg;
        ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
      }
      if (a & ATTR_STRIKEOUT) {
        ctx.fillStyle = fg;
        ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
      }
    }
  }

  function repaintCursorOnly(frame: DecodedFrame, m: CellMetrics) {
    repaintOverlay(frame, m);
  }

  function repaintCursorIfNeeded() {
    const m = metrics();
    if (currentFrame && m) repaintCursorOnly(currentFrame, m);
  }

  function onFrame(data: ArrayBuffer | number[]) {
    const buffer = data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;
    const frame = decodeBinaryFrame(buffer);
    if (!frame) return;

    if (frame.bell) props.onBell?.();

    // When geometry changes, viewport is entirely different — must clear and repaint
    const geomChanged = frame.screenRows !== lastScreenRows || frame.screenCols !== lastScreenCols;
    const scrollChanged = frame.displayOffset !== lastDisplayOffset || frame.historySize !== lastHistorySize;

    if (geomChanged) {
      selectionStart = null;
      selectionEnd = null;
      cachedSelectionText = "";
      rowMap.clear();
      fullRepaintNeeded = true;
    }

    if (scrollChanged || geomChanged) {
      lastDisplayOffset = frame.displayOffset;
      lastHistorySize = frame.historySize;
      lastScreenRows = frame.screenRows;
      lastScreenCols = frame.screenCols;
    }

    // When backend sends all screen rows, replace rowMap to discard stale entries
    const screenRowCount = frame.screenRows || lastResizeRows || 24;
    if (frame.rows.length >= screenRowCount) {
      rowMap.clear();

      fullRepaintNeeded = true;
    } else if (scrollChanged && !geomChanged) {
      // Scroll changed but only partial rows arrived — old row indices are stale.
      // DON'T clear rowMap (would cause blank flash). Instead request a full frame;
      // when it arrives, the >= screenRowCount branch above will replace rowMap.
      fullRepaintNeeded = true;
      invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
    }
    for (const row of frame.rows) {

      rowMap.set(row.index, row);
      pendingDirtyRows.add(row.index);
    }

    currentFrame = frame;

    // Ack on receive, not after paint. Otherwise the backend is forced to wait
    // until the frontend has displayed an intermediate PTY state (for example a
    // newline carrying the previous SGR background before the CLI writes reset/text).
    invokeRef?.("ack_terminal_frame", { sessionId: props.sessionId }).catch(ipcErr("ack_terminal_frame"));

    if (hidden) return;
    scheduleRepaint();
  }

  // --- Link detection on hover ---

  let linkThrottle: ReturnType<typeof setTimeout> | undefined;

  async function checkLinksAtRow(row: number, col: number) {
    if (!invokeRef || !alive) return;
    const gen = ++linkCheckGeneration;

    // OSC 8 hyperlinks take priority — the program explicitly tagged this cell
    try {
      const uri = await invokeRef("terminal_hyperlink_at", {
        sessionId: props.sessionId, row, col,
      }) as string | null;
      if (uri) {
        hoveredLink = { row, colStart: col, colEnd: col + 1, path: uri };
        canvasRef.style.cursor = "pointer";
        if (currentFrame) { const m = metrics(); if (m) repaintOverlay(currentFrame, m); }
        return;
      }
    } catch { /* ignore — command may not exist on older backend */ }
    if (!alive || gen !== linkCheckGeneration) return;

    let rowText: string;
    try {
      rowText = await invokeRef("terminal_get_row_text", {
        sessionId: props.sessionId,
        row,
      }) as string;
    } catch { return; }
    if (!alive || gen !== linkCheckGeneration) return;

    const cacheKey = `${row}:${rowText}`;
    let links = linkCache.get(cacheKey);
    if (links === undefined) {
      const fpRe = filePathRegex();
      const fuRe = fileUrlRegex();
      const webUrlRe = /https?:\/\/[^\s<>"{}|\\^`\[\]]+/g;
      const fileMatches: { text: string; candidate: string; index: number }[] = [];
      const urlMatches: { text: string; path: string; index: number }[] = [];
      let match: RegExpExecArray | null;

      // Web URLs (no resolution needed)
      webUrlRe.lastIndex = 0;
      while ((match = webUrlRe.exec(rowText)) !== null) {
        urlMatches.push({ text: match[0], path: match[0], index: match.index });
      }

      // File paths
      fpRe.lastIndex = 0;
      while ((match = fpRe.exec(rowText)) !== null) {
        const idx = rowText.indexOf(match[1], match.index);
        fileMatches.push({ text: match[1], candidate: match[1], index: idx });
      }
      fuRe.lastIndex = 0;
      while ((match = fuRe.exec(rowText)) !== null) {
        fileMatches.push({ text: match[0], candidate: match[1], index: match.index });
      }

      if (fileMatches.length === 0 && urlMatches.length === 0) {
        linkCache.set(cacheKey, null);
        if (linkCache.size > 200) { const oldest = linkCache.keys().next().value; if (oldest !== undefined) linkCache.delete(oldest); }
        links = null;
      } else {
        // Resolve file paths
        const termId = terminalsStore.getTerminalForSession(props.sessionId);
        const termData = termId ? terminalsStore.get(termId) : undefined;
        const cwd = termData?.cwd || "";
        const resolvedFiles = await Promise.all(
          fileMatches.map(async (m) => {
            try {
              const r = await invokeRef!("resolve_terminal_path", { cwd, candidate: m.candidate }) as { absolute_path: string; is_directory: boolean } | null;
              if (!r) return null;
              let line: number | undefined;
              let col: number | undefined;
              const lc = m.candidate.match(/:(\d+)(?::(\d+))?$/);
              if (lc) {
                line = parseInt(lc[1], 10);
                if (lc[2]) col = parseInt(lc[2], 10);
              }
              return { text: m.text, path: r.absolute_path, line, col, index: m.index };
            } catch (e) {
              appLogger.debug("terminal", "resolve_terminal_path failed", { candidate: m.candidate, error: e });
              return null;
            }
          }),
        );
        const validFiles = resolvedFiles.filter(Boolean) as { text: string; path: string; line?: number; col?: number; index: number }[];
        const allLinks = [...validFiles, ...urlMatches];
        links = allLinks.length > 0 ? allLinks : null;
        if (linkCache.size > 200) { const oldest = linkCache.keys().next().value; if (oldest !== undefined) linkCache.delete(oldest); }
        linkCache.set(cacheKey, links);
      }
    }

    hoveredLink = null;
    if (links) {
      for (const link of links) {
        const start = link.index ?? 0;
        const end = start + link.text.length;
        if (col >= start && col < end) {
          hoveredLink = { row, colStart: start, colEnd: end, path: link.path, line: link.line, col: link.col };
          break;
        }
      }
    }
    canvasRef.style.cursor = hoveredLink ? "pointer" : "text";
    if (currentFrame) { const m = metrics(); if (m) repaintOverlay(currentFrame, m); }
  }

  onMount(async () => {
    const baseCtx = canvasRef.getContext("2d", { alpha: false });
    const overlayCtx = overlayCanvasRef.getContext("2d");
    if (!baseCtx || !overlayCtx) {
      appLogger.error("terminal", "Failed to acquire canvas 2D context");
      return;
    }
    ctx = baseCtx;
    octx = overlayCtx;
    acquireCache();
    const fontFamily = settingsStore.getFontFamily();
    const fontSize = settingsStore.state.defaultFontSize;
    const fontWeight = settingsStore.state.fontWeight;
    await document.fonts.load(`${fontWeight} ${fontSize}px ${fontFamily}`, "M").catch(() => document.fonts.ready);
    remeasure();

    resizeObserver = new ResizeObserver(() => {
      clearTimeout(resizeDebounce);
      resizeDebounce = setTimeout(() => remeasure(), 100);
    });
    resizeObserver.observe(containerRef);

    // Flow control: stop acking frames when hidden, request full frame on show
    visibilityObserver = new IntersectionObserver((entries) => {
      const isVisible = entries[0]?.isIntersecting ?? false;
      if (isVisible && hidden) {
        hidden = false;
        rowMap.clear();
        fullRepaintNeeded = true;
        currentFrame = null;
        lastDisplayOffset = -1;
        remeasure();
        invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
      } else if (!isVisible && !hidden) {
        hidden = true;
      }
    }, { threshold: 0 });
    visibilityObserver.observe(containerRef);

    // DPR change: browser zoom or external monitor switch
    dprChangeHandler = () => {
      dprMediaQuery?.removeEventListener("change", dprChangeHandler!);
      dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      dprMediaQuery.addEventListener("change", dprChangeHandler!);
      remeasure();
    };
    dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    dprMediaQuery.addEventListener("change", dprChangeHandler);

    canvasRef.addEventListener("focus", () => {
      setFocused(true); startBlink(); props.onFocus?.();
      if (currentFrame?.focusReporting) writePty("\x1b[I");
    });
    canvasRef.addEventListener("blur", () => {
      setFocused(false); stopBlink(); repaintCursorIfNeeded();
      if (currentFrame?.focusReporting) writePty("\x1b[O");
    });

    // --- Keyboard ---
    let composing = false;
    canvasRef.addEventListener("compositionstart", () => { composing = true; });
    canvasRef.addEventListener("compositionend", (e) => {
      composing = false;
      if (e.data) writePty(e.data);
    });

    let leftOptionHeld = false;

    canvasRef.addEventListener("keydown", (e: KeyboardEvent) => {
      if (composing) return;
      resetBlink();

      // Arrow Down with no modifiers: snap to bottom when scrolled up
      if (e.key === "ArrowDown" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey
        && currentFrame && currentFrame.displayOffset > 0) {
        e.preventDefault();
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -(currentFrame.displayOffset) }).catch(ipcErr("terminal_scroll"));
        return;
      }

      // Cmd+Up/Down (macOS) or Ctrl+Up/Down (Win/Linux): navigate between command blocks (OSC 133)
      if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey
        && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        const term = terminalsStore.get(props.terminalId);
        if (term) {
          const blocks = term.commandBlocks;
          const active = term.activeBlock;
          const allPromptLines = blocks.map((b) => b.promptLine).concat(active ? [active.promptLine] : []);
          if (allPromptLines.length > 0 && currentFrame) {
            const currentViewLine = currentFrame.historySize - currentFrame.displayOffset;
            let targetLine: number | undefined;
            if (e.key === "ArrowUp") {
              for (let i = allPromptLines.length - 1; i >= 0; i--) {
                if (allPromptLines[i] < currentViewLine) { targetLine = allPromptLines[i]; break; }
              }
            } else {
              for (let i = 0; i < allPromptLines.length; i++) {
                if (allPromptLines[i] > currentViewLine) { targetLine = allPromptLines[i]; break; }
              }
            }
            if (targetLine !== undefined) {
              invokeRef?.("terminal_scroll_to", { sessionId: props.sessionId, line: targetLine }).catch(ipcErr("terminal_scroll_to"));
            }
            e.preventDefault();
            return;
          }
        }
      }

      // Force re-render: clear accumulated buffer and request fresh frame from Rust
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "l" && !e.altKey) {
        e.preventDefault();
  
        rowMap.clear();
        fullRepaintNeeded = true;
        currentFrame = null;
        lastDisplayOffset = -1;
        remeasure();
        invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
        return;
      }

      if ((e.metaKey || e.ctrlKey) && e.key === "f" && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        props.onSearchOpen?.();
        return;
      }

      // Escape closes search when visible
      if (e.key === "Escape" && props.searchVisible) {
        e.preventDefault();
        props.onSearchClose?.();
        return;
      }

      // Resume banner: Space/Enter accept, other keys dismiss
      if (props.hasPendingResume) {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          props.onResume?.();
        } else if (e.key.length === 1) {
          props.onResumeDismiss?.();
          // Let the keystroke pass through to PTY
          const seq = keyToSequence(e);
          if (seq !== null) writePty(seq);
        } else if (e.key === "Escape" || e.key === "Backspace" || e.key === "Delete" || e.key === "Tab") {
          e.preventDefault();
          props.onResumeDismiss?.();
        }
        return;
      }

      // Cmd+Enter: don't send \r to PTY — let document-level keybinding handle
      if (e.metaKey && e.key === "Enter") {
        return;
      }

      // Ctrl/Cmd+C with selection → copy instead of interrupt
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && selectionStart && selectionEnd) {
        e.preventDefault();
        e.stopPropagation();
        copySelection();
        return;
      }

      // Windows Ctrl+V paste
      if (isWindows() && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && (e.key === "v" || e.key === "V")) {
        e.preventDefault();
        navigator.clipboard.readText().then(
          (text) => { if (text) writePty(`\x1b[200~${text}\x1b[201~`); },
        ).catch(ipcErr("clipboard_read"));
        return;
      }

      // Any keypress clears selection — full repaint to remove ghost highlights
      // Skip pure modifier keys so Cmd+C / Ctrl+C can fire as a chord
      if (selectionStart && e.key !== "Meta" && e.key !== "Control" && e.key !== "Alt" && e.key !== "Shift") {
        selectionStart = null;
        selectionEnd = null;
        cachedSelectionText = "";
        fullRepaintNeeded = true;
        scheduleRepaint();
      }

      // Shift+Enter → ESC CR (multi-line for Claude Code, Ink, etc.)
      // Must run BEFORE Kitty block — CC expects \x1b\r, not CSI 13;2 u
      if (e.key === "Enter" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        writePty("\x1b\r");
        return;
      }

      // Shift+Tab: send CSI Z but prevent browser focus navigation
      if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        writePty("\x1b[Z");
        return;
      }

      // macOS WebKit Emacs keybindings: Ctrl+A/D/E/K etc. intercepted by native
      // text system before our handler. Use e.code for reliable mapping.
      if (isMacOS() && e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
        const cm = e.code.match(/^Key([A-Z])$/);
        if (cm) {
          const ctrl = String.fromCharCode(cm[1].charCodeAt(0) - 0x40);
          e.preventDefault();
          writePty(ctrl);
          return;
        }
      }

      // Kitty keyboard protocol: encode special keys when flag 1 (disambiguate) is active
      const kbFlags = currentFrame?.keyboardFlags ?? 0;
      if (kbFlags & 1) {
        const seq = kittySequenceForKey(e.key, e.shiftKey, e.altKey, e.ctrlKey, e.metaKey);
        if (seq !== null) {
          e.preventDefault();
          writePty(seq);
          return;
        }
      }

      // macOS Alt/Option key handling (left Option only)
      if (isMacOS() && e.altKey && !e.metaKey && !e.ctrlKey) {
        if (e.code === "AltLeft") {
          leftOptionHeld = true;
          return;
        }
        if (leftOptionHeld) {
          const altSeq = altSequenceFromCode(e);
          if (altSeq) {
            e.preventDefault();
            writePty(altSeq);
            return;
          }
        }
      }
      if (!e.altKey) leftOptionHeld = false;

      // Default: legacy VT100 encoding
      const seq = keyToSequence(e);
      if (seq !== null) {
        e.preventDefault();
        e.stopPropagation();
        writePty(seq);
      }
    });

    // Track Alt key release for macOS left-option state
    canvasRef.addEventListener("keyup", (e: KeyboardEvent) => {
      if (e.code === "AltLeft") leftOptionHeld = false;
    });

    canvasRef.addEventListener("paste", (e: ClipboardEvent) => {
      if (e.clipboardData) {
        const items = e.clipboardData.items;
        for (let i = 0; i < items.length; i++) {
          if (items[i].type.startsWith("image/")) {
            e.preventDefault();
            writePty("\x16");
            return;
          }
        }
      }
      const text = e.clipboardData?.getData("text");
      if (text) writePty(`\x1b[200~${text}\x1b[201~`);
      e.preventDefault();
    });

    // --- Mouse selection ---
    let clickCount = 0;
    let lastClickTime = 0;

    canvasRef.addEventListener("mousedown", (e: MouseEvent) => {
      canvasRef.focus();
      if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
        const pos = canvasToGrid(e);
        if (currentFrame.sgrMouse) {
          writePty(sgrMouseSequence(e.button, pos.col, pos.row, true));
        }
        e.preventDefault();
        return;
      }
      if (e.button !== 0) return;
      const pos = canvasToGrid(e);
      const absRow = viewportRowToAbs(pos.row);
      if (absRow === null) return;
      const absPos = { col: pos.col, row: absRow };
      const now = Date.now();

      if (now - lastClickTime < 400) {
        clickCount++;
      } else {
        clickCount = 1;
      }
      lastClickTime = now;

      if (clickCount === 2) {
        selectionStart = absPos;
        selectionEnd = absPos;
      } else if (clickCount >= 3) {
        const m = metrics();
        const maxCol = m ? Math.floor((canvasRef.getBoundingClientRect().width) / m.cellWidth) - 1 : 79;
        selectionStart = { col: 0, row: absRow };
        selectionEnd = { col: maxCol, row: absRow };
        clickCount = 3;
      } else {
        selectionStart = absPos;
        selectionEnd = null;
      }
      selecting = true;
      fullRepaintNeeded = true;
      scheduleRepaint();
    });

    const onMouseMove = (e: MouseEvent) => {
      if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
        if (currentFrame.mouseMode >= 3) {
          const pos = canvasToGrid(e);
          writePty(sgrMouseSequence(35, pos.col, pos.row, true));
        } else if (currentFrame.mouseMode >= 2 && e.buttons > 0) {
          const pos = canvasToGrid(e);
          const btn = e.buttons & 1 ? 0 : e.buttons & 4 ? 1 : 2;
          writePty(sgrMouseSequence(32 + btn, pos.col, pos.row, true));
        }
        return;
      }

      if (selecting && selectionStart) {
        const pos = canvasToGrid(e);
        const absRow = viewportRowToAbs(pos.row);
        if (absRow === null) return;
        selectionEnd = { col: pos.col, row: absRow };
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
      }

      // Link detection (throttled)
      if (!selecting) {
        clearTimeout(linkThrottle);
        linkThrottle = setTimeout(() => {
          const pos = canvasToGrid(e);
          checkLinksAtRow(pos.row, pos.col);
        }, 100);
      }
    };

    const onMouseUp = (e: MouseEvent) => {
      if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
        const pos = canvasToGrid(e);
        if (currentFrame.sgrMouse) {
          writePty(sgrMouseSequence(e.button, pos.col, pos.row, false));
        }
        return;
      }
      if (selecting && selectionStart && selectionEnd) {
        copySelection();
      }
      selecting = false;
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);

    // Link click — plain click opens, skip if user was selecting text
    canvasRef.addEventListener("click", () => {
      if (!hoveredLink) return;
      const dragged = selectionStart && selectionEnd
        && (selectionStart.row !== selectionEnd.row || selectionStart.col !== selectionEnd.col);
      if (dragged) return;
      if (hoveredLink.path.startsWith("http://") || hoveredLink.path.startsWith("https://")) {
        handleOpenUrl(hoveredLink.path);
      } else {
        props.onOpenFilePath?.(hoveredLink.path, hoveredLink.line, hoveredLink.col);
      }
    });

    // --- Scroll ---
    function handleWheel(e: WheelEvent) {
      e.preventDefault();
      e.stopPropagation();
      if (currentFrame && currentFrame.mouseMode > 0) {
        const pos = canvasToGrid(e as unknown as MouseEvent);
        const btn = e.deltaY < 0 ? 64 : 65;
        writePty(sgrMouseSequence(btn, pos.col, pos.row, true));
        return;
      }
      const m = metrics();
      const lines = m ? Math.round(e.deltaY / m.cellHeight) : Math.sign(e.deltaY);
      const delta = -(lines || Math.sign(e.deltaY));
      if (delta !== 0) {
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
      }
    }
    canvasRef.addEventListener("wheel", handleWheel, { passive: false });
    scrollbarRef.addEventListener("wheel", handleWheel, { passive: false });

    // Scrollbar drag
    let scrollDragging = false;
    let scrollDragStartY = 0;
    let scrollDragStartOffset = 0;

    // Scrollbar track click: jump to position
    scrollbarRef.addEventListener("mousedown", (e: MouseEvent) => {
      if (e.target === scrollThumbRef) return; // thumb has its own handler
      if (!currentFrame || currentFrame.historySize === 0) return;
      e.preventDefault();
      const rect = scrollbarRef.getBoundingClientRect();
      const clickRatio = (e.clientY - rect.top) / rect.height;
      const targetOffset = Math.round((1 - clickRatio) * currentFrame.historySize);
      const delta = targetOffset - currentFrame.displayOffset;
      if (delta !== 0) {
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
      }
    });

    scrollThumbRef.addEventListener("mousedown", (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      scrollDragging = true;
      scrollDragStartY = e.clientY;
      scrollDragStartOffset = currentFrame?.displayOffset ?? 0;
    });

    const onScrollDragMove = (e: MouseEvent) => {
      if (!scrollDragging || !currentFrame) return;
      const historySize = currentFrame.historySize;
      if (historySize === 0) return;
      const trackHeight = scrollbarRef.clientHeight;
      const thumbHeight = parseFloat(scrollThumbRef.style.height) || 20;
      const scrollRange = trackHeight - thumbHeight;
      if (scrollRange <= 0) return;

      const dy = e.clientY - scrollDragStartY;
      const offsetDelta = Math.round((dy / scrollRange) * historySize);
      const newOffset = Math.max(0, Math.min(historySize, scrollDragStartOffset - offsetDelta));
      const delta = newOffset - (currentFrame.displayOffset);
      if (delta !== 0) {
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
      }
    };

    const onScrollDragUp = () => {
      scrollDragging = false;
    };

    document.addEventListener("mousemove", onScrollDragMove);
    document.addEventListener("mouseup", onScrollDragUp);

    // Touch input (mobile/tablet)
    cleanupTouch = installTouchHandlers(canvasRef, touchTextareaRef, {
      onScroll: (dy) => {
        const lines = Math.round(dy / (metrics()?.cellHeight ?? 20));
        if (lines !== 0) invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -lines }).catch(ipcErr("terminal_scroll"));
      },
      onInput: (data) => writePty(data),
      onFocus: () => { setFocused(true); startBlink(); props.onFocus?.(); },
      onFontSizeChange: (delta) => {
        const cur = settingsStore.state.defaultFontSize;
        const next = Math.round(cur + delta);
        if (next !== cur) settingsStore.setDefaultFontSize(next);
      },
      onSelectionMode: () => { /* future: enter selection UI */ },
    });

    // Subscribe to grid channel via transport abstraction
    try {
      transport = createTransport(props.sessionId);
      invokeRef = (cmd, args) => transport!.invoke(cmd, args);
      await transport.subscribe((data) => onFrame(data));
      unsubscribe = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.removeEventListener("mousemove", onScrollDragMove);
        document.removeEventListener("mouseup", onScrollDragUp);
        transport?.unsubscribe();
      };
    } catch (e) {
      appLogger.error("terminal", "Failed to subscribe to terminal grid channel", {
        sessionId: props.sessionId, error: e,
      });
      unsubscribe = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.removeEventListener("mousemove", onScrollDragMove);
        document.removeEventListener("mouseup", onScrollDragUp);
      };
    }

    // Listen for session events via transport
    if (transport) {
      await transport.onEvent("cwd", (payload) => {
        const cwd = (payload as { cwd: string }).cwd ?? (payload as string);
        terminalsStore.update(props.terminalId, { cwd });
        props.onCwdChange?.(props.terminalId, cwd);
      });
      await transport.onEvent("osc133", (payload) => {
        const { marker, line, exit_code } = payload as { marker: string; line: number; exit_code: number | null };
        terminalsStore.handleOsc133(props.terminalId, marker, line, exit_code ?? undefined);
      });
      await transport.onEvent("output", (payload) => {
        const { data } = payload as { data: string };
        pluginRegistry.processRawOutput(data, props.sessionId);
      });
    }

    props.onRef?.({
      focus: () => canvasRef.focus(),
      getSelectionText: () => cachedSelectionText,
      refresh: () => {
  
        rowMap.clear();
        fullRepaintNeeded = true;
        currentFrame = null;
        lastDisplayOffset = -1;
        remeasure();
        invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
      },
      searchFind: async (query: string) => {
        if (!query || !invokeRef) {
          searchMatches = [];
          activeSearchIndex = -1;
          const m = metrics();
          if (currentFrame && m) paintFrame(currentFrame, m);
          return { index: -1, count: 0 };
        }
        const matches = await invokeRef("terminal_search", {
          sessionId: props.sessionId, query,
        }) as { row: number; col_start: number; col_end: number }[];
        searchMatches = matches;
        if (matches.length > 0) {
          activeSearchIndex = findNearestVisibleMatch(matches);
          scrollToMatch(matches[activeSearchIndex]);
        } else {
          activeSearchIndex = -1;
        }
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
        return { index: activeSearchIndex, count: matches.length };
      },
      searchNext: () => {
        if (searchMatches.length === 0) return { index: -1, count: 0 };
        activeSearchIndex = (activeSearchIndex + 1) % searchMatches.length;
        scrollToMatch(searchMatches[activeSearchIndex]);
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
        return { index: activeSearchIndex, count: searchMatches.length };
      },
      searchPrev: () => {
        if (searchMatches.length === 0) return { index: -1, count: 0 };
        activeSearchIndex = (activeSearchIndex - 1 + searchMatches.length) % searchMatches.length;
        scrollToMatch(searchMatches[activeSearchIndex]);
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
        return { index: activeSearchIndex, count: searchMatches.length };
      },
      searchClear: () => {
        searchMatches = [];
        activeSearchIndex = -1;
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
      },
    });
  });

  createEffect(() => {
    terminalsStore.state.terminals[props.terminalId]?.fontSize;
    settingsStore.state.defaultFontSize;
    settingsStore.state.font;
    settingsStore.state.fontWeight;
    if (!alive) return;
    settingsStore.state.theme;
    invalidateGlyphCache();
    fontStyleCache.clear();
    colorStringCache.clear();
    fullRepaintNeeded = true;
    remeasure();
  });

  async function copySelection() {
    try {
      const text = getLocalSelectionText();
      if (text) {
        const trimmed = text.split("\n").map(line => line.replace(/\s+$/, "")).join("\n");
        cachedSelectionText = trimmed;
        await navigator.clipboard.writeText(trimmed);
      }
    } catch (e) {
      appLogger.warn("terminal", "Clipboard write failed", { error: e });
    }
  }

  onCleanup(() => {
    alive = false;
    stopBlink();
    if (rafId !== undefined) { cancelAnimationFrame(rafId); rafId = undefined; }
    clearTimeout(resizeDebounce);
    resizeObserver?.disconnect();
    visibilityObserver?.disconnect();
    if (dprChangeHandler) dprMediaQuery?.removeEventListener("change", dprChangeHandler);
    unsubscribe?.();
    cleanupTouch?.();
    clearTimeout(linkThrottle);
    linkCache.clear();
    rowMap.clear();
    colorStringCache.clear();
    fontStyleCache.clear();
    releaseCache();
  });

  return (
    <div
      ref={containerRef!}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
      }}
      onDragOver={(e) => {
        if (e.dataTransfer?.types?.includes("application/x-tuic-path")) {
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={(e) => {
        const path = e.dataTransfer?.getData("application/x-tuic-path");
        if (!path) return;
        e.preventDefault();
        const quoted = `'${path.replace(/'/g, "'\\''")}' `;
        writePty(quoted);
        canvasRef.focus();
      }}
    >
      {/* Offscreen textarea for mobile virtual keyboard input */}
      <textarea
        ref={touchTextareaRef!}
        style={{
          position: "fixed",
          top: "-9999px",
          left: "-9999px",
          width: "1px",
          height: "1px",
          opacity: "0",
          "pointer-events": "none",
        }}
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck={false}
        tabIndex={-1}
      />
      <canvas
        ref={canvasRef!}
        style={{
          display: "block",
          outline: "none",
          cursor: "text",
        }}
        tabIndex={0}
      />
      {/* Overlay canvas: cursor, selection, search highlights — redrawn every frame without touching base canvas */}
      <canvas
        ref={overlayCanvasRef!}
        style={{
          position: "absolute",
          top: "0",
          left: "0",
          "pointer-events": "none",
        }}
      />
      {/* Suggest/intent overlay */}
      <div
        ref={overlayRef!}
        style={{
          position: "absolute",
          top: "0",
          left: "0",
          right: "0",
          bottom: "0",
          "pointer-events": "none",
          "z-index": "10",
          overflow: "hidden",
        }}
      />
      {/* Scrollbar */}
      <div
        ref={scrollbarRef!}
        style={{
          position: "absolute",
          top: "0",
          right: "0",
          width: "14px",
          height: "100%",
          display: "none",
          "z-index": "20",
        }}
      >
        <div
          ref={scrollThumbRef!}
          style={{
            width: "10px",
            "margin-left": "2px",
            "border-radius": "5px",
            background: "var(--text-primary, rgba(255,255,255,0.3))",
            opacity: "var(--scrollbar-opacity, 0.3)",
            "min-height": "20px",
            position: "absolute",
            top: "0",
            cursor: "grab",
          }}
        />
      </div>
    </div>
  );
};

export default CanvasTerminal;
