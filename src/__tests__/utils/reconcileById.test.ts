import { describe, expect, it } from "vitest";
import { reconcileById } from "../../utils/reconcileById";

type Row = { id: string; n: number; tags: string[] };

const rows = (): Row[] => [
	{ id: "a", n: 1, tags: ["x"] },
	{ id: "b", n: 2, tags: ["y"] },
	{ id: "c", n: 3, tags: [] },
];

describe("reconcileById", () => {
	// catches: the helper hides a real change or reorders rows
	it("changed_row_updates_only_that_row", () => {
		const prev = rows();
		const next = rows();
		next[1] = { id: "b", n: 2, tags: ["y", "z"] };

		const out = reconcileById(next, prev, (r) => r.id);

		expect(out).not.toBe(prev);
		expect(out[0]).toBe(prev[0]);
		expect(out[1]).toBe(next[1]);
		expect(out[1].tags).toEqual(["y", "z"]);
		expect(out[2]).toBe(prev[2]);
	});

	// catches: positional reuse hands back the old order after two rows swap places
	it("returns the new order when unchanged rows swap places", () => {
		const prev = rows();
		const next = [rows()[1], rows()[0], rows()[2]];

		const out = reconcileById(next, prev, (r) => r.id);

		expect(out).not.toBe(prev);
		expect(out.map((r) => r.id)).toEqual(["b", "a", "c"]);
		expect(out[0]).toBe(prev[1]);
		expect(out[1]).toBe(prev[0]);
	});

	// catches: an identical snapshot still produces a new array, so the list re-renders
	it("returns prev itself when nothing changed", () => {
		const prev = rows();
		expect(reconcileById(rows(), prev, (r) => r.id)).toBe(prev);
	});

	// catches: a removed or added row leaves the old array in place
	it("reflects added and removed rows", () => {
		const prev = rows();
		const next = [rows()[0], { id: "d", n: 4, tags: [] }];

		const out = reconcileById(next, prev, (r) => r.id);

		expect(out.map((r) => r.id)).toEqual(["a", "d"]);
		expect(out[0]).toBe(prev[0]);
	});
});
