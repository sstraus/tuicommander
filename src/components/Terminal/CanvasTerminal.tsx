import { Component, createSignal, onMount, onCleanup } from "solid-js";
import { settingsStore } from "../../stores/settings";
import {
  decodeBinaryFrame,
  measureFont,
  computeCursorRect,
  type CellMetrics,
  type CursorShape,
  type DecodedFrame,
  type DecodedCell,
} from "./canvasTerminalUtils";
import { keyToSequence } from "./terminalInput";
// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame, DecodedCell };

export interface CanvasTerminalProps {
  sessionId: string;
}

/**
 * Canvas2D terminal renderer.
 * Receives binary grid frames via Tauri Channel and paints cells to <canvas>.
 * Replaces xterm.js for desktop builds.
 */
const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
  let canvasRef!: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  const glyphCache = new Map<string, ImageBitmap>();

  const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
  const [focused, setFocused] = createSignal(false);
  let currentFrame: DecodedFrame | null = null;
  let cursorShape: CursorShape = "block";
  let cursorBlinkOn = true;
  let blinkInterval: ReturnType<typeof setInterval> | undefined;
  let unsubscribe: (() => void) | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;

  function writePty(data: string) {
    invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch(() => {});
  }

  function remeasure() {
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const fontSize = settingsStore.state.defaultFontSize;
    const fontFamily = settingsStore.getFontFamily();
    const m = measureFont(ctx, fontSize, fontFamily, dpr);
    setMetrics(m);

    // Resize canvas backing store to match container
    const rect = canvasRef.getBoundingClientRect();
    canvasRef.width = Math.floor(rect.width * dpr);
    canvasRef.height = Math.floor(rect.height * dpr);
    ctx.scale(dpr, dpr);

    // Clear glyph cache on font change
    glyphCache.clear();

    // Repaint full frame if we have one
    if (currentFrame) {
      paintFrame(currentFrame, m);
    }
  }

  function paintFrame(frame: DecodedFrame, m: CellMetrics) {
    const fontFamily = settingsStore.getFontFamily();
    const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";

    for (const row of frame.rows) {
      const y = row.index * m.cellHeight;

      // Clear the full row
      ctx.clearRect(0, y, canvasRef.width / m.dpr, m.cellHeight);

      for (let c = 0; c < row.cells.length; c++) {
        const cell = row.cells[c];
        if (cell.char === "") continue; // wide char spacer

        const x = c * m.cellWidth;
        const fg = resolveFg(cell, fgDefault);
        const bg = resolveBg(cell, bgDefault);

        // Paint background if not default
        if (!cell.defaultBg || cell.inverse) {
          ctx.fillStyle = bg;
          ctx.fillRect(x, y, m.cellWidth, m.cellHeight);
        }

        // Paint character
        if (cell.char !== " ") {
          const fontStyle = buildFontStyle(cell, m.fontSize, fontFamily);
          ctx.font = fontStyle;
          ctx.fillStyle = fg;
          if (cell.dim) ctx.globalAlpha = 0.5;
          ctx.fillText(cell.char, x, y + m.baseline);
          if (cell.dim) ctx.globalAlpha = 1.0;
        }

        // Underline
        if (cell.underline) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
        }

        // Strikeout
        if (cell.strikeout) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
        }
      }
    }
    paintCursor(frame, m);
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
    if (cell.bold) style += "bold ";
    return `${style}${fontSize}px ${fontFamily}`;
  }

  function paintCursor(frame: DecodedFrame, m: CellMetrics) {
    if (!frame.cursorVisible) return;
    if (!cursorBlinkOn && focused()) return;

    const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";
    const rect = computeCursorRect(cursorShape, frame.cursorRow, frame.cursorCol, m);

    if (!focused()) {
      ctx.strokeStyle = fgDefault;
      ctx.lineWidth = 1;
      ctx.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1);
      return;
    }

    ctx.fillStyle = fgDefault;
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);

    // For block cursor, draw the character in inverse
    if (cursorShape === "block") {
      const row = frame.rows.find((r) => r.index === frame.cursorRow);
      const cell = row?.cells[frame.cursorCol];
      if (cell && cell.char && cell.char !== " ") {
        const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
        const fontFamily = settingsStore.getFontFamily();
        ctx.font = buildFontStyle(cell, m.fontSize, fontFamily);
        ctx.fillStyle = bgDefault;
        ctx.fillText(cell.char, rect.x, frame.cursorRow * m.cellHeight + m.baseline);
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

  function repaintCursorRow(frame: DecodedFrame, m: CellMetrics) {
    const y = frame.cursorRow * m.cellHeight;
    ctx.clearRect(0, y, canvasRef.width / m.dpr, m.cellHeight);

    const row = frame.rows.find((r) => r.index === frame.cursorRow);
    if (row) {
      const fontFamily = settingsStore.getFontFamily();
      const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
      const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";
      for (let c = 0; c < row.cells.length; c++) {
        const cell = row.cells[c];
        if (cell.char === "") continue;
        const x = c * m.cellWidth;
        const fg = resolveFg(cell, fgDefault);
        const bg = resolveBg(cell, bgDefault);
        if (!cell.defaultBg || cell.inverse) {
          ctx.fillStyle = bg;
          ctx.fillRect(x, y, m.cellWidth, m.cellHeight);
        }
        if (cell.char !== " ") {
          ctx.font = buildFontStyle(cell, m.fontSize, fontFamily);
          ctx.fillStyle = fg;
          if (cell.dim) ctx.globalAlpha = 0.5;
          ctx.fillText(cell.char, x, y + m.baseline);
          if (cell.dim) ctx.globalAlpha = 1.0;
        }
        if (cell.underline) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
        }
        if (cell.strikeout) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
        }
      }
    }
    paintCursor(frame, m);
  }

  function repaintCursorIfNeeded() {
    const m = metrics();
    if (currentFrame && m) repaintCursorRow(currentFrame, m);
  }

  function onFrame(data: ArrayBuffer | number[]) {
    const buffer = data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;
    const frame = decodeBinaryFrame(buffer);
    if (!frame) return;

    currentFrame = frame;
    const m = metrics();
    if (m) {
      paintFrame(frame, m);
    }
  }

  onMount(async () => {
    ctx = canvasRef.getContext("2d")!;
    remeasure();

    resizeObserver = new ResizeObserver(() => remeasure());
    resizeObserver.observe(canvasRef.parentElement ?? canvasRef);

    canvasRef.addEventListener("focus", () => { setFocused(true); startBlink(); });
    canvasRef.addEventListener("blur", () => { setFocused(false); stopBlink(); repaintCursorIfNeeded(); });

    let composing = false;
    canvasRef.addEventListener("compositionstart", () => { composing = true; });
    canvasRef.addEventListener("compositionend", (e) => {
      composing = false;
      if (e.data) writePty(e.data);
    });

    canvasRef.addEventListener("keydown", (e: KeyboardEvent) => {
      if (composing) return;
      resetBlink();
      const seq = keyToSequence(e);
      if (seq !== null) {
        e.preventDefault();
        e.stopPropagation();
        writePty(seq);
      }
    });

    canvasRef.addEventListener("paste", (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text");
      if (text) writePty(text);
      e.preventDefault();
    });

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
      unsubscribe = () => {
        invoke("unsubscribe_terminal_grid", {
          sessionId: props.sessionId,
        }).catch(() => {});
      };
    } catch {
      // Not in Tauri context (tests, PWA)
    }
  });

  onCleanup(() => {
    stopBlink();
    resizeObserver?.disconnect();
    unsubscribe?.();
    glyphCache.clear();
  });

  return (
    <canvas
      ref={canvasRef!}
      style={{
        width: "100%",
        height: "100%",
        display: "block",
      }}
      tabIndex={-1}
    />
  );
};

export default CanvasTerminal;
