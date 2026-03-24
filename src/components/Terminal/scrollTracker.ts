/** Buffer state snapshot passed to ScrollTracker methods.
 *  Plain values only — no xterm dependency. */
export interface BufferSnapshot {
  viewportY: number;
  baseY: number;
  type: string; // "normal" | "alternate"
}

/** Opaque token pairing a pre-write snapshot with its post-write callback. */
export interface WriteToken {
  viewportY: number;
  wasAtBottom: boolean;
  bufferType: string;
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

  /** Update visibility state. When hidden, onScroll and afterWrite use
   *  inference instead of trusting buf.viewportY. */
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

  /** Snapshot buffer state before terminal.write(). Returns a token
   *  to pair with the afterWrite callback.
   *  Uses tracker's viewportY (intended position) instead of xterm's live
   *  viewportY, which may be stale if a rAF-batched scrollToLine hasn't
   *  executed yet. */
  beforeWrite(buf: BufferSnapshot): WriteToken {
    return {
      viewportY: this._visible ? this.viewportY : buf.viewportY,
      wasAtBottom: this._wasAtBottom,
      bufferType: buf.type,
    };
  }

  /** Process buffer state after terminal.write() completes.
   *  Returns a RestoreAction the caller should execute.
   *  Also updates internal tracked state. */
  afterWrite(buf: BufferSnapshot, token: WriteToken): RestoreAction {
    // Skip everything on buffer type transitions (normal↔alternate).
    // Alt buffer reads 0/0 — updating state would corrupt normal tracking.
    if (buf.type !== "normal" || token.bufferType !== "normal") {
      return { type: "none" };
    }

    // Determine restore action BEFORE updating state
    let action: RestoreAction = { type: "none" };

    if (this._visible && !token.wasAtBottom && buf.viewportY !== token.viewportY) {
      // Viewport moved while user was scrolled up — restore if buffer
      // hasn't contracted below the original position
      if (buf.baseY >= token.viewportY) {
        action = { type: "scroll-to-line", line: token.viewportY };
      }
    }

    if (action.type === "scroll-to-line") {
      // Only update baseY — keep viewportY at the intended restore target.
      // The actual scrollToLine may be deferred (rAF batching), so the
      // tracker must reflect the intended position for subsequent writes.
      this.baseY = buf.baseY;
    } else {
      this.updateState(buf);
    }

    return action;
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
   *  is unreliable (xterm doesn't scroll a zero-dimension viewport). */
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
