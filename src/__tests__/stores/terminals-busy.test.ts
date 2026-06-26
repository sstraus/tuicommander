import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

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

	const addTerminal = (name = "T") => store.add(makeTerminal({ name }));

	describe("isBusy()", () => {
		it("returns false for terminal that was never busy", () => {
			testInScope(() => {
				const id = addTerminal();
				expect(store.isBusy(id)).toBe(false);
			});
		});

		it("returns true immediately when shellState becomes busy", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				expect(store.isBusy(id)).toBe(true);
			});
		});

		it("remains true for 2 seconds after shellState becomes idle (debounce hold)", () => {
			testInScope(() => {
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
			});
		});

		it("cancels cooldown when shellState goes back to busy", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.update(id, { shellState: "idle" });
				vi.advanceTimersByTime(1000);
				// Go busy again before cooldown expires
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(2000);
				// Should still be busy — cooldown was cancelled
				expect(store.isBusy(id)).toBe(true);
			});
		});

		it("returns false for unknown terminal ID", () => {
			testInScope(() => {
				expect(store.isBusy("nonexistent")).toBe(false);
			});
		});
	});

	describe("isAnyBusy()", () => {
		it("returns false when no terminals exist", () => {
			testInScope(() => {
				expect(store.isAnyBusy()).toBe(false);
			});
		});

		it("returns true when at least one terminal is busy", () => {
			testInScope(() => {
				const id1 = addTerminal("T1");
				addTerminal("T2");
				store.update(id1, { shellState: "busy" });
				expect(store.isAnyBusy()).toBe(true);
			});
		});

		it("returns true during debounce hold even after idle", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.update(id, { shellState: "idle" });
				expect(store.isAnyBusy()).toBe(true);
				vi.advanceTimersByTime(2000);
				expect(store.isAnyBusy()).toBe(false);
			});
		});
	});

	describe("getBusyDuration()", () => {
		it("returns 0 for terminal that was never busy", () => {
			testInScope(() => {
				const id = addTerminal();
				expect(store.getBusyDuration(id)).toBe(0);
			});
		});

		it("returns elapsed time while terminal is busy", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(3000);
				expect(store.getBusyDuration(id)).toBe(3000);
			});
		});

		it("returns total busy duration after idle (frozen at transition)", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(5000);
				store.update(id, { shellState: "idle" });
				// Duration frozen at 5000
				expect(store.getBusyDuration(id)).toBe(5000);
				vi.advanceTimersByTime(3000);
				// Still 5000 — doesn't grow after idle
				expect(store.getBusyDuration(id)).toBe(5000);
			});
		});

		it("resets when terminal goes busy again after cooldown expires", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(5000);
				store.update(id, { shellState: "idle" });
				expect(store.getBusyDuration(id)).toBe(5000);

				// Wait for cooldown to fully expire before starting new busy cycle
				vi.advanceTimersByTime(2000);

				// New busy cycle — busySinceMap resets because cooldown already fired
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(1000);
				expect(store.getBusyDuration(id)).toBe(1000);
			});
		});

		it("preserves busySince when re-entering busy during cooldown", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(5000);
				store.update(id, { shellState: "idle" });
				expect(store.getBusyDuration(id)).toBe(5000);

				// Re-enter busy within cooldown — same logical busy period
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(1000);
				// Duration continues from original start, not from re-entry
				expect(store.getBusyDuration(id)).toBe(6000);
			});
		});
	});

	describe("onBusyToIdle callback", () => {
		it("fires when terminal transitions from busy to idle after debounce", () => {
			testInScope(() => {
				const callback = vi.fn();
				store.onBusyToIdle(callback);

				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				vi.advanceTimersByTime(5000);
				store.update(id, { shellState: "idle" });

				// Callback fires after debounce period
				vi.advanceTimersByTime(2000);
				expect(callback).toHaveBeenCalledWith(id, 5000);
			});
		});

		it("does not fire if terminal goes busy again during cooldown", () => {
			testInScope(() => {
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
			});
		});

		it("does not fire for short busy periods (< 5s)", () => {
			testInScope(() => {
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
			});
		});
	});

	describe("cleanup on remove", () => {
		it("clears debounced busy state when terminal is removed", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				expect(store.isBusy(id)).toBe(true);

				store.remove(id);
				expect(store.isBusy(id)).toBe(false);
				expect(store.isAnyBusy()).toBe(false);
			});
		});
	});

	describe("awaitingInputConfident", () => {
		it("preserves confident awaitingInput on idle→busy (Ink menu survives repaint oscillation)", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.update(id, { shellState: "idle" });
				store.setAwaitingInput(id, "question", true);
				// A confident prompt (e.g. Ink "Enter to select") must survive the
				// idle→busy oscillation caused by TUI repaints (cursor blink, animation,
				// scrollbar). It clears only on real user-input, process exit, or the
				// agent→shell transition — never on a repaint-driven idle→busy.
				store.update(id, { shellState: "busy" });
				expect(store.get(id)?.awaitingInput).toBe("question");
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
			});
		});

		it("clears awaitingInput on idle→busy when confident is false", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.update(id, { shellState: "idle" });
				store.setAwaitingInput(id, "question", false);
				// Idle→busy SHOULD clear a low-confidence detection (silence-based heuristic)
				store.update(id, { shellState: "busy" });
				expect(store.get(id)?.awaitingInput).toBeNull();
			});
		});

		it("clearAwaitingInput resets confident flag", () => {
			testInScope(() => {
				const id = addTerminal();
				store.setAwaitingInput(id, "question", true);
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
				store.clearAwaitingInput(id);
				expect(store.get(id)?.awaitingInput).toBeNull();
				expect(store.get(id)?.awaitingInputConfident).toBe(false);
			});
		});

		it("clears a confident question after sustained busy with no re-detection (prompt answered, agent resumed work)", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.setAwaitingInput(id, "question", true);
				// The agent keeps producing output (busy) and the prompt is never
				// re-detected — it was answered. The badge must not stay stuck.
				vi.advanceTimersByTime(2499);
				expect(store.get(id)?.awaitingInput).toBe("question");
				vi.advanceTimersByTime(1);
				expect(store.get(id)?.awaitingInput).toBeNull();
				expect(store.get(id)?.awaitingInputConfident).toBe(false);
			});
		});

		it("keeps a confident question when the terminal is idle at the deadline (genuine static prompt)", () => {
			testInScope(() => {
				const id = addTerminal();
				// Menu shown, then the terminal goes idle waiting for the answer —
				// no output. This is a real pending prompt and must stay highlighted.
				store.update(id, { shellState: "busy" });
				store.update(id, { shellState: "idle" });
				store.setAwaitingInput(id, "question", true);
				vi.advanceTimersByTime(5000);
				expect(store.get(id)?.awaitingInput).toBe("question");
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
			});
		});

		it("refreshes the clear timer on re-detection (Ink menu repaint keeps it highlighted)", () => {
			testInScope(() => {
				const id = addTerminal();
				store.update(id, { shellState: "busy" });
				store.setAwaitingInput(id, "question", true);
				vi.advanceTimersByTime(2000);
				// Repaint re-emits the same confident question — refreshes the deadline.
				store.setAwaitingInput(id, "question", true);
				vi.advanceTimersByTime(2000);
				expect(store.get(id)?.awaitingInput).toBe("question");
				// Past the refreshed deadline with no further re-detection → cleared.
				vi.advanceTimersByTime(500);
				expect(store.get(id)?.awaitingInput).toBeNull();
			});
		});
	});
});
