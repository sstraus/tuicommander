import { beforeEach, describe, expect, it } from "vitest";
import { tabOrderingStore } from "../../stores/tabOrdering";

describe("tabOrderingStore", () => {
	beforeEach(() => {
		tabOrderingStore.clear();
	});

	describe("insert", () => {
		it("appends to end by default", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b", "c"]))).toEqual(["a", "b", "c"]);
		});

		it("inserts after specified id", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c", "a");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b", "c"]))).toEqual(["a", "c", "b"]);
		});

		it("deduplicates — inserting existing id is no-op", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("a");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b"]))).toEqual(["a", "b"]);
		});
	});

	describe("remove", () => {
		it("removes an id from the order", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c");
			tabOrderingStore.remove("b");
			expect(tabOrderingStore.getOrdered(new Set(["a", "c"]))).toEqual(["a", "c"]);
		});

		it("no-op for unknown id", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.remove("z");
			expect(tabOrderingStore.getOrdered(new Set(["a"]))).toEqual(["a"]);
		});
	});

	describe("reorder", () => {
		it("moves source before target", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c");
			tabOrderingStore.reorder("c", "a", "before");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b", "c"]))).toEqual(["c", "a", "b"]);
		});

		it("moves source after target", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c");
			tabOrderingStore.reorder("a", "b", "after");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b", "c"]))).toEqual(["b", "a", "c"]);
		});

		it("no-op when source equals target", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.reorder("a", "a", "before");
			expect(tabOrderingStore.getOrdered(new Set(["a", "b"]))).toEqual(["a", "b"]);
		});
	});

	describe("getOrdered", () => {
		it("filters to visible set only", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			tabOrderingStore.insert("c");
			expect(tabOrderingStore.getOrdered(new Set(["a", "c"]))).toEqual(["a", "c"]);
		});

		it("appends new ids not in order", () => {
			tabOrderingStore.insert("a");
			tabOrderingStore.insert("b");
			const result = tabOrderingStore.getOrdered(new Set(["a", "b", "x", "y"]));
			expect(result.slice(0, 2)).toEqual(["a", "b"]);
			expect(new Set(result.slice(2))).toEqual(new Set(["x", "y"]));
		});

		it("returns empty array for empty visible set", () => {
			tabOrderingStore.insert("a");
			expect(tabOrderingStore.getOrdered(new Set())).toEqual([]);
		});
	});
});
