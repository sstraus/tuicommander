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
 * Prevents programmatic scrollTop changes on the xterm viewport element
 * while the user is scrolled up. This blocks xterm.js DomScrollableElement's
 * rAF-based scrollTop sync that causes scroll-to-bottom jumps when agents
 * send ESC[2J (clear display) or similar sequences.
 *
 * How it works:
 * - Overrides the scrollTop setter on the viewport element via Object.defineProperty
 * - While locked, the setter silently discards writes (xterm buffer updates normally)
 * - User scroll events (wheel, mousedown on scrollbar) temporarily allow scrollTop
 *   changes so the user can always scroll freely
 * - When the user scrolls back to bottom, the lock deactivates
 *
 * This is a DOM-level defense, complementary to the ESC[2J/ESC[3J stripping
 * and ScrollTracker's afterWrite restore logic.
 */
export class ViewportLock {
  private container: HTMLElement | null = null;
  private viewport: HTMLElement | null = null;
  private locked = false;
  private userScrolling = false;
  private anchorScrollTop = 0;
  private cleanupFns: (() => void)[] = [];

  /** Register the container. No listeners or overrides installed until
   *  update() transitions to locked state. Zero footprint when at bottom. */
  attach(container: HTMLElement): void {
    this.container = container;
  }

  /** Dynamically engage/disengage ALL viewport interventions.
   *  At bottom: zero listeners, zero overrides — xterm is fully native.
   *  Scrolled up: scrollTop override + user-scroll listeners installed. */
  update(isAtBottom: boolean): void {
    if (!this.container) return;
    const shouldLock = !isAtBottom;
    if (shouldLock === this.locked) return;
    this.locked = shouldLock;

    if (shouldLock) {
      this.engage();
    } else {
      this.disengage();
    }
  }

  /** Remove everything and reset. */
  dispose(): void {
    this.disengage();
    this.container = null;
    this.locked = false;
  }

  get isLocked(): boolean {
    return this.locked;
  }

  /** Lock: save scrollTop, fight any programmatic scroll by restoring it.
   *  No DOM/CSS modifications — just event-driven scroll restore. */
  private engage(): void {
    const vp = this.container?.querySelector<HTMLElement>(".xterm-viewport");
    if (!vp) return;
    this.viewport = vp;
    this.anchorScrollTop = vp.scrollTop;

    const self = this;

    // When user scrolls via wheel, update the anchor
    const onWheel = () => { self.userScrolling = true; };

    // On ANY scroll: if not user-initiated, snap back to anchor
    const onScroll = () => {
      if (!self.viewport) return;
      if (self.userScrolling) {
        // User scrolled — update anchor to new position
        self.anchorScrollTop = self.viewport.scrollTop;
        self.userScrolling = false;
      } else {
        // Programmatic scroll (xterm jump) — restore anchor
        self.viewport.scrollTop = self.anchorScrollTop;
      }
    };

    vp.addEventListener("wheel", onWheel, { passive: true });
    vp.addEventListener("scroll", onScroll);
    this.cleanupFns.push(
      () => vp.removeEventListener("wheel", onWheel),
      () => vp.removeEventListener("scroll", onScroll),
    );
  }

  /** Unlock: remove listeners, no cleanup needed (no DOM was modified). */
  private disengage(): void {
    for (const fn of this.cleanupFns) fn();
    this.cleanupFns = [];
    this.viewport = null;
  }
}
