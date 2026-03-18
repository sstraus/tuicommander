import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  createLongPressHandler,
  createLongPressHandlerFromHotkey,
} from "../../hooks/useLongPressHotkey";
import type { KeyEvent } from "../../hooks/useLongPressHotkey";
import { parseHotkey } from "../../utils/hotkey";

function press(key: string): KeyEvent {
  return { eventType: "KeyPress", key };
}

function release(key: string): KeyEvent {
  return { eventType: "KeyRelease", key };
}

describe("createLongPressHandler", () => {
  let onStart: ReturnType<typeof vi.fn>;
  let onStop: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    onStart = vi.fn();
    onStop = vi.fn();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeHandler(hotkey: string, longPressMs = 400) {
    const parsed = parseHotkey(hotkey)!;
    return createLongPressHandler(parsed, longPressMs, {
      onStart: onStart as () => void,
      onStop: onStop as () => void,
    });
  }

  describe("short press (below threshold)", () => {
    it("does not trigger start or stop", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(200);
      h.handleEvent(release("F5"));

      expect(onStart).not.toHaveBeenCalled();
      expect(onStop).not.toHaveBeenCalled();
    });

    it("allows immediate re-press after short press", () => {
      const h = makeHandler("F5");

      // First short press
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(100);
      h.handleEvent(release("F5"));

      // Second short press
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(100);
      h.handleEvent(release("F5"));

      expect(onStart).not.toHaveBeenCalled();
    });
  });

  describe("long press (above threshold)", () => {
    it("triggers onStart when timer fires", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(400);

      expect(onStart).toHaveBeenCalledOnce();
      expect(onStop).not.toHaveBeenCalled();
    });

    it("triggers onStop on key release after long press", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(400);
      h.handleEvent(release("F5"));

      expect(onStart).toHaveBeenCalledOnce();
      expect(onStop).toHaveBeenCalledOnce();
    });

    it("respects custom longPressMs threshold", () => {
      const h = makeHandler("F5", 200);
      h.handleEvent(press("F5"));

      vi.advanceTimersByTime(199);
      expect(onStart).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      expect(onStart).toHaveBeenCalledOnce();
    });
  });

  describe("key repeat filtering", () => {
    it("ignores repeated KeyPress without KeyRelease", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      h.handleEvent(press("F5")); // repeat
      h.handleEvent(press("F5")); // repeat
      vi.advanceTimersByTime(400);

      expect(onStart).toHaveBeenCalledOnce();
    });

    it("does not restart timer on repeat", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(300);
      h.handleEvent(press("F5")); // repeat at 300ms — should NOT restart timer
      vi.advanceTimersByTime(100);

      // Timer should fire at 400ms from first press, not 400ms from repeat
      expect(onStart).toHaveBeenCalledOnce();
    });
  });

  describe("modifier combos", () => {
    it("triggers for Cmd+Space when Meta is held", () => {
      const h = makeHandler("Cmd+Space");
      h.handleEvent(press("MetaLeft"));
      h.handleEvent(press("Space"));
      vi.advanceTimersByTime(400);

      expect(onStart).toHaveBeenCalledOnce();
    });

    it("does not trigger Cmd+Space without Meta held", () => {
      const h = makeHandler("Cmd+Space");
      h.handleEvent(press("Space"));
      vi.advanceTimersByTime(400);

      expect(onStart).not.toHaveBeenCalled();
    });

    it("does not trigger Cmd+Space with extra Shift held", () => {
      const h = makeHandler("Cmd+Space");
      h.handleEvent(press("MetaLeft"));
      h.handleEvent(press("ShiftLeft")); // extra modifier
      h.handleEvent(press("Space"));
      vi.advanceTimersByTime(400);

      expect(onStart).not.toHaveBeenCalled();
    });

    it("tracks modifier release correctly", () => {
      const h = makeHandler("Cmd+D");
      h.handleEvent(press("MetaLeft"));
      h.handleEvent(press("KeyD"));
      vi.advanceTimersByTime(400);
      expect(onStart).toHaveBeenCalledOnce();

      h.handleEvent(release("KeyD"));
      expect(onStop).toHaveBeenCalledOnce();

      // Release Meta, then press D without modifier — should not trigger
      h.handleEvent(release("MetaLeft"));
      h.handleEvent(press("KeyD"));
      vi.advanceTimersByTime(400);

      expect(onStart).toHaveBeenCalledOnce(); // still only once
    });
  });

  describe("unrelated keys", () => {
    it("ignores keys that do not match hotkey", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("KeyA"));
      h.handleEvent(press("Space"));
      h.handleEvent(release("KeyA"));
      vi.advanceTimersByTime(500);

      expect(onStart).not.toHaveBeenCalled();
      expect(onStop).not.toHaveBeenCalled();
    });
  });

  describe("cleanup", () => {
    it("cancels pending timer", () => {
      const h = makeHandler("F5");
      h.handleEvent(press("F5"));
      vi.advanceTimersByTime(200);
      h.cleanup();
      vi.advanceTimersByTime(300);

      expect(onStart).not.toHaveBeenCalled();
    });
  });

  describe("getState", () => {
    it("reflects internal state correctly", () => {
      const h = makeHandler("F5");
      expect(h.getState().hotkeyDown).toBe(false);
      expect(h.getState().dictationStarted).toBe(false);

      h.handleEvent(press("F5"));
      expect(h.getState().hotkeyDown).toBe(true);
      expect(h.getState().dictationStarted).toBe(false);

      vi.advanceTimersByTime(400);
      expect(h.getState().dictationStarted).toBe(true);

      h.handleEvent(release("F5"));
      expect(h.getState().hotkeyDown).toBe(false);
      expect(h.getState().dictationStarted).toBe(false);
    });

    it("tracks modifier state", () => {
      const h = makeHandler("Cmd+Space");
      h.handleEvent(press("MetaLeft"));
      expect(h.getState().mods.cmd).toBe(true);

      h.handleEvent(release("MetaLeft"));
      expect(h.getState().mods.cmd).toBe(false);
    });
  });

  describe("events with missing key", () => {
    it("ignores events without key field", () => {
      const h = makeHandler("F5");
      h.handleEvent({ eventType: "KeyPress" }); // no key
      h.handleEvent({ eventType: "KeyPress", key: undefined });
      vi.advanceTimersByTime(500);

      expect(onStart).not.toHaveBeenCalled();
    });
  });

  describe("non-keyboard event types", () => {
    it("ignores MouseMove and other types", () => {
      const h = makeHandler("F5");
      h.handleEvent({ eventType: "MouseMove", key: "F5" });
      h.handleEvent({ eventType: "ButtonPress", key: "F5" });
      vi.advanceTimersByTime(500);

      expect(onStart).not.toHaveBeenCalled();
    });
  });
});

describe("createLongPressHandlerFromHotkey", () => {
  it("returns null for invalid hotkey", () => {
    expect(createLongPressHandlerFromHotkey("", 400, { onStart: vi.fn(), onStop: vi.fn() })).toBeNull();
    expect(createLongPressHandlerFromHotkey("Shift", 400, { onStart: vi.fn(), onStop: vi.fn() })).toBeNull();
  });

  it("returns handler for valid hotkey", () => {
    const h = createLongPressHandlerFromHotkey("F5", 400, { onStart: vi.fn(), onStop: vi.fn() });
    expect(h).not.toBeNull();
    expect(h!.handleEvent).toBeTypeOf("function");
    expect(h!.cleanup).toBeTypeOf("function");
  });
});
