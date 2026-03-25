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

  describe("beforeWrite / afterWrite", () => {
    it("returns scroll-to-line when visible, scrolled up, and viewport moved", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // user scrolled up
      // beforeWrite uses tracker's viewportY (50), not xterm's
      const token = t.beforeWrite(snap(50, 100));
      // After write: xterm moved viewport to 80 (escape sequence)
      const action = t.afterWrite(snap(80, 105, "normal"), token);
      expect(action.type).toBe("scroll-to-line");
      expect(action.line).toBe(50);
    });

    it("returns none when at bottom", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      const token = t.beforeWrite(snap(100, 100));
      const action = t.afterWrite(snap(101, 101), token);
      expect(action.type).toBe("none");
    });

    it("returns none when viewport did not move", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up
      const token = t.beforeWrite(snap(50, 100));
      const action = t.afterWrite(snap(50, 105), token);
      expect(action.type).toBe("none");
    });

    it("returns none when hidden (viewport unreliable)", () => {
      const t = new ScrollTracker();
      t.setVisible(false);
      const token = t.beforeWrite(snap(50, 100));
      const action = t.afterWrite(snap(0, 105), token);
      expect(action.type).toBe("none");
    });

    it("returns none when buffer contracted below position", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up
      const token = t.beforeWrite(snap(50, 100));
      // Buffer contracted (agent cleared screen)
      const action = t.afterWrite(snap(0, 10), token);
      expect(action.type).toBe("none");
    });

    it("returns none on normal→alternate transition", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100));
      const token = t.beforeWrite(snap(50, 100, "normal"));
      const action = t.afterWrite(snap(0, 0, "alternate"), token);
      expect(action.type).toBe("none");
    });

    it("returns none on alternate→normal transition", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      const token = t.beforeWrite(snap(0, 0, "alternate"));
      const action = t.afterWrite(snap(50, 100, "normal"), token);
      expect(action.type).toBe("none");
    });

    it("updates tracked state after write (visible, no restore)", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      const token = t.beforeWrite(snap(100, 100));
      t.afterWrite(snap(105, 105), token);
      expect(t.isAtBottom).toBe(true);
      expect(t.linesFromBottom).toBe(0);
    });

    it("keeps intended viewportY when restoring (rAF batching safety)", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // user at 50

      // First write: xterm moves viewport to 80
      const tok1 = t.beforeWrite(snap(50, 100));
      const act1 = t.afterWrite(snap(80, 105), tok1);
      expect(act1.type).toBe("scroll-to-line");
      expect(act1.line).toBe(50);
      // Tracker should keep viewportY=50 (intended position)
      expect(t.linesFromBottom).toBe(55); // 105 - 50

      // Second write (rAF hasn't fired, xterm still at 80):
      // beforeWrite uses tracker's viewportY=50, not xterm's 80
      const tok2 = t.beforeWrite(snap(80, 105));
      expect(tok2.viewportY).toBe(50); // from tracker, not from buf
      const act2 = t.afterWrite(snap(85, 110), tok2);
      expect(act2.type).toBe("scroll-to-line");
      expect(act2.line).toBe(50); // still restoring to user's position
    });

    it("updates tracked state after write (hidden, at bottom)", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(100, 100)); // at bottom
      t.setVisible(false);
      const token = t.beforeWrite(snap(0, 100));
      t.afterWrite(snap(0, 150), token);
      // Should infer still at bottom
      expect(t.isAtBottom).toBe(true);
    });

    it("skips state update on buffer type change", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up
      const token = t.beforeWrite(snap(50, 100, "normal"));
      t.afterWrite(snap(0, 0, "alternate"), token); // entered alt buffer
      // State must NOT be corrupted
      expect(t.isAtBottom).toBe(false);
      expect(t.linesFromBottom).toBe(50);
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
      // Simulate many writes while hidden
      const token1 = t.beforeWrite(snap(0, 100));
      t.afterWrite(snap(0, 200), token1);
      const token2 = t.beforeWrite(snap(0, 200));
      t.afterWrite(snap(0, 500), token2);
      t.setVisible(true);
      expect(t.isAtBottom).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("rapid writes while scrolled up each restore independently", () => {
      const t = new ScrollTracker();
      t.setVisible(true);
      t.onScroll(snap(50, 100)); // scrolled up

      // First write: viewport jumps
      const tok1 = t.beforeWrite(snap(50, 100));
      const act1 = t.afterWrite(snap(80, 105), tok1);
      expect(act1.type).toBe("scroll-to-line");
      expect(act1.line).toBe(50);

      // Second write: viewport jumps again from restored position
      const tok2 = t.beforeWrite(snap(50, 105));
      const act2 = t.afterWrite(snap(90, 110), tok2);
      expect(act2.type).toBe("scroll-to-line");
      expect(act2.line).toBe(50);
    });

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
  const scrollToBottomCalls: number[] = [];
  let bufferState: BufferSnapshot = { viewportY: 5, baseY: 100, type: "normal" };

  const lock = new ViewportLock();
  lock.attach(
    container,
    (line) => scrollToLineCalls.push(line),
    () => scrollToBottomCalls.push(1),
    () => bufferState,
  );

  return { lock, container, viewport, scrollToLineCalls, scrollToBottomCalls, bufferState, setBuffer: (b: BufferSnapshot) => { bufferState = b; } };
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
  });

  describe("write-based scroll lock", () => {
    it("restores scroll position when scroll fires during write", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // locked at line 5

      // Simulate a write that triggers a programmatic scroll
      lock.writeStart();
      viewport.dispatchEvent(new Event("scroll"));
      lock.writeEnd();

      expect(scrollToLineCalls).toEqual([5]);
      lock.dispose();
    });

    it("does NOT restore when scroll fires outside a write (user scroll)", () => {
      const { lock, viewport, scrollToLineCalls, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // locked

      // User scrolls (no write in progress)
      setBuffer({ viewportY: 10, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(scrollToLineCalls).toEqual([]); // no restore
      lock.dispose();
    });

    it("unlocks when user scrolls to bottom outside a write", () => {
      const { lock, viewport, setBuffer } = createLockHarness();
      setBuffer({ viewportY: 5, baseY: 100, type: "normal" });
      lock.update(false); // locked

      // User scrolls to bottom
      setBuffer({ viewportY: 100, baseY: 100, type: "normal" });
      viewport.dispatchEvent(new Event("scroll"));

      expect(lock.isLocked).toBe(false);
      lock.dispose();
    });

    it("no listeners active when unlocked (at bottom)", () => {
      const { lock, viewport, scrollToLineCalls } = createLockHarness();
      // Never locked — fire scroll events
      lock.writeStart();
      viewport.dispatchEvent(new Event("scroll"));
      lock.writeEnd();

      expect(scrollToLineCalls).toEqual([]); // no listener installed
      lock.dispose();
    });
  });

  describe("dispose", () => {
    it("cleans up and is safe to call multiple times", () => {
      const { lock } = createLockHarness();
      lock.update(false); // locked
      lock.dispose();
      lock.dispose(); // should not throw
    });
  });
});
