import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { installTouchHandlers, type TouchHandlerOptions } from "../components/Terminal/canvasTerminalTouch";

function makeTouch(x: number, y: number, id = 0): Touch {
  return { identifier: id, clientX: x, clientY: y, screenX: x, screenY: y, pageX: x, pageY: y, radiusX: 1, radiusY: 1, rotationAngle: 0, force: 1, target: document.body } as Touch;
}

function touchEvent(type: string, touches: Touch[], changedTouches = touches): TouchEvent {
  return new TouchEvent(type, {
    bubbles: true,
    cancelable: true,
    touches: touches as unknown as TouchList,
    changedTouches: changedTouches as unknown as TouchList,
    targetTouches: touches as unknown as TouchList,
  } as unknown as TouchEventInit);
}

describe("installTouchHandlers", () => {
  let canvas: HTMLCanvasElement;
  let textarea: HTMLTextAreaElement;
  let opts: { [K in keyof TouchHandlerOptions]: ReturnType<typeof vi.fn> };
  let cleanup: () => void;

  beforeEach(() => {
    canvas = document.createElement("canvas");
    textarea = document.createElement("textarea");
    document.body.appendChild(canvas);
    document.body.appendChild(textarea);

    opts = {
      onScroll: vi.fn(),
      onInput: vi.fn(),
      onFocus: vi.fn(),
      onFontSizeChange: vi.fn(),
      onSelectionMode: vi.fn(),
    };

    cleanup = installTouchHandlers(canvas, textarea, opts as unknown as TouchHandlerOptions);
  });

  afterEach(() => {
    cleanup();
    canvas.remove();
    textarea.remove();
  });

  describe("tap to focus", () => {
    it("tap calls onFocus and focuses textarea", () => {
      const focusSpy = vi.spyOn(textarea, "focus");
      const t = makeTouch(100, 100);
      canvas.dispatchEvent(touchEvent("touchstart", [t]));
      canvas.dispatchEvent(touchEvent("touchend", [], [t]));
      expect(opts.onFocus).toHaveBeenCalledOnce();
      expect(focusSpy).toHaveBeenCalled();
    });
  });

  describe("single-finger swipe scrolls", () => {
    it("swipe down scrolls positively", () => {
      const t1 = makeTouch(100, 100);
      const t2 = makeTouch(100, 140); // moved down 40px
      canvas.dispatchEvent(touchEvent("touchstart", [t1]));
      canvas.dispatchEvent(touchEvent("touchmove", [t2], [t2]));
      expect(opts.onScroll).toHaveBeenCalledWith(expect.any(Number));
      const delta = opts.onScroll.mock.calls[0][0] as number;
      expect(delta).toBeGreaterThan(0);
    });

    it("swipe up scrolls negatively", () => {
      const t1 = makeTouch(100, 140);
      const t2 = makeTouch(100, 100); // moved up 40px
      canvas.dispatchEvent(touchEvent("touchstart", [t1]));
      canvas.dispatchEvent(touchEvent("touchmove", [t2], [t2]));
      expect(opts.onScroll).toHaveBeenCalledWith(expect.any(Number));
      const delta = opts.onScroll.mock.calls[0][0] as number;
      expect(delta).toBeLessThan(0);
    });
  });

  describe("long press triggers selection mode", () => {
    it("600ms hold calls onSelectionMode", async () => {
      vi.useFakeTimers();
      const t = makeTouch(100, 100);
      canvas.dispatchEvent(touchEvent("touchstart", [t]));
      vi.advanceTimersByTime(620);
      expect(opts.onSelectionMode).toHaveBeenCalledOnce();
      canvas.dispatchEvent(touchEvent("touchend", [], [t]));
      vi.useRealTimers();
    });

    it("movement before 600ms cancels long press", () => {
      vi.useFakeTimers();
      const t1 = makeTouch(100, 100);
      const t2 = makeTouch(100, 115); // > 10px threshold
      canvas.dispatchEvent(touchEvent("touchstart", [t1]));
      canvas.dispatchEvent(touchEvent("touchmove", [t2], [t2]));
      vi.advanceTimersByTime(700);
      expect(opts.onSelectionMode).not.toHaveBeenCalled();
      vi.useRealTimers();
    });
  });

  describe("pinch zoom adjusts font size", () => {
    it("pinch out (fingers moving apart) calls onFontSizeChange with positive delta", () => {
      const t1a = makeTouch(100, 100, 0);
      const t2a = makeTouch(200, 100, 1);
      canvas.dispatchEvent(touchEvent("touchstart", [t1a, t2a]));

      const t1b = makeTouch(80, 100, 0);   // moved apart
      const t2b = makeTouch(220, 100, 1);
      canvas.dispatchEvent(touchEvent("touchmove", [t1b, t2b], [t1b, t2b]));

      expect(opts.onFontSizeChange).toHaveBeenCalled();
      const delta = opts.onFontSizeChange.mock.calls[0][0] as number;
      expect(delta).toBeGreaterThan(0);
    });

    it("pinch in (fingers moving together) calls onFontSizeChange with negative delta", () => {
      const t1a = makeTouch(80, 100, 0);
      const t2a = makeTouch(220, 100, 1);
      canvas.dispatchEvent(touchEvent("touchstart", [t1a, t2a]));

      const t1b = makeTouch(100, 100, 0);  // moved together
      const t2b = makeTouch(200, 100, 1);
      canvas.dispatchEvent(touchEvent("touchmove", [t1b, t2b], [t1b, t2b]));

      expect(opts.onFontSizeChange).toHaveBeenCalled();
      const delta = opts.onFontSizeChange.mock.calls[0][0] as number;
      expect(delta).toBeLessThan(0);
    });
  });

  describe("textarea input forwarding", () => {
    it("input event on textarea calls onInput", () => {
      textarea.value = "a";
      textarea.dispatchEvent(new InputEvent("input", { bubbles: true }));
      expect(opts.onInput).toHaveBeenCalledWith("a");
    });

    it("textarea is cleared after input", () => {
      textarea.value = "hello";
      textarea.dispatchEvent(new InputEvent("input", { bubbles: true }));
      expect(textarea.value).toBe("");
    });
  });

  describe("cleanup", () => {
    it("cleanup removes all listeners", () => {
      cleanup();
      const focusSpy = vi.spyOn(textarea, "focus");
      const t = makeTouch(100, 100);
      canvas.dispatchEvent(touchEvent("touchstart", [t]));
      canvas.dispatchEvent(touchEvent("touchend", [], [t]));
      expect(opts.onFocus).not.toHaveBeenCalled();
      expect(focusSpy).not.toHaveBeenCalled();
      // re-install so afterEach cleanup() doesn't throw
      cleanup = installTouchHandlers(canvas, textarea, opts as unknown as TouchHandlerOptions);
    });
  });
});
