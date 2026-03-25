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
  private viewport: HTMLElement | null = null;
  private locked = false;
  private userScrolling = false;
  private originalDescriptor: PropertyDescriptor | undefined;
  private cleanupFns: (() => void)[] = [];

  /** Attach to the .xterm-viewport element inside the terminal container.
   *  Call after terminal.open(). */
  attach(container: HTMLElement): void {
    const vp = container.querySelector<HTMLElement>(".xterm-viewport");
    if (!vp) return;
    this.viewport = vp;

    // Capture the original scrollTop descriptor from the prototype chain.
    // Walk up to find it — real browsers define it on Element.prototype,
    // but test environments (happy-dom) may not have it at all.
    this.originalDescriptor = this.findScrollTopDescriptor(vp);

    const self = this;
    const origDesc = this.originalDescriptor;

    // When no native descriptor exists (test environments), fall back to
    // a simple value store that mirrors the native scrollTop behavior.
    let storedValue = vp.scrollTop ?? 0;

    // Override scrollTop on the instance so xterm's DomScrollableElement
    // writes go through our gate.
    Object.defineProperty(vp, "scrollTop", {
      configurable: true,
      get() {
        return origDesc?.get ? origDesc.get.call(vp) : storedValue;
      },
      set(value: number) {
        if (self.locked && !self.userScrolling) {
          // Silently discard programmatic scrollTop changes while locked
          return;
        }
        if (origDesc?.set) {
          origDesc.set.call(vp, value);
        } else {
          storedValue = value;
        }
      },
    });

    // Detect user-initiated scrolling: wheel events on the viewport
    const onWheel = () => {
      self.userScrolling = true;
      // Reset after a microtask — the wheel event handler chain
      // (including xterm's) will have run by then.
      queueMicrotask(() => { self.userScrolling = false; });
    };

    // Detect scrollbar drag: mousedown on the viewport (scrollbar area)
    const onMouseDown = () => {
      self.userScrolling = true;
    };
    const onMouseUp = () => {
      self.userScrolling = false;
    };

    vp.addEventListener("wheel", onWheel, { passive: true });
    vp.addEventListener("mousedown", onMouseDown);
    window.addEventListener("mouseup", onMouseUp);

    this.cleanupFns.push(
      () => vp.removeEventListener("wheel", onWheel),
      () => vp.removeEventListener("mousedown", onMouseDown),
      () => window.removeEventListener("mouseup", onMouseUp),
    );
  }

  /** Update lock state based on whether the user is at the bottom.
   *  Call after every scroll state change (onScroll, afterWrite, etc.). */
  update(isAtBottom: boolean): void {
    this.locked = !isAtBottom;
  }

  /** Remove all listeners and restore the original scrollTop descriptor. */
  dispose(): void {
    for (const fn of this.cleanupFns) fn();
    this.cleanupFns = [];

    // Restore original scrollTop behavior
    if (this.viewport) {
      delete (this.viewport as unknown as Record<string, unknown>).scrollTop;
    }
    this.viewport = null;
    this.locked = false;
  }

  /** Whether the lock is currently active (for debugging/testing). */
  get isLocked(): boolean {
    return this.locked;
  }

  /** Walk the prototype chain to find the native scrollTop property descriptor. */
  private findScrollTopDescriptor(el: HTMLElement): PropertyDescriptor | undefined {
    let proto: object | null = el;
    while (proto) {
      const desc = Object.getOwnPropertyDescriptor(proto, "scrollTop");
      if (desc) return desc;
      proto = Object.getPrototypeOf(proto);
    }
    return undefined;
  }
}
