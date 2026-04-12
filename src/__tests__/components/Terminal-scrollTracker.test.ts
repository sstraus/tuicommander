import { describe, it, expect } from "vitest";
import { ScrollTracker, ViewportLock, type BufferSnapshot } from "../../components/Terminal/scrollTracker";

/** Helper: create a normal-buffer snapshot */
function snap(viewportY: number, baseY: number, type = "normal"): BufferSnapshot {
  return { viewportY, baseY, type };
}

describe("ScrollTracker", () => {
  describe("initial state", () => {
    it("starts at bottom with zero position", () => {
      const t = new ScrollTracker();
      expect(t.isAtBottom).toBe(true);
      expect(t.linesFromBottom).toBe(0);
    });
  });

  describe("onScroll (visible)", () => {
    it("trusts buf.viewportY when visible", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100));
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(50);
    });

    it("detects at-bottom when viewportY >= baseY", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100));
      expect(t.isAtBottom).toBe(true);
      expect(t.linesFromBottom).toBe(0);
    });

    it("ignores alternate buffer", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up in normal
      t.onScroll(snap(0, 0, "alternate")); // alt buffer fires 0/0
      expect(t.isAtBottom).toBe(false); // must NOT corrupt
      expect(t.linesFromBottom).toBe(50);
    });

    it("accepts viewportY=0 with baseY>0 (scrolled to top)", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // user scrolled up
      t.onScroll(snap(0, 100)); // scrolled to very top
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(100);
    });

    it("viewportY=0 after being at bottom means scrolled to top", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      t.onScroll(snap(0, 100)); // scrolled to top
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(100);
    });

    it("accepts viewportY=0 when buffer is empty", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(0, 0)); // empty buffer — legitimate
      expect(t.isAtBottom).toBe(true);
    });
  });

  describe("onScroll (hidden)", () => {
    it("infers viewportY = baseY when wasAtBottom", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      t.setVisible(false);
      t.onScroll(snap(0, 200)); // hidden: viewportY unreliable
      expect(t.isAtBottom).toBe(true);
      expect(t.linesFromBottom).toBe(0);
    });

    it("clamps viewportY when scrolled up and buffer grows", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up 50 lines
      t.setVisible(false);
      t.onScroll(snap(0, 200)); // hidden: baseY grew
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(150); // 200 - 50
    });

    it("clamps viewportY when buffer contracts below position", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up
      t.setVisible(false);
      t.onScroll(snap(0, 30)); // buffer shrank below viewportY
      expect(t.isAtBottom).toBe(true); // clamped to baseY
      expect(t.linesFromBottom).toBe(0);
    });
  });

  describe("snapshotForFit / computeFitRestore", () => {
    it("returns scroll-to-bottom when wasAtBottom", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      const s = t.snapshotForFit();
      const action = t.computeFitRestore(120, s);
      expect(action.type).toBe("scroll-to-bottom");
    });

    it("returns scroll-to-line preserving linesFromBottom", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(80, 100)); // 20 lines from bottom
      const s = t.snapshotForFit();
      const action = t.computeFitRestore(150, s);
      expect(action.type).toBe("scroll-to-line");
      expect(action.line).toBe(130); // 150 - 20
    });

    it("returns scroll-to-bottom with clamped flag when buffer shrank", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // 50 lines from bottom
      const s = t.snapshotForFit();
      const action = t.computeFitRestore(30, s); // buffer shrank to 30
      expect(action.type).toBe("scroll-to-bottom");
      expect(action.clamped).toBe(true);
    });

    it("snapshot survives onScroll during fit (race condition)", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(80, 100)); // 20 lines from bottom
      const s = t.snapshotForFit();
      // Simulate: fit triggers reflow → onScroll fires with new values
      t.onScroll(snap(0, 80));
      // computeFitRestore must use the SNAPSHOT, not the corrupted state
      const action = t.computeFitRestore(120, s);
      expect(action.type).toBe("scroll-to-line");
      expect(action.line).toBe(100); // 120 - 20 (from snapshot)
    });
  });

  describe("suppressNextScroll", () => {
    it("ignores one onScroll after suppress", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up
      t.suppressNextScroll();
      t.onScroll(snap(80, 100)); // programmatic scroll — should be ignored
      expect(t.linesFromBottom).toBe(50); // unchanged
    });

    it("resumes tracking after one suppressed event", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100));
      t.suppressNextScroll();
      t.onScroll(snap(80, 100)); // suppressed
      t.onScroll(snap(90, 100)); // this should be tracked
      expect(t.linesFromBottom).toBe(10);
    });
  });

  describe("visibility transitions", () => {
    it("preserves state across hide/show without writes", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100));
      t.setVisible(false);
      t.setVisible(true);
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(50);
    });

    it("hidden writes with wasAtBottom track bottom correctly", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      t.setVisible(false);
      // Simulate many onScroll events while hidden (from writes)
      t.onScroll(snap(0, 200));
      t.onScroll(snap(0, 500));
      t.setVisible(true);
      expect(t.isAtBottom).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("computeFitRestore uses snapshot not live state", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(80, 100)); // 20 from bottom
      const s = t.snapshotForFit();
      const action = t.computeFitRestore(200, s);
      expect(action.line).toBe(180); // 200 - 20
    });
  });
});

/** Helper: create a container with a .xterm-viewport child element */
function createViewportContainer(): { container: HTMLDivElement; viewport: HTMLDivElement } {
  const container = document.createElement("div");
  const viewport = document.createElement("div");
  viewport.className = "xterm-viewport";
  container.appendChild(viewport);
  document.body.appendChild(container);
  return { container, viewport };
}

/** Helper: create mock terminal API functions and a mutable buffer for ViewportLock */
function createLockHarness() {
  const { container, viewport } = createViewportContainer();
  const scrollToLineCalls: number[] = [];
  let bufferState: BufferSnapshot = { viewportY: 5, baseY: 100, type: "normal" };

  const lock = new ViewportLock();
  lock.attach(
    container,
    (line) => scrollToLineCalls.push(line),
    () => bufferState,
  );

  return { lock, container, viewport, scrollToLineCalls, bufferState, setBuffer: (b: BufferSnapshot) => { bufferState = b; } };
}

describe("ViewportLock", () => {
  describe("attach", () => {
    it("starts unlocked", () => {
      const { lock } = createLockHarness();
      expect(lock.isLocked).toBe(false);
      lock.dispose();
    });

    it("does nothing when no .xterm-viewport found", () => {
      const lock = new ViewportLock();
      const container = document.createElement("div");
      lock.attach(
        container,
        () => {},
        () => ({ viewportY: 0, baseY: 0, type: "normal" }),
      );
      lock.update(false); // should not throw
      lock.dispose();
    });
  });

  describe("lock/unlock transitions", () => {
    it("locks when update(false) and unlocks when update(true)", () => {
      const { lock } = createLockHarness();
      lock.update(false); // user scrolled up
      expect(lock.isLocked).toBe(true);
      lock.update(true); // user at bottom
      expect(lock.isLocked).toBe(false);
      lock.dispose();
    });

    it("does not re-engage on duplicate update calls", () => {
      const { lock } = createLockHarness();
      lock.update(false);
      lock.update(false); // no-op
      expect(lock.isLocked).toBe(true);
      lock.dispose();
    });

    it("update(true) during write disengages immediately", () => {
      const { lock, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 50, baseY: 100, type: "normal" });
      lock.update(false);

      lock.writeStart();
      lock.update(true); // writeInProgress must NOT block disengage
      expect(lock.isLocked).toBe(false);

      lock.writeEnd();
      lock.dispose();
    });
  });

  describe("sync restore during write/render", () => {
    it("restores anchor synchronously when scroll fires during write", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // locked at anchorLine=5

      // xterm write auto-scrolls to bottom → scroll event fires with viewportY != anchorLine
      lock.writeStart();
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([5]); // sync restore in the same tick
      lock.writeEnd();
      lock.dispose();
    });

    it("restores anchor synchronously when scroll fires during pendingRender (after writeEnd, before renderComplete)", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false);

      // Parser finished (writeEnd), renderer hasn't repainted yet — pendingRender=true
      lock.writeStart();
      lock.writeEnd();
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([5]); // classified as programmatic (pendingRender)
      lock.renderComplete();
      lock.dispose();
    });

    it("classifies scroll as user after renderComplete()", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false);

      lock.writeStart();
      lock.writeEnd();
      lock.renderComplete(); // renderer repainted — scroll events now classified as user

      // User wheel scroll
      setBuffer({ viewportY: 30, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([]); // user scroll → no restore
      lock.dispose();
    });

    it("no restore when scrollY already at anchor during write", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // anchorLine=5

      lock.writeStart();
      setBuffer({ viewportY: 5, baseY: 120, type: "normal" }); // baseY grew, viewportY still at anchor
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([]); // no-op, already at anchor
      lock.writeEnd();
      lock.dispose();
    });
  });

  describe("user scroll outside write", () => {
    it("does NOT restore when scroll fires outside a write (user scroll)", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false);

      // User scrolls (no write in progress, no pendingRender)
      setBuffer({ viewportY: 10, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([]); // no restore
      lock.dispose();
    });

    it("updates anchor when user scrolls", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // anchorLine=5

      // User scrolls to line 40
      setBuffer({ viewportY: 40, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      // Subsequent write must restore to the NEW anchor (40), not the old one (5)
      lock.writeStart();
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([40]);
      lock.writeEnd();
      lock.dispose();
    });

    it("unlocks when user scrolls to bottom outside a write", () => {
      const { lock, viewport, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false);

      // User scrolls to bottom
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(lock.isLocked).toBe(false);
      lock.dispose();
    });

    it("discards transient viewportY=0 during user scroll when scrollback exists", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 40, baseY: 100, type: "normal" });
      lock.update(false); // anchorLine=40

      // Renderer rebuild emits transient viewportY=0 (fontSize reassign) — must NOT update anchor
      setBuffer({ viewportY: 0, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      // Subsequent write restores to 40, not 0
      lock.writeStart();
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([40]);
      lock.writeEnd();
      lock.dispose();
    });
  });

  describe("anchor invalidation", () => {
    it("resets anchor when scrollback is cleared (baseY drops below anchor)", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 50, baseY: 100, type: "normal" });
      lock.update(false); // anchorLine=50

      // ESC[3J clears scrollback → baseY drops below anchor
      lock.writeStart();
      setBuffer({ viewportY: 0, baseY: 0, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));
      // anchor reset to viewportY (0). viewportY === anchorLine → no restore needed
      expect(scrollToLineCalls).toEqual([]);

      // New content grows baseY — must NOT jump to stale anchor (50)
      setBuffer({ viewportY: 60, baseY: 60, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));
      // Sync restore to anchor=0 because classifier still programmatic (writeInProgress)
      expect(scrollToLineCalls).toEqual([0]);

      lock.writeEnd();
      lock.dispose();
    });
  });

  describe("re-entrant restore guard", () => {
    it("restores exactly once when scrollToLine triggers a synchronous scroll event", () => {
      const { container, viewport } = createViewportContainer();
      let restoreCalls = 0;
      let bufferState: BufferSnapshot = { viewportY: 100, baseY: 100, type: "normal" };

      const lock = new ViewportLock();
      lock.attach(
        container,
        (line) => {
          restoreCalls++;
          // Simulate xterm's synchronous scroll event from scrollToLine —
          // inRestore guard must block re-entry.
          bufferState = { viewportY: line, baseY: bufferState.baseY, type: "normal" };
          viewport.dispatchEvent(new Event("scroll"));
        },
        () => bufferState,
      );

      // Lock at anchorLine=50
      bufferState = { viewportY: 50, baseY: 100, type: "normal" };
      lock.update(false);

      // Write auto-scrolls to bottom → scroll fires → sync restore → re-entrant scroll blocked
      lock.writeStart();
      bufferState = { viewportY: 100, baseY: 100, type: "normal" };
      viewport.dispatchEvent(new Event("scroll"));

      expect(restoreCalls).toBe(1); // exactly one restore, no loop
      lock.writeEnd();
      lock.dispose();
    });
  });

  describe("listener lifecycle", () => {
    it("no scroll handler active when unlocked", () => {
      const { lock, viewport, scrollToLineCalls } = createLockHarness();
      // Never locked — fire scroll events
      lock.writeStart();
      viewport.dispatchEvent(new Event("scroll"));
      lock.writeEnd();

      expect(scrollToLineCalls).toEqual([]); // no listener installed
      lock.dispose();
    });

    it("removes scroll listener on disengage", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // listener installed
      lock.update(true); // disengage → listener removed

      // Scroll events after disengage should have no effect
      lock.writeStart();
      setBuffer({ viewportY: 50, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));
      lock.writeEnd();

      expect(scrollToLineCalls).toEqual([]);
      lock.dispose();
    });
  });

  describe("dispose", () => {
    it("cleans up and is safe to call multiple times", () => {
      const { lock } = createLockHarness();
      lock.update(false);
      lock.dispose();
      lock.dispose(); // should not throw
    });
  });
});
