import { Component, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { settingsStore } from "../../stores/settings";
import {
  decodeBinaryFrame,
  measureFont,
  computeCursorRect,
  snapLineHeight,
  type CellMetrics,
  type CursorShape,
  type DecodedFrame,
  type DecodedCell,
} from "./canvasTerminalUtils";
import { keyToSequence, altSequenceFromCode } from "./terminalInput";
import { kittySequenceForKey } from "./kittyKeyboard";
import { isSuggestBlock, continuationRowsAfterSuggest } from "./suggestOverlay";
import {
  filePathRegex,
  fileUrlRegex,
} from "./linkProvider";
import { terminalsStore } from "../../stores/terminals";
import { isMacOS, isWindows } from "../../platform";
// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame, DecodedCell };

export interface CanvasTerminalProps {
  sessionId: string;
  onOpenFilePath?: (path: string, line?: number, col?: number) => void;
  onSearchOpen?: () => void;
  onSearchClose?: () => void;
  searchVisible?: boolean;
}

const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
const INTENT_RE = /^[\s●⏺]*intent:\s+/;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
  let canvasRef!: HTMLCanvasElement;
  let scrollbarRef!: HTMLDivElement;
  let scrollThumbRef!: HTMLDivElement;
  let overlayRef!: HTMLDivElement;
  let containerRef!: HTMLDivElement;
  let ctx: CanvasRenderingContext2D;

  const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
  const [focused, setFocused] = createSignal(false);
  let currentFrame: DecodedFrame | null = null;
  // Accumulated screen buffer: survives partial damage frames so resize can repaint everything
  let screenRows = new Map<number, DecodedFrame["rows"][0]>();
  let lastDisplayOffset = -1;
  let cursorShape: CursorShape = "block";
  let cursorBlinkOn = true;
  let blinkInterval: ReturnType<typeof setInterval> | undefined;
  let unsubscribe: (() => void) | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;
  let rafId: number | undefined;
  let alive = true;

  // Selection state
  let selecting = false;
  let selectionStart: { col: number; row: number } | null = null;
  let selectionEnd: { col: number; row: number } | null = null;

  // Link detection
  const linkCache = new Map<string, { text: string; path: string; line?: number; col?: number; index: number }[] | null>();
  let hoveredLink: { row: number; colStart: number; colEnd: number; path: string; line?: number; col?: number } | null = null;

  // Cached CSS custom properties (re-read on remeasure, not every frame)
  let cachedBgDefault = "#1e1e1e";
  let cachedFgDefault = "#d4d4d4";

  // Row index → row data lookup (rebuilt each paintFrame)
  let rowMap = new Map<number, DecodedFrame["rows"][0]>();

  function writePty(data: string) {
    invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch(() => {});
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

  function remeasure() {
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const fontSize = settingsStore.state.defaultFontSize;
    const fontFamily = settingsStore.getFontFamily();
    const fontWeight = settingsStore.state.fontWeight;
    const m = measureFont(ctx, fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
    setMetrics(m);

    cachedBgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    cachedFgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";

    const rect = containerRef.getBoundingClientRect();
    const cols = Math.floor(rect.width / m.cellWidth);
    const rows = Math.floor(rect.height / m.cellHeight);
    const logicalW = cols * m.cellWidth;
    const logicalH = rows * m.cellHeight;
    canvasRef.width = logicalW * dpr;
    canvasRef.height = logicalH * dpr;
    canvasRef.style.width = `${logicalW}px`;
    canvasRef.style.height = `${logicalH}px`;
    ctx.scale(dpr, dpr);
    if (cols > 0 && rows > 0 && invokeRef) {
      invokeRef("resize_pty", { sessionId: props.sessionId, rows, cols }).catch(() => {});
    }

    if (currentFrame) {
      paintFrame(currentFrame, m);
    }
  }

  function paintFrame(frame: DecodedFrame, m: CellMetrics) {
    rowMap = new Map(frame.rows.map(r => [r.index, r]));
    // Fill with default background so line-height gaps aren't visible
    const w = canvasRef.width / m.dpr;
    const h = canvasRef.height / m.dpr;
    ctx.fillStyle = cachedBgDefault;
    ctx.fillRect(0, 0, w, h);
    for (const row of frame.rows) {
      const y = row.index * m.cellHeight;
      paintRow(row, y, m);
    }

    paintSelection(frame, m);
    paintCursor(frame, m);
    updateScrollbar(frame);
    updateSuggestOverlay(frame, m);
  }

  function paintSelection(frame: DecodedFrame, m: CellMetrics) {
    if (!selectionStart || !selectionEnd) return;
    const startRow = Math.min(selectionStart.row, selectionEnd.row);
    const endRow = Math.max(selectionStart.row, selectionEnd.row);

    ctx.fillStyle = "rgba(58, 130, 220, 0.35)";

    for (const row of frame.rows) {
      if (row.index < startRow || row.index > endRow) continue;
      const y = row.index * m.cellHeight;

      if (startRow === endRow) {
        const c0 = Math.min(selectionStart.col, selectionEnd.col);
        const c1 = Math.max(selectionStart.col, selectionEnd.col);
        ctx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
      } else if (row.index === startRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
        ctx.fillRect(startCol * m.cellWidth, y, (row.cells.length - startCol) * m.cellWidth, m.cellHeight);
      } else if (row.index === endRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
        ctx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
      } else {
        ctx.fillRect(0, y, row.cells.length * m.cellWidth, m.cellHeight);
      }
    }
  }

  function resolveFg(cell: DecodedCell, defaultColor: string): string {
    if (cell.inverse) {
      return cell.defaultBg ? defaultColor : `rgb(${cell.bgR},${cell.bgG},${cell.bgB})`;
    }
    return cell.defaultFg ? defaultColor : `rgb(${cell.fgR},${cell.fgG},${cell.fgB})`;
  }

  function resolveBg(cell: DecodedCell, defaultColor: string): string {
    if (cell.inverse) {
      return cell.defaultFg ? defaultColor : `rgb(${cell.fgR},${cell.fgG},${cell.fgB})`;
    }
    return cell.defaultBg ? defaultColor : `rgb(${cell.bgR},${cell.bgG},${cell.bgB})`;
  }

  function buildFontStyle(cell: DecodedCell, fontSize: number, fontFamily: string): string {
    let style = "";
    if (cell.italic) style += "italic ";
    const weight = cell.bold ? "bold" : settingsStore.state.fontWeight;
    return `${style}${weight} ${fontSize}px ${fontFamily}`;
  }

  function paintCursor(frame: DecodedFrame, m: CellMetrics) {
    if (!frame.cursorVisible) return;
    if (!cursorBlinkOn && focused()) return;

    const rect = computeCursorRect(cursorShape, frame.cursorRow, frame.cursorCol, m);

    if (!focused()) {
      ctx.strokeStyle = cachedFgDefault;
      ctx.lineWidth = 1;
      ctx.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1);
      return;
    }

    ctx.fillStyle = cachedFgDefault;
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);

    if (cursorShape === "block") {
      const row = rowMap.get(frame.cursorRow);
      const cell = row?.cells[frame.cursorCol];
      if (cell && cell.char && cell.char !== " ") {
        const fontFamily = settingsStore.getFontFamily();
        ctx.font = buildFontStyle(cell, m.fontSize, fontFamily);
        ctx.fillStyle = cachedBgDefault;
        ctx.fillText(cell.char, rect.x, frame.cursorRow * m.cellHeight + m.baseline);
      }
    }
  }

  // --- Scrollbar ---

  function updateScrollbar(frame: DecodedFrame) {
    if (!scrollbarRef || !scrollThumbRef) return;
    const total = frame.historySize + (frame.rows.length > 0 ? Math.max(...frame.rows.map(r => r.index)) + 1 : 24);
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

  function makeOverlayDiv(top: number, height: number, background: string): HTMLDivElement {
    const div = document.createElement("div");
    div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
    return div;
  }

  function updateSuggestOverlay(frame: DecodedFrame, m: CellMetrics) {
    if (!overlayRef) return;
    const bg = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    const numRows = frame.rows.length > 0 ? Math.max(...frame.rows.map(r => r.index)) + 1 : 0;

    const getRowSnapshot = (i: number) => {
      const row = rowMap.get(i);
      if (!row) return null;
      const text = row.cells.map(c => c.char || " ").join("");
      return { text, isWrapped: false };
    };

    overlayRef.textContent = "";
    for (let row = 0; row < numRows; row++) {
      const snapshot = getRowSnapshot(row);
      if (!snapshot) continue;
      const text = snapshot.text;

      if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, numRows, getRowSnapshot)) {
        overlayRef.appendChild(makeOverlayDiv(row * m.cellHeight, m.cellHeight, bg));
        const hiddenRows = continuationRowsAfterSuggest(row, numRows, getRowSnapshot);
        for (const contRow of hiddenRows) {
          overlayRef.appendChild(makeOverlayDiv(contRow * m.cellHeight, m.cellHeight, bg));
        }
        if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
      } else if (INTENT_RE.test(text)) {
        overlayRef.appendChild(makeOverlayDiv(row * m.cellHeight, m.cellHeight, "rgba(181,147,90,0.12)"));
      }
    }
  }

  function startBlink() {
    stopBlink();
    cursorBlinkOn = true;
    blinkInterval = setInterval(() => {
      cursorBlinkOn = !cursorBlinkOn;
      const m = metrics();
      if (currentFrame && m) {
        repaintCursorRow(currentFrame, m);
      }
    }, 530);
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

  function paintRow(row: DecodedFrame["rows"][0], y: number, m: CellMetrics) {
    const fontFamily = settingsStore.getFontFamily();
    let lastFont = "";
    for (let c = 0; c < row.cells.length; c++) {
      const cell = row.cells[c];
      const x = c * m.cellWidth;
      if (!cell.defaultBg || cell.inverse) {
        ctx.fillStyle = resolveBg(cell, cachedBgDefault);
        ctx.fillRect(x, y, m.cellWidth, m.cellHeight);
      }
      if (cell.char && cell.char !== " ") {
        ctx.fillStyle = resolveFg(cell, cachedFgDefault);
        if (cell.dim) ctx.globalAlpha = 0.5;
        const cp = cell.char.codePointAt(0) ?? 0;
        if (((cp >= 0x2580 && cp <= 0x2593) || (cp >= 0x2596 && cp <= 0x259F)) && drawBlockChar(cp, x, y, m)) {
          // Block element drawn as geometry
        } else {
          const font = buildFontStyle(cell, m.fontSize, fontFamily);
          if (font !== lastFont) { ctx.font = font; lastFont = font; }
          ctx.fillText(cell.char, x, y + m.baseline);
        }
        if (cell.dim) ctx.globalAlpha = 1.0;
      }
      if (cell.underline) {
        ctx.fillStyle = resolveFg(cell, cachedFgDefault);
        ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
      }
      if (cell.strikeout) {
        ctx.fillStyle = resolveFg(cell, cachedFgDefault);
        ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
      }
    }
  }

  function repaintCursorRow(frame: DecodedFrame, m: CellMetrics) {
    paintFrame(frame, m);
  }

  function repaintCursorIfNeeded() {
    const m = metrics();
    if (currentFrame && m) repaintCursorRow(currentFrame, m);
  }

  function onFrame(data: ArrayBuffer | number[]) {
    const buffer = data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;
    const frame = decodeBinaryFrame(buffer);
    if (!frame) return;

    // When scroll position changes, the entire visible content is different
    if (frame.displayOffset !== lastDisplayOffset) {
      screenRows.clear();
      lastDisplayOffset = frame.displayOffset;
    }

    // Merge incoming damaged rows into the full screen buffer
    for (const row of frame.rows) {
      screenRows.set(row.index, row);
    }

    // Build a full frame from the accumulated buffer
    currentFrame = {
      ...frame,
      rows: Array.from(screenRows.values()),
    };

    if (rafId === undefined) {
      rafId = requestAnimationFrame(() => {
        rafId = undefined;
        if (!alive) return;
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
      });
    }
  }

  // --- Link detection on hover ---

  let linkThrottle: ReturnType<typeof setTimeout> | undefined;

  async function checkLinksAtRow(row: number, col: number) {
    if (!invokeRef || !alive) return;
    const rowText = await invokeRef("terminal_get_row_text", {
      sessionId: props.sessionId,
      row,
    }) as string;
    if (!alive) return;

    const cacheKey = `${row}:${rowText}`;
    let links = linkCache.get(cacheKey);
    if (links === undefined) {
      const fpRe = filePathRegex();
      const fuRe = fileUrlRegex();
      const matches: { text: string; candidate: string; index: number }[] = [];
      let match: RegExpExecArray | null;
      fpRe.lastIndex = 0;
      while ((match = fpRe.exec(rowText)) !== null) {
        const idx = rowText.indexOf(match[1], match.index);
        matches.push({ text: match[1], candidate: match[1], index: idx });
      }
      fuRe.lastIndex = 0;
      while ((match = fuRe.exec(rowText)) !== null) {
        matches.push({ text: match[0], candidate: match[1], index: match.index });
      }

      if (matches.length === 0) {
        linkCache.set(cacheKey, null);
        if (linkCache.size > 200) linkCache.clear();
        links = null;
      } else {
        const termData = terminalsStore.get(props.sessionId);
        const cwd = termData?.cwd || "";
        const resolved = await Promise.all(
          matches.map(async (m) => {
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
            } catch {
              return null;
            }
          }),
        );
        const validLinks = resolved.filter(Boolean) as { text: string; path: string; line?: number; col?: number; index: number }[];
        links = validLinks.length > 0 ? validLinks : null;
        if (linkCache.size > 200) linkCache.clear();
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
    canvasRef.style.cursor = hoveredLink ? "pointer" : "";
  }

  onMount(async () => {
    ctx = canvasRef.getContext("2d", { alpha: false })!;
    await document.fonts.ready;
    remeasure();

    resizeObserver = new ResizeObserver(() => remeasure());
    resizeObserver.observe(containerRef);

    canvasRef.addEventListener("focus", () => { setFocused(true); startBlink(); });
    canvasRef.addEventListener("blur", () => { setFocused(false); stopBlink(); repaintCursorIfNeeded(); });

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

      // DEFERRED (2026-05-02) — Cmd+F opens the shared SearchBar but search uses xterm's
      // SearchAddon which doesn't work with CanvasTerminal. Needs a CanvasTerminalSearch
      // component that uses the Rust terminal_search IPC + canvas highlight rendering.
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
        ).catch(() => {});
        return;
      }

      // Any keypress clears selection — full repaint to remove ghost highlights
      if (selectionStart) {
        selectionStart = null;
        selectionEnd = null;
        invokeRef?.("terminal_select_clear", { sessionId: props.sessionId }).catch(() => {});
        const m = metrics();
        if (currentFrame && m) {
          ctx.clearRect(0, 0, canvasRef.width / m.dpr, canvasRef.height / m.dpr);
          paintFrame(currentFrame, m);
        }
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
      const text = e.clipboardData?.getData("text");
      if (text) writePty(`\x1b[200~${text}\x1b[201~`);
      e.preventDefault();
    });

    // --- Mouse selection ---
    let clickCount = 0;
    let lastClickTime = 0;

    canvasRef.addEventListener("mousedown", (e: MouseEvent) => {
      canvasRef.focus();
      if (e.button !== 0) return;
      const pos = canvasToGrid(e);
      const now = Date.now();

      if (now - lastClickTime < 400) {
        clickCount++;
      } else {
        clickCount = 1;
      }
      lastClickTime = now;

      if (clickCount === 2) {
        // Word select
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: pos.col, row: pos.row, word: true }).catch(() => {});
        selectionStart = pos;
        selectionEnd = pos;
      } else if (clickCount >= 3) {
        // Line select
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: 0, row: pos.row }).catch(() => {});
        const m = metrics();
        const maxCol = m ? Math.floor((canvasRef.getBoundingClientRect().width) / m.cellWidth) - 1 : 79;
        invokeRef?.("terminal_select_update", { sessionId: props.sessionId, col: maxCol, row: pos.row }).catch(() => {});
        selectionStart = { col: 0, row: pos.row };
        selectionEnd = { col: maxCol, row: pos.row };
        clickCount = 3;
      } else {
        // Start fresh selection
        selectionStart = pos;
        selectionEnd = null;
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: pos.col, row: pos.row }).catch(() => {});
      }
      selecting = true;
      const m = metrics();
      if (currentFrame && m) paintFrame(currentFrame, m);
    });

    const onMouseMove = (e: MouseEvent) => {
      if (selecting && selectionStart) {
        const pos = canvasToGrid(e);
        selectionEnd = pos;
        invokeRef?.("terminal_select_update", { sessionId: props.sessionId, col: pos.col, row: pos.row }).catch(() => {});
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

    const onMouseUp = () => {
      selecting = false;
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);

    // Link click
    canvasRef.addEventListener("click", (e: MouseEvent) => {
      if (hoveredLink && (e.metaKey || e.ctrlKey)) {
        props.onOpenFilePath?.(hoveredLink.path, hoveredLink.line, hoveredLink.col);
      }
    });

    // --- Scroll ---
    function handleWheel(e: WheelEvent) {
      e.preventDefault();
      e.stopPropagation();
      const m = metrics();
      const lines = m ? Math.round(e.deltaY / m.cellHeight) : Math.sign(e.deltaY);
      const delta = -(lines || Math.sign(e.deltaY));
      if (delta !== 0) {
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(() => {});
      }
    }
    canvasRef.addEventListener("wheel", handleWheel, { passive: false });
    scrollbarRef.addEventListener("wheel", handleWheel, { passive: false });

    // Scrollbar drag
    let scrollDragging = false;
    let scrollDragStartY = 0;
    let scrollDragStartOffset = 0;

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
        invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(() => {});
      }
    };

    const onScrollDragUp = () => {
      scrollDragging = false;
    };

    document.addEventListener("mousemove", onScrollDragMove);
    document.addEventListener("mouseup", onScrollDragUp);

    // Subscribe to grid channel
    try {
      const { invoke, Channel } = await import("@tauri-apps/api/core");
      invokeRef = invoke;
      const channel = new Channel<ArrayBuffer | number[]>();
      channel.onmessage = onFrame;
      await invoke("subscribe_terminal_grid", {
        sessionId: props.sessionId,
        channel,
      });
      invoke("terminal_request_frame", { sessionId: props.sessionId }).catch(() => {});
      unsubscribe = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.removeEventListener("mousemove", onScrollDragMove);
        document.removeEventListener("mouseup", onScrollDragUp);
        invoke("unsubscribe_terminal_grid", {
          sessionId: props.sessionId,
        }).catch(() => {});
      };
    } catch {
      unsubscribe = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
      };
    }
  });

  createEffect(() => {
    settingsStore.state.defaultFontSize;
    settingsStore.state.font;
    settingsStore.state.fontWeight;
    settingsStore.state.theme;
    remeasure();
  });

  async function copySelection() {
    if (!invokeRef) return;
    try {
      const text = await invokeRef("terminal_select_text", { sessionId: props.sessionId }) as string | null;
      if (text) {
        await navigator.clipboard.writeText(text);
      }
    } catch {
      // clipboard not available
    }
  }

  onCleanup(() => {
    alive = false;
    stopBlink();
    if (rafId !== undefined) { cancelAnimationFrame(rafId); rafId = undefined; }
    resizeObserver?.disconnect();
    unsubscribe?.();
    clearTimeout(linkThrottle);
    linkCache.clear();
    screenRows.clear();
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
    >
      <canvas
        ref={canvasRef!}
        style={{
          display: "block",
          outline: "none",
        }}
        tabIndex={0}
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
            background: "rgba(255,255,255,0.3)",
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
