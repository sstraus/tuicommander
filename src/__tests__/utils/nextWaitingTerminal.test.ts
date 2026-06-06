import { describe, expect, it } from "vitest";
import { nextWaitingTerminal } from "../../utils/nextWaitingTerminal";

describe("nextWaitingTerminal", () => {
	it("returns null when nothing is waiting", () => {
		expect(nextWaitingTerminal([], "a")).toBe(null);
		expect(nextWaitingTerminal([], null)).toBe(null);
	});

	it("starts at the first waiting terminal when the active one is not waiting", () => {
		expect(nextWaitingTerminal(["b", "c"], "a")).toBe("b");
	});

	it("starts at the first waiting terminal when nothing is active", () => {
		expect(nextWaitingTerminal(["b", "c"], null)).toBe("b");
	});

	it("advances to the next waiting terminal when the active one is in the list", () => {
		expect(nextWaitingTerminal(["a", "b", "c"], "a")).toBe("b");
		expect(nextWaitingTerminal(["a", "b", "c"], "b")).toBe("c");
	});

	it("wraps from the last waiting terminal back to the first", () => {
		expect(nextWaitingTerminal(["a", "b", "c"], "c")).toBe("a");
	});

	it("re-targets the same terminal when it is the only one waiting", () => {
		expect(nextWaitingTerminal(["a"], "a")).toBe("a");
	});
});
