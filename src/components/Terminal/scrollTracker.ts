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
 * Prevents programmatic scroll changes while the user is scrolled up.
 *
 * Strategy: track whether a terminal.write() is in progress. Scroll events
 * that fire during a write are programmatic (xterm reflow). Scroll events
 * outside a write are user-initiated (wheel, scrollbar drag, keyboard).
 *
 * When locked (user not at bottom):
 * - Programmatic scrolls (during write) → restore to anchor via scrollToLine()
 * - User scrolls → update anchor, check if at bottom → unlock if so
 *
 * When unlocked (at bottom): zero listeners, zero overhead — xterm is native.
 */
export class ViewportLock {
  private locked = false;
  private writeInProgress = false;
  private anchorLine = 0;
  private scrollToLineFn: ((line: number) => void) | null = null;
  private getBufferFn: (() => BufferSnapshot) | null = null;
  private cleanupScroll: (() => void) | null = null;
  private viewport: HTMLElement | null = null;

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

  /** Signal that a terminal.write() callback has fired. */
  writeEnd(): void {
    this.writeInProgress = false;
  }

  /** Engage or disengage based on scroll position.
   *  At bottom → unlock (xterm native). Scrolled up → lock. */
  update(isAtBottom: boolean): void {
    const shouldLock = !isAtBottom;
    if (shouldLock === this.locked) return;
    this.locked = shouldLock;

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
      if (!this.getBufferFn) return;
      const buf = this.getBufferFn();

      if (this.writeInProgress) {
        // Programmatic scroll from xterm write — restore anchor
        if (this.scrollToLineFn && buf.baseY >= this.anchorLine) {
          this.scrollToLineFn(this.anchorLine);
        }
      } else {
        // User-initiated scroll — update anchor.
        // Discard viewportY=0 when scrollback exists: renderer rebuilds
        // (fontSize re-assign) fire scroll events with a transient 0 value.
        if (buf.viewportY > 0 || buf.baseY === 0) {
          this.anchorLine = buf.viewportY;
        }
        // If user scrolled to bottom, unlock
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
