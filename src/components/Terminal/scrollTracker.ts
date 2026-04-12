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
/** Optional diagnostic logger — called with event name + details.
 *  Used to trace scroll-jump bugs. Terminal.tsx wires this to appLogger. */
export type ViewportLockLogger = (event: string, details: Record<string, unknown>) => void;

/** How long a user gesture marks scroll events as user-driven. Must exceed
 *  the browser's gesture→scroll latency and cover a drag's native scroll
 *  events without letting xterm's programmatic writes sneak through. */
const USER_INTENT_TTL_MS = 300;

/** Grace period after writeEnd() during which DOM scroll events are still
 *  classified as programmatic. xterm's write callback fires after parsing,
 *  but the renderer updates the DOM on the next animation frame — scroll
 *  events from that deferred render would otherwise be misclassified as
 *  user-initiated, causing the lock to disengage. 50ms covers ~3 frames
 *  at 60fps with margin. */
const WRITE_RENDER_LAG_MS = 50;

/** Maximum time the ViewportLock can stay engaged before the watchdog
 *  forces a disengage. Prevents the terminal from appearing permanently
 *  frozen due to unforeseen state machine edge cases. */
const WATCHDOG_TIMEOUT_MS = 2_000;

/** Minimum time after disengage before re-engage is allowed.
 *  Prevents the lock from immediately re-engaging after the user scrolls
 *  to bottom while PTY output is streaming. */
const REENGAGE_DEBOUNCE_MS = 300;

export class ViewportLock {
  private locked = false;
  private writeInProgress = false;
  /** Initialized to negative infinity so the "recent write" grace window
   *  is never true before the first actual writeEnd() call. */
  private lastWriteEndMs = -Infinity;
  private userIntent = false;
  private userIntentTimer: ReturnType<typeof setTimeout> | null = null;
  /** Sticky flag: set when user scrolls while locked, cleared on disengage.
   *  Unlike userIntent (300ms TTL), this persists so that a user scroll-to-bottom
   *  is recognized even if the next write arrives after the TTL expires. */
  private userScrolledWhileLocked = false;
  private anchorLine = 0;
  private scrollToLineFn: ((line: number) => void) | null = null;
  private getBufferFn: (() => BufferSnapshot) | null = null;
  private cleanupScroll: (() => void) | null = null;
  private cleanupGestures: (() => void) | null = null;
  private viewport: HTMLElement | null = null;
  private logger: ViewportLockLogger | null = null;
  /** Pending rAF handle for the deduped anchor-restore scheduled by writeEnd(). */
  private rafHandle: ReturnType<typeof requestAnimationFrame> | null = null;
  /** Watchdog timer: force-disengages after WATCHDOG_TIMEOUT_MS to prevent
   *  the terminal from appearing permanently frozen. */
  private watchdogTimer: ReturnType<typeof setTimeout> | null = null;
  private engagedAtMs = 0;
  /** Timestamp of the last disengage — used to enforce re-engage debounce. */
  private lastDisengagedAtMs = -Infinity;

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
    this.installGestureListeners();
  }

  /** Listen for user input gestures on the viewport so subsequent scroll
   *  events can be classified as user-driven even while a terminal write
   *  is in progress. Without this, the write-in-progress guard in engage()
   *  treats every scroll as programmatic and rubber-bands the user back
   *  to the anchor — making the scrollbar feel glued to the bottom during
   *  steady PTY streaming. */
  private installGestureListeners(): void {
    if (!this.viewport) return;
    const mark = () => this.markUserIntent();
    const opts: AddEventListenerOptions = { passive: true, capture: true };
    const events = ["wheel", "touchstart", "touchmove", "pointerdown", "keydown"] as const;
    for (const ev of events) {
      this.viewport.addEventListener(ev, mark, opts);
    }
    this.cleanupGestures = () => {
      for (const ev of events) {
        this.viewport?.removeEventListener(ev, mark, opts);
      }
    };
  }

  private markUserIntent(): void {
    this.userIntent = true;
    if (this.userIntentTimer) clearTimeout(this.userIntentTimer);
    this.userIntentTimer = setTimeout(() => {
      this.userIntent = false;
      this.userIntentTimer = null;
    }, USER_INTENT_TTL_MS);
  }

  /** Signal that a terminal.write() is about to start. */
  writeStart(): void {
    this.writeInProgress = true;
  }

  /** Signal that a terminal.write() callback has fired. The write flag
   *  clears immediately, but lastWriteEndMs provides a grace window for
   *  deferred renderer scroll events (see WRITE_RENDER_LAG_MS).
   *  When locked, schedules a single rAF to restore the anchor — deduped
   *  so that bursts of writes produce at most one scrollToLine per frame. */
  writeEnd(): void {
    this.writeInProgress = false;
    this.lastWriteEndMs = performance.now();
    if (this.locked && this.rafHandle === null) {
      this.rafHandle = requestAnimationFrame(() => {
        this.rafHandle = null;
        if (this.locked && this.scrollToLineFn) {
          this.scrollToLineFn(this.anchorLine);
        }
      });
    }
  }

  /** Flag a user-driven scroll intent with the same TTL as the internal
   *  gesture listeners. Kept for callers that detect user input via a
   *  path other than the viewport listeners (e.g. terminal.onKey) so the
   *  lock can disengage even while PTY output is streaming. */
  userScrollIntent(): void {
    this.markUserIntent();
  }

  /** Engage or disengage based on scroll position.
   *  At bottom → unlock (xterm native). Scrolled up → lock.
   *  Refuses to disengage during a write — xterm auto-scrolls to bottom
   *  during writes, which would falsely report "at bottom" and drop the lock.
   *  Exception: userScrollIntent() bypasses the guard for one call. */
  update(isAtBottom: boolean): void {
    const shouldLock = !isAtBottom;
    if (shouldLock === this.locked) return;
    // Debounce re-engage: don't lock again within REENGAGE_DEBOUNCE_MS of last
    // disengage. Prevents the lock from snapping back immediately when the user
    // scrolls to bottom while PTY output is streaming.
    if (shouldLock && (performance.now() - this.lastDisengagedAtMs < REENGAGE_DEBOUNCE_MS)) {
      this.logger?.("reengage-blocked-debounce", { elapsed: performance.now() - this.lastDisengagedAtMs });
      return;
    }
    this.locked = shouldLock;
    this.logger?.(shouldLock ? "engage" : "disengage", {
      anchorLine: this.anchorLine,
      writeInProgress: this.writeInProgress,
    });

    if (shouldLock) {
      this.engage();
    } else {
      this.lastDisengagedAtMs = performance.now();
      this.disengage();
    }
  }

  get isLocked(): boolean {
    return this.locked;
  }

  dispose(): void {
    this.disengage();
    this.cleanupGestures?.();
    this.cleanupGestures = null;
    if (this.userIntentTimer) {
      clearTimeout(this.userIntentTimer);
      this.userIntentTimer = null;
    }
    if (this.watchdogTimer) {
      clearTimeout(this.watchdogTimer);
      this.watchdogTimer = null;
    }
    if (this.rafHandle !== null) {
      cancelAnimationFrame(this.rafHandle);
      this.rafHandle = null;
    }
    this.userIntent = false;
    this.userScrolledWhileLocked = false;
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

    // Watchdog: force-disengage after timeout to prevent permanent freeze.
    this.engagedAtMs = performance.now();
    this.watchdogTimer = setTimeout(() => {
      if (!this.locked) return;
      const elapsed = Math.round(performance.now() - this.engagedAtMs);
      const buf = this.getBufferFn?.();
      this.logger?.("watchdog-force-disengage", {
        elapsedMs: elapsed,
        anchorLine: this.anchorLine,
        viewportY: buf?.viewportY ?? -1,
        baseY: buf?.baseY ?? -1,
        writeInProgress: this.writeInProgress,
        userIntent: this.userIntent,
        userScrolledWhileLocked: this.userScrolledWhileLocked,
      });
      this.locked = false;
      this.disengage();
    }, WATCHDOG_TIMEOUT_MS);

    const onScroll = () => {
      if (!this.getBufferFn) return;
      const buf = this.getBufferFn();

      // Scrollback cleared (ESC[3J) — anchor points to deleted lines.
      // Reset anchor to current viewport position to prevent a stale
      // anchor from jumping the viewport when baseY eventually regrows.
      if (buf.baseY < this.anchorLine) {
        this.logger?.("anchor-invalidated", {
          oldAnchor: this.anchorLine,
          baseY: buf.baseY,
          viewportY: buf.viewportY,
        });
        this.anchorLine = buf.viewportY;
      }

      // Classify scroll origin. A write may be in progress AND the user may
      // be actively scrolling (wheel, trackpad, etc.). Gesture listeners flag
      // userIntent with a short TTL so user scrolls aren't rubber-banded.
      //
      // Also treat scrolls within WRITE_RENDER_LAG_MS after writeEnd() as
      // programmatic: xterm's renderer updates the DOM on the next animation
      // frame, after the write callback — without this grace period those
      // deferred scrolls are misclassified as user-initiated.
      const recentWrite = !this.writeInProgress
        && (performance.now() - this.lastWriteEndMs < WRITE_RENDER_LAG_MS);
      const isProgrammatic = (this.writeInProgress || recentWrite) && !this.userIntent && !this.userScrolledWhileLocked;

      if (isProgrammatic) {
        // Programmatic scroll from xterm write — ignore. Anchor restore
        // is handled via rAF scheduled in writeEnd(), not the scroll handler.
        // No re-entrancy risk: scrollToLine() is called from rAF, not here.
        this.logger?.("dom-scroll-programmatic-ignored", {
          viewportY: buf.viewportY,
          baseY: buf.baseY,
          anchorLine: this.anchorLine,
        });
      } else {
        // User-initiated scroll — update anchor and set sticky flag.
        this.userScrolledWhileLocked = true;
        // Discard viewportY=0 when scrollback exists: renderer rebuilds
        // (fontSize re-assign) fire scroll events with a transient 0 value.
        const willUnlock = buf.viewportY >= buf.baseY;
        this.logger?.("dom-scroll-user", {
          viewportY: buf.viewportY,
          baseY: buf.baseY,
          anchorLine: this.anchorLine,
          willUnlock,
        });
        if (buf.viewportY > 0 || buf.baseY === 0) {
          this.anchorLine = buf.viewportY;
        }
        // If user scrolled to bottom, unlock
        if (willUnlock) {
          this.update(true);
        }
      }
    };

    this.viewport.addEventListener("scroll", onScroll);
    this.cleanupScroll = () => this.viewport?.removeEventListener("scroll", onScroll);
  }

  private disengage(): void {
    this.userScrolledWhileLocked = false;
    if (this.watchdogTimer) {
      clearTimeout(this.watchdogTimer);
      this.watchdogTimer = null;
    }
    this.cleanupScroll?.();
    this.cleanupScroll = null;
  }
}
