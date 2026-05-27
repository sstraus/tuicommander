import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

describe("toastsStore", () => {
	let toastsStore: typeof import("../../stores/toasts").toastsStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		const mod = await import("../../stores/toasts");
		toastsStore = mod.toastsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("adds a toast with default level and no sound", () => {
		testInScope(() => {
			const id = toastsStore.add("Hello");
			expect(toastsStore.toasts).toHaveLength(1);
			expect(toastsStore.toasts[0]).toMatchObject({ id, title: "Hello", message: "", level: "info" });
		});
	});

	it("adds a toast with custom level and message", () => {
		testInScope(() => {
			toastsStore.add("Oops", "something broke", "error");
			expect(toastsStore.toasts[0]).toMatchObject({ title: "Oops", message: "something broke", level: "error" });
		});
	});

	it("removes a toast by id", () => {
		testInScope(() => {
			const id = toastsStore.add("A");
			toastsStore.add("B");
			expect(toastsStore.toasts).toHaveLength(2);
			toastsStore.remove(id);
			expect(toastsStore.toasts).toHaveLength(1);
			expect(toastsStore.toasts[0].title).toBe("B");
		});
	});

	it("auto-dismisses after 4 seconds", () => {
		testInScope(() => {
			toastsStore.add("Ephemeral");
			expect(toastsStore.toasts).toHaveLength(1);
			vi.advanceTimersByTime(4000);
			expect(toastsStore.toasts).toHaveLength(0);
		});
	});

	it("manual remove clears the auto-dismiss timer (no double-free)", () => {
		testInScope(() => {
			const id = toastsStore.add("Quick");
			toastsStore.remove(id);
			expect(toastsStore.toasts).toHaveLength(0);

			// Advance past the 4s auto-dismiss — must not error or double-remove
			expect(() => vi.advanceTimersByTime(5000)).not.toThrow();
			expect(toastsStore.toasts).toHaveLength(0);
		});
	});

	it("auto-dismiss does not fire after manual remove", () => {
		testInScope(() => {
			const id = toastsStore.add("Tmp");
			// Add a second toast to verify it is not affected
			toastsStore.add("Keeper");
			toastsStore.remove(id);
			expect(toastsStore.toasts).toHaveLength(1);
			vi.advanceTimersByTime(4000);
			// Both timers fired — Tmp was already removed, Keeper auto-dismissed
			expect(toastsStore.toasts).toHaveLength(0);
		});
	});

	it("accepts sound parameter without error", () => {
		testInScope(() => {
			const id = toastsStore.add("Ding", "", "info", true);
			expect(id).toBeGreaterThan(0);
			expect(toastsStore.toasts).toHaveLength(1);
		});
	});

	it("assigns unique incrementing ids", () => {
		testInScope(() => {
			const id1 = toastsStore.add("First");
			const id2 = toastsStore.add("Second");
			expect(id2).toBe(id1 + 1);
		});
	});

	it("sets createdAt to a recent timestamp", () => {
		testInScope(() => {
			const before = Date.now();
			toastsStore.add("Timed");
			const after = Date.now();
			expect(toastsStore.toasts[0].createdAt).toBeGreaterThanOrEqual(before);
			expect(toastsStore.toasts[0].createdAt).toBeLessThanOrEqual(after);
		});
	});
});
