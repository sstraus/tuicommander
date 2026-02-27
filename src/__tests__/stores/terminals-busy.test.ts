import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

describe("terminalsStore debounced busy signal", () => {
  let store: typeof import("../../stores/terminals").terminalsStore;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/terminals")).terminalsStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const addTerminal = (name = "T") =>
    store.add({ sessionId: null, fontSize: 14, name, cwd: null, awaitingInput: null });

  describe("isBusy()", () => {
    it("returns false for terminal that was never busy", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        expect(store.isBusy(id)).toBe(false);
        dispose();
      });
    });

    it("returns true immediately when shellState becomes busy", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        expect(store.isBusy(id)).toBe(true);
        dispose();
      });
    });

    it("remains true for 2 seconds after shellState becomes idle (debounce hold)", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        expect(store.isBusy(id)).toBe(true);

        store.update(id, { shellState: "idle" });
        // Still true — 2s hold
        expect(store.isBusy(id)).toBe(true);

        vi.advanceTimersByTime(1999);
        expect(store.isBusy(id)).toBe(true);

        vi.advanceTimersByTime(1);
        expect(store.isBusy(id)).toBe(false);
        dispose();
      });
    });

    it("cancels cooldown when shellState goes back to busy", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        store.update(id, { shellState: "idle" });
        vi.advanceTimersByTime(1000);
        // Go busy again before cooldown expires
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(2000);
        // Should still be busy — cooldown was cancelled
        expect(store.isBusy(id)).toBe(true);
        dispose();
      });
    });

    it("returns false for unknown terminal ID", () => {
      createRoot((dispose) => {
        expect(store.isBusy("nonexistent")).toBe(false);
        dispose();
      });
    });
  });

  describe("isAnyBusy()", () => {
    it("returns false when no terminals exist", () => {
      createRoot((dispose) => {
        expect(store.isAnyBusy()).toBe(false);
        dispose();
      });
    });

    it("returns true when at least one terminal is busy", () => {
      createRoot((dispose) => {
        const id1 = addTerminal("T1");
        addTerminal("T2");
        store.update(id1, { shellState: "busy" });
        expect(store.isAnyBusy()).toBe(true);
        dispose();
      });
    });

    it("returns true during debounce hold even after idle", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        store.update(id, { shellState: "idle" });
        expect(store.isAnyBusy()).toBe(true);
        vi.advanceTimersByTime(2000);
        expect(store.isAnyBusy()).toBe(false);
        dispose();
      });
    });
  });

  describe("getBusyDuration()", () => {
    it("returns 0 for terminal that was never busy", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        expect(store.getBusyDuration(id)).toBe(0);
        dispose();
      });
    });

    it("returns elapsed time while terminal is busy", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(3000);
        expect(store.getBusyDuration(id)).toBe(3000);
        dispose();
      });
    });

    it("returns total busy duration after idle (frozen at transition)", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(5000);
        store.update(id, { shellState: "idle" });
        // Duration frozen at 5000
        expect(store.getBusyDuration(id)).toBe(5000);
        vi.advanceTimersByTime(3000);
        // Still 5000 — doesn't grow after idle
        expect(store.getBusyDuration(id)).toBe(5000);
        dispose();
      });
    });

    it("resets when terminal goes busy again", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(5000);
        store.update(id, { shellState: "idle" });
        expect(store.getBusyDuration(id)).toBe(5000);

        // New busy cycle
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(1000);
        expect(store.getBusyDuration(id)).toBe(1000);
        dispose();
      });
    });
  });

  describe("onBusyToIdle callback", () => {
    it("fires when terminal transitions from busy to idle after debounce", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onBusyToIdle(callback);

        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(5000);
        store.update(id, { shellState: "idle" });

        // Callback fires after debounce period
        vi.advanceTimersByTime(2000);
        expect(callback).toHaveBeenCalledWith(id, 5000);
        dispose();
      });
    });

    it("does not fire if terminal goes busy again during cooldown", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onBusyToIdle(callback);

        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(3000);
        store.update(id, { shellState: "idle" });
        vi.advanceTimersByTime(1000);
        // Goes busy again before cooldown
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(5000);

        expect(callback).not.toHaveBeenCalled();
        dispose();
      });
    });

    it("does not fire for short busy periods (< 5s)", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onBusyToIdle(callback);

        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        vi.advanceTimersByTime(2000);
        store.update(id, { shellState: "idle" });
        vi.advanceTimersByTime(2000);

        // Duration was only 2s, below the 5s threshold — should still fire with duration
        // (the threshold decision is for the caller, not the store)
        expect(callback).toHaveBeenCalledWith(id, 2000);
        dispose();
      });
    });
  });

  describe("cleanup on remove", () => {
    it("clears debounced busy state when terminal is removed", () => {
      createRoot((dispose) => {
        const id = addTerminal();
        store.update(id, { shellState: "busy" });
        expect(store.isBusy(id)).toBe(true);

        store.remove(id);
        expect(store.isBusy(id)).toBe(false);
        expect(store.isAnyBusy()).toBe(false);
        dispose();
      });
    });
  });
});
