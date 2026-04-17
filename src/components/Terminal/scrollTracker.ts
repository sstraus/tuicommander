/** Buffer state snapshot passed to ScrollTracker methods.
 *  Plain values only — no xterm dependency. */
export interface BufferSnapshot {
  viewportY: number;
  baseY: number;
  type: string; // "normal" | "alternate"
}

/** Action the caller should execute after a scroll-related decision. */
export interface RestoreAction {
  type: "scroll-to-bottom" | "scroll-to-line" | "none";
  line?: number;
  clamped?: boolean;
}

/**
 * Tracks terminal scroll position independently from xterm's internal state.
 *
 * xterm.js has several blind spots that require external tracking:
 * - onScroll doesn't fire when baseY grows while user is scrolled up
 * - buf.viewportY is unreliable when the container is display:none
 * - Alternate buffer always reads 0/0, corrupting normal buffer state
 * - onScroll fires synchronously on programmatic scrolls (re-entrancy)
 *
 * This class encapsulates all scroll state and transitions as a pure
 * state machine with zero DOM dependencies, making it unit-testable.
 */
export class ScrollTracker {
  private viewportY = 0;
  private baseY = 0;
  private _wasAtBottom = true;
  private _visible = false;
  private _suppressScroll = false;

  get isAtBottom(): boolean {
    return this._wasAtBottom;
  }

  get linesFromBottom(): number {
    return this.baseY - this.viewportY;
  }

  /** Update visibility state. When hidden, onScroll uses inference
   *  instead of trusting buf.viewportY. */
  setVisible(visible: boolean): void {
    this._visible = visible;
  }

  /** Handle terminal.onScroll event. Ignores alternate buffer and
   *  re-entrant calls from programmatic scrolls. */
  onScroll(buf: BufferSnapshot): void {
    if (buf.type !== "normal") return;
    if (this._suppressScroll) {
      this._suppressScroll = false;
      return;
    }
    this.updateState(buf);
  }

  /** Capture scroll state before fitAddon.fit(). fit() triggers reflow →
   *  potential onScroll events that corrupt the tracker. The snapshot freezes
   *  the pre-fit values for computeFitRestore to use. */
  snapshotForFit(): { wasAtBottom: boolean; linesFromBottom: number } {
    return { wasAtBottom: this._wasAtBottom, linesFromBottom: this.linesFromBottom };
  }

  /** Compute scroll action after fitAddon.fit() reflows the buffer.
   *  @param newBaseY — buffer.active.baseY after fit
   *  @param snapshot — pre-fit snapshot from snapshotForFit() */
  computeFitRestore(newBaseY: number, snapshot: { wasAtBottom: boolean; linesFromBottom: number }): RestoreAction {
    if (snapshot.wasAtBottom) {
      return { type: "scroll-to-bottom" };
    }

    if (snapshot.linesFromBottom > newBaseY) {
      // Buffer shrank below tracked scroll distance — old position gone
      return { type: "scroll-to-bottom", clamped: true };
    }

    return { type: "scroll-to-line", line: newBaseY - snapshot.linesFromBottom };
  }

  /** Suppress the next onScroll event. Call before programmatic
   *  scrollToLine/scrollToBottom to prevent re-entrant state corruption. */
  suppressNextScroll(): void {
    this._suppressScroll = true;
  }

  /** Core state update. When visible, trusts buffer values directly.
   *  When hidden, infers viewportY from wasAtBottom since buf.viewportY
   *  is unreliable (xterm doesn't scroll a zero-dimension viewport).
   *
   *  Guard: in xterm v6, DomScrollableElement can desync _ydisp from
   *  the visual scroll position, reporting viewportY=0 permanently even
   *  when the user has scrolled. */
  private updateState(buf: BufferSnapshot): void {
    if (this._visible) {
      this.viewportY = buf.viewportY;
      this.baseY = buf.baseY;
      this._wasAtBottom = buf.viewportY >= buf.baseY;
    } else {
      this.baseY = buf.baseY;
      if (this._wasAtBottom) {
        this.viewportY = buf.baseY;
      } else {
        this.viewportY = Math.min(this.viewportY, buf.baseY);
        this._wasAtBottom = this.viewportY >= buf.baseY;
      }
    }
  }
}

/**
 * Clamps the scroll range to the start of the most recent "frame" — a redraw
 * boundary marked by the agent emitting cursor-home + clear-screen
 * (`ESC[H` + `ESC[2J` in either order).
 *
 * Agents like Claude Code 2.1.109 run in normal-buffer mode and redraw by
 * re-emitting the full TUI frame on every tick, pushing prior frames into
 * scrollback. Stripping `ESC[3J` accumulates 20+ duplicate banners. Rather
 * than fighting the PTY, this component captures the baseY at each frame
 * boundary and blocks the user's scroll from going above it — the duplicate
 * history stays in memory but becomes unreachable.
 *
 * Floor is an absolute xterm buffer line, monotonically non-decreasing for
 * a single session. When xterm's scrollback eviction shifts lines down past
 * `baseY`, `getFloor(baseY)` resets to 0 until the next boundary repopulates.
 */
/** Cursor-home variants: ESC[H, ESC[;H, ESC[1;1H, ESC[1f, … */
const CURSOR_HOME_RE = /\x1b\[[0-9;]*[Hf]/;

export class ScrollFloor {
  private floor = 0;
  private pendingBoundary = false;

  /** Scan a PTY chunk for a frame-boundary marker. Cheap fast-path first. */
  scanChunk(data: string): void {
    if (!data.includes("\x1b[2J")) return;
    if (!CURSOR_HOME_RE.test(data)) return;
    this.pendingBoundary = true;
  }

  /** Capture current baseY as the new floor after a write+render, if a
   *  boundary was seen. Skipped in alternate buffer (vim/nano own the
   *  viewport there). */
  onRender(buf: BufferSnapshot): void {
    if (!this.pendingBoundary) return;
    this.pendingBoundary = false;
    if (buf.type !== "normal") return;
    this.floor = buf.baseY;
  }

  /** Get the floor for the current buffer. Auto-resets when xterm's
   *  scrollback eviction has shifted the stored line past `baseY`. */
  getFloor(baseY: number): number {
    if (this.floor > baseY) this.floor = 0;
    return this.floor;
  }

  reset(): void {
    this.floor = 0;
    this.pendingBoundary = false;
  }
}

/** Optional diagnostic logger — called with event name + details.
 *  Used to trace scroll-jump bugs. Terminal.tsx wires this to appLogger. */
export type ViewportLockLogger = (event: string, details: Record<string, unknown>) => void;

/**
 * Anchors the viewport to a specific buffer line while the user has scrolled up.
 *
 * Model: "aggancio alla riga". When engaged, every programmatic scroll triggered
 * by xterm (write parsing, renderer repaint) is restored synchronously to the
 * anchor line. User scrolls update the anchor; reaching bottom disengages.
 *
 * Classification is binary and time-independent:
 *   programmatic = writeInProgress || pendingRender
 *   user         = everything else
 *
 * `writeInProgress` is set by Terminal.tsx around `terminal.write()`.
 * `pendingRender` is set in `writeEnd()` and cleared by `renderComplete()`
 * (wired to `terminal.onRender` which fires after the actual repaint).
 * This eliminates the race between parse-end and DOM update without resorting
 * to time-based grace windows or asynchronous restore via microtask/rAF.
 *
 * Re-entrancy guard: scrollToLineFn() triggers a synchronous DOM scroll event;
 * `inRestore` blocks it from being reclassified as a user scroll.
 */
export class ViewportLock {
  private locked = false;
  private writeInProgress = false;
  private pendingRender = false;
  private anchorLine = 0;
  private scrollToLineFn: ((line: number) => void) | null = null;
  private getBufferFn: (() => BufferSnapshot) | null = null;
  private cleanupScroll: (() => void) | null = null;
  private viewport: HTMLElement | null = null;
  private logger: ViewportLockLogger | null = null;
  /** Blocks re-entry through the scroll handler while scrollToLineFn is restoring. */
  private inRestore = false;

  /** Install a diagnostic logger. Optional — no-op when unset. */
  setLogger(logger: ViewportLockLogger | null): void {
    this.logger = logger;
  }

  /** Bind terminal API callbacks. Call once after terminal.open(). */
  attach(
    container: HTMLElement,
    scrollToLine: (line: number) => void,
    getBuffer: () => BufferSnapshot,
  ): void {
    this.viewport = container.querySelector<HTMLElement>(".xterm-viewport");
    this.scrollToLineFn = scrollToLine;
    this.getBufferFn = getBuffer;
  }

  /** Signal that a terminal.write() is about to start. */
  writeStart(): void {
    this.writeInProgress = true;
  }

  /** Signal that a terminal.write() parse callback has fired. The renderer
   *  still needs to paint — flag `pendingRender` until `renderComplete()`. */
  writeEnd(): void {
    this.writeInProgress = false;
    this.pendingRender = true;
  }

  /** Called from terminal.onRender after the renderer has repainted.
   *  Clears `pendingRender` so subsequent scroll events are classified as user. */
  renderComplete(): void {
    this.pendingRender = false;
  }

  /** Engage or disengage based on scroll position.
   *  At bottom → unlock. Scrolled up → lock. */
  update(isAtBottom: boolean): void {
    const shouldLock = !isAtBottom;
    if (shouldLock === this.locked) return;
    this.locked = shouldLock;
    this.logger?.(shouldLock ? "engage" : "disengage", {
      anchorLine: this.anchorLine,
      writeInProgress: this.writeInProgress,
    });
    if (shouldLock) {
      this.engage();
    } else {
      this.disengage();
    }
  }

  get isLocked(): boolean {
    return this.locked;
  }

  dispose(): void {
    this.disengage();
    this.scrollToLineFn = null;
    this.getBufferFn = null;
    this.viewport = null;
    this.locked = false;
  }

  private engage(): void {
    if (!this.viewport || !this.getBufferFn) {
      this.locked = false;
      return;
    }
    const buf = this.getBufferFn();
    // Never anchor to line 0 when scrollback exists — renderer rebuilds
    // (fontSize re-assign, clearTextureAtlas) can report viewportY=0 transiently.
    if (buf.viewportY > 0 || buf.baseY === 0) {
      this.anchorLine = buf.viewportY;
    }

    const onScroll = () => {
      if (this.inRestore) return;
      if (!this.getBufferFn || !this.scrollToLineFn) return;
      const buf = this.getBufferFn();

      // Scrollback cleared (ESC[3J path surviving strip, or buffer shrink) —
      // anchor points to deleted lines. Reset to current viewport so the
      // restore below doesn't jump to a stale line when baseY regrows.
      if (buf.baseY < this.anchorLine) {
        this.logger?.("anchor-invalidated", {
          oldAnchor: this.anchorLine,
          baseY: buf.baseY,
          viewportY: buf.viewportY,
        });
        this.anchorLine = buf.viewportY;
      }

      const programmatic = this.writeInProgress || this.pendingRender;

      if (programmatic) {
        // xterm auto-scroll from write/render — restore anchor synchronously.
        // Sync restore in the same tick as the DOM scroll event eliminates
        // the visible frame-gap that async restore via microtask/rAF creates.
        if (buf.viewportY !== this.anchorLine) {
          this.logger?.("restore-sync", {
            viewportY: buf.viewportY,
            anchorLine: this.anchorLine,
          });
          this.inRestore = true;
          this.scrollToLineFn(this.anchorLine);
          this.inRestore = false;
        }
      } else {
        // User-initiated scroll — update anchor.
        // Discard viewportY=0 when scrollback exists: renderer rebuilds
        // (fontSize re-assign) fire scroll events with a transient 0 value.
        if (buf.viewportY > 0 || buf.baseY === 0) {
          this.anchorLine = buf.viewportY;
        }
        this.logger?.("dom-scroll-user", {
          viewportY: buf.viewportY,
          baseY: buf.baseY,
          anchorLine: this.anchorLine,
        });
        // If user reached bottom, unlock.
        if (buf.viewportY >= buf.baseY) {
          this.update(true);
        }
      }
    };

    this.viewport.addEventListener("scroll", onScroll);
    this.cleanupScroll = () => this.viewport?.removeEventListener("scroll", onScroll);
  }

  private disengage(): void {
    this.cleanupScroll?.();
    this.cleanupScroll = null;
  }
}
